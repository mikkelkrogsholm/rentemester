//! Multi-year figures from observed live periods and classified archive balances, never invented years.
use crate::{cockpit::number, invalid};
use rentemester_engine::{
    Error,
    financial_statements::{self, BalanceSheet, ProfitAndLoss},
    fiscal_year::{self, Date, LabelStrategy, StartMonth},
    money::Money,
    trial_balance::{self, Line, Period},
};
use rusqlite::Transaction;
use serde_json::{Value, json};
use std::collections::BTreeMap;
// Archive balances are signed; the shared ledger reader deliberately rejects negative debit/credit.
fn signed_archive_money(value: rusqlite::types::ValueRef<'_>) -> Result<Money, Error> {
    use rusqlite::types::ValueRef;
    match value {
        ValueRef::Integer(n) if n.unsigned_abs() <= 9_007_199_254_740_991 => {
            Money::parse(&n.to_string())
        }
        ValueRef::Real(n) if n.is_finite() => Money::parse(ryu_js::Buffer::new().format_finite(n)),
        _ => Err(invalid("unsupported archive amount storage or precision")),
    }
}
fn ratio(n: &Money, d: &Money) -> Result<Value, Error> {
    if d == &Money::default() {
        return Ok(Value::Null);
    }
    let n: f64 = n.decimal().parse().map_err(invalid)?;
    let d: f64 = d.decimal().parse().map_err(invalid)?;
    serde_json::Number::from_f64(n / d)
        .map(Value::Number)
        .ok_or_else(|| invalid("multi-year ratio out of range"))
}
fn row(year: &str, source: &str, pl: ProfitAndLoss, bs: BalanceSheet) -> Result<Value, Error> {
    Ok(
        json!({"year":year,"source":source,"omsaetning":number(&pl.income.total)?,"udgifter":number(&pl.expense.total)?,"resultat":number(&pl.result)?,"balancesum":number(&bs.assets.total)?,"egenkapital":number(&bs.equity.total)?,"bruttomargin":ratio(&pl.result,&pl.income.total)?,"egenkapitalandel":ratio(&bs.equity.total,&bs.assets.total)?,"balanced":bs.balanced}),
    )
}
pub fn read(tx: &Transaction<'_>, slug: &str, today: &str) -> Result<Value, Error> {
    crate::validate(tx)?;
    Date::parse(today)?;
    let company = crate::company_setup::profile(tx)?;
    let start = StartMonth::new(
        company["fiscalYearStartMonth"]
            .as_i64()
            .ok_or_else(|| invalid("fiscal start month"))? as i32,
    )?;
    let strategy = LabelStrategy::try_from(
        company["fiscalYearLabelStrategy"]
            .as_str()
            .ok_or_else(|| invalid("fiscal label strategy"))?,
    )?;
    let current = fiscal_year::for_date(Date::parse(today)?, start, strategy)?.display_label;
    let mut observed = BTreeMap::new();
    let mut q=tx.prepare("SELECT DISTINCT transaction_date FROM journal_entries WHERE status IN ('posted','reversed') ORDER BY transaction_date").map_err(invalid)?;
    for date in q
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(invalid)?
    {
        let period = fiscal_year::for_date(Date::parse(&date.map_err(invalid)?)?, start, strategy)?;
        observed.insert(period.display_label.clone(), period);
    }
    let mut years = BTreeMap::new();
    for (label, period) in observed {
        let report = crate::trial_balance_owned(
            tx,
            Period::new(&period.start.to_string(), &period.end.to_string())?,
        )?;
        let balance =
            crate::trial_balance_owned(tx, Period::new("0000-01-01", &period.end.to_string())?)?;
        let mut value = row(
            &label,
            "live",
            financial_statements::profit_and_loss(&report),
            financial_statements::balance_sheet(&balance),
        )?;
        value["periodStart"] = json!(period.start.to_string());
        value["periodEnd"] = json!(period.end.to_string());
        years.insert(label, value);
    }
    let mut q=tx.prepare("SELECT id,fiscal_year FROM import_archive_years WHERE id IN (SELECT MAX(id) FROM import_archive_years GROUP BY fiscal_year) ORDER BY fiscal_year").map_err(invalid)?;
    let archives = q
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i32>(1)?)))
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    let mut unavailable = vec![];
    for (id, year) in archives {
        let label = format!("{year:04}");
        if years.contains_key(&label) {
            continue;
        }
        let mut q=tx.prepare("SELECT b.account_no,COALESCE(a.name,b.account_name,b.account_no),a.type,a.normal_balance,b.amount FROM import_archive_balances b LEFT JOIN accounts a ON a.account_no=b.account_no WHERE b.archive_year_id=?1 ORDER BY b.line_no,b.id").map_err(invalid)?;
        let mut rs = q.query([id]).map_err(invalid)?;
        let mut lines = vec![];
        let mut missing = false;
        while let Some(r) = rs.next().map_err(invalid)? {
            let kind: Option<String> = r.get(2).map_err(invalid)?;
            let normal: Option<String> = r.get(3).map_err(invalid)?;
            let (Some(kind), Some(normal)) = (kind, normal) else {
                missing = true;
                continue;
            };
            if !["income", "expense", "asset", "liability", "equity", "vat"]
                .contains(&kind.as_str())
                || !["debit", "credit"].contains(&normal.as_str())
            {
                missing = true;
                continue;
            }
            let amount = signed_archive_money(r.get_ref(4).map_err(invalid)?)?;
            let (debit, credit) = if amount.is_negative() {
                (Money::default(), Money::default().subtract(&amount))
            } else {
                (amount, Money::default())
            };
            lines.push(Line {
                account_no: r.get(0).map_err(invalid)?,
                name: r.get(1).map_err(invalid)?,
                kind,
                normal_balance: normal,
                entry_id: id,
                debit,
                credit,
            });
        }
        let reason = if missing {
            Some("Arkivets konti mangler en entydig klassifikation i kontoplanen.")
        } else if lines.is_empty() {
            Some("Arkivet indeholder ingen saldobalance at beregne tallene fra.")
        } else if !(0..=9999).contains(&year) {
            Some("Arkivets regnskabsår ligger uden for det understøttede datoområde.")
        } else {
            None
        };
        if let Some(reason) = reason {
            unavailable.push(json!({"year":label,"source":"archive","reason":reason}));
            continue;
        }
        // Archive amounts already describe the named year's closing trial balance. Feed its signed
        // account balances to the same native reducers; no archive figures enter the live ledger.
        let report = trial_balance::build(
            Period::new(&format!("{year:04}-01-01"), &format!("{year:04}-12-31"))?,
            lines,
        );
        let mut value = row(
            &label,
            "archive",
            financial_statements::profit_and_loss(&report),
            financial_statements::balance_sheet(&report),
        )?;
        value["periodStart"] = Value::Null;
        value["periodEnd"] = Value::Null;
        years.insert(label, value);
    }
    let current_year = years
        .get(&current)
        .filter(|y| y["source"] == "live")
        .map(|_| json!(current))
        .unwrap_or(Value::Null);
    Ok(
        json!({"slug":slug,"company":company,"years":years.into_values().collect::<Vec<_>>(),"currentYear":current_year,"asOfDate":today,"archiveDetailsAvailable":false,"unavailableYears":unavailable}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (rusqlite::Connection, rusqlite::Connection) {
        let mut c = rusqlite::Connection::open_in_memory().unwrap();
        let mut l = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic MultiYear").unwrap();
        l.execute_batch("INSERT INTO accounts(id,account_no,name,type,normal_balance) VALUES(1,'A','Synthetic assets','asset','debit'),(2,'I','Synthetic income','income','credit'),(3,'X','Synthetic expense','expense','debit'),(4,'VI','Synthetic input VAT','vat','debit'),(5,'VO','Synthetic output VAT','vat','credit'),(6,'E','Synthetic equity','equity','credit');").unwrap();
        (c, l)
    }
    #[test]
    fn shifted_year_signs_cumulative_equity_and_live_report_parity_without_fake_empty_years() {
        let (_c, mut l) = fixture();
        l.execute_batch("UPDATE companies SET fiscal_year_start_month=7,fiscal_year_label_strategy='span' WHERE id=1;INSERT INTO journal_entries(id,entry_no,transaction_date,text,rule_version,entry_hash) VALUES(1,'SYN-1','2024-07-01','Synthetic income','fixture','synthetic-1'),(2,'SYN-2','2025-06-30','Synthetic expense','fixture','synthetic-2'),(3,'SYN-3','2025-07-01','Synthetic negative income','fixture','synthetic-3'),(4,'SYN-4','2026-06-30','Synthetic expense','fixture','synthetic-4');INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount) VALUES(1,1,200.02,0),(1,2,0,200.02),(2,3,40.01,0),(2,1,0,40.01),(3,2,100.02,0),(3,1,0,100.02),(4,3,25.02,0),(4,1,0,25.02);").unwrap();
        let before = l.total_changes();
        let tx = l.transaction().unwrap();
        let result = read(&tx, "synthetic", "2026-10-19").unwrap();
        assert_eq!(result["years"].as_array().unwrap().len(), 2);
        assert!(result["currentYear"].is_null());
        let first = &result["years"][0];
        let second = &result["years"][1];
        assert_eq!(first["year"], "2024/25");
        assert_eq!(first["periodStart"], "2024-07-01");
        assert_eq!(first["periodEnd"], "2025-06-30");
        assert_eq!(first["resultat"].as_f64(), Some(160.01));
        assert_eq!(second["year"], "2025/26");
        assert_eq!(second["resultat"].as_f64(), Some(-125.04));
        assert_eq!(second["balancesum"].as_f64(), Some(34.97));
        assert_eq!(second["egenkapital"].as_f64(), Some(34.97));
        assert_eq!(second["egenkapitalandel"].as_f64(), Some(1.0));
        for row in result["years"].as_array().unwrap() {
            let reports =
                crate::cockpit::read(&tx, "synthetic", "2026-10-19", row["year"].as_str(), None)
                    .unwrap();
            assert_eq!(row["omsaetning"], reports.income_statement["totalIncome"]);
            assert_eq!(row["udgifter"], reports.income_statement["totalExpense"]);
            assert_eq!(row["resultat"], reports.income_statement["result"]);
            assert_eq!(row["balancesum"], reports.balance["totalAssets"]);
            assert_eq!(row["egenkapital"], reports.balance["equity"]["total"]);
        }
        tx.commit().unwrap();
        assert_eq!(l.total_changes(), before);
    }
    #[test]
    fn archived_signed_balances_share_classification_and_missing_evidence_is_explicit() {
        let (_c, mut l) = fixture();
        l.execute_batch("INSERT INTO import_archive_years(id,source_system,fiscal_year,balance_count) VALUES(1,'synthetic',2022,6),(2,'synthetic',2021,0),(3,'synthetic',2020,1);INSERT INTO import_archive_balances(archive_year_id,line_no,account_no,amount) VALUES(1,1,'A',100),(1,2,'I',-80),(1,3,'X',20),(1,4,'VI',5),(1,5,'VO',-5),(1,6,'E',-40),(3,1,'UNCLASSIFIED',7);").unwrap();
        let tx = l.transaction().unwrap();
        let result = read(&tx, "synthetic", "2026-10-19").unwrap();
        let rows = result["years"].as_array().unwrap();
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row["source"], "archive");
        assert_eq!(row["year"], "2022");
        assert_eq!(row["omsaetning"].as_f64(), Some(80.0));
        assert_eq!(row["udgifter"].as_f64(), Some(20.0));
        assert_eq!(row["resultat"].as_f64(), Some(60.0));
        assert_eq!(row["balancesum"].as_f64(), Some(105.0));
        assert_eq!(row["egenkapital"].as_f64(), Some(100.0));
        assert_eq!(row["balanced"], true);
        assert_eq!(result["unavailableYears"].as_array().unwrap().len(), 2);
        assert!(result["currentYear"].is_null());
    }
    #[test]
    fn empty_company_has_no_made_up_year_and_live_overlap_wins_archive() {
        let (_c, mut l) = fixture();
        {
            let tx = l.transaction().unwrap();
            let result = read(&tx, "synthetic", "2026-10-19").unwrap();
            assert_eq!(result["years"], json!([]));
        }
        l.execute_batch("INSERT INTO import_archive_years(source_system,fiscal_year,balance_count) VALUES('synthetic',2026,0);INSERT INTO journal_entries(id,entry_no,transaction_date,text,rule_version,entry_hash) VALUES(1,'SYN-1','2026-10-19','Synthetic expense','fixture','synthetic-1');INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount) VALUES(1,3,10.01,0),(1,1,0,10.01);").unwrap();
        let tx = l.transaction().unwrap();
        let result = read(&tx, "synthetic", "2026-10-19").unwrap();
        assert_eq!(result["years"].as_array().unwrap().len(), 1);
        assert_eq!(result["years"][0]["source"], "live");
        assert_eq!(result["currentYear"], "2026");
        assert!(result["years"][0]["bruttomargin"].is_null());
        assert_eq!(result["unavailableYears"], json!([]));
    }
}
