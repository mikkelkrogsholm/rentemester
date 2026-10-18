//! Browser projections read from an already owned, authorized company transaction.
use crate::{invalid, legacy_money};
use rentemester_engine::{
    Error,
    fiscal_year::{self, Date, LabelStrategy, StartMonth},
    money::Money,
    trial_balance::Period,
};
use rusqlite::Transaction;
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub struct Snapshot {
    pub journal: Value,
    pub trial_balance: Value,
    pub fiscal_years: Value,
    pub income_statement: Value,
    pub balance: Value,
}
pub(crate) fn number(money: &Money) -> Result<Value, Error> {
    let n: f64 = money.decimal().parse().map_err(invalid)?;
    serde_json::Number::from_f64(n)
        .map(Value::Number)
        .ok_or_else(|| invalid("amount cannot be displayed"))
}
/// Calculations remain exact Money; only the existing cockpit's numeric display wire uses f64.
pub fn read(
    tx: &Transaction<'_>,
    slug: &str,
    today: &str,
    selected: Option<&str>,
    account: Option<&str>,
) -> Result<Snapshot, Error> {
    crate::validate(tx)?;
    let (name, cvr, country, currency, month, strategy): (String, Option<String>, String, String, i32, String) = tx.query_row(
        "SELECT name,cvr,country,currency,fiscal_year_start_month,fiscal_year_label_strategy FROM companies WHERE id=1", [],
        |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).map_err(invalid)?;
    let start = StartMonth::new(month)?;
    let strategy_value = LabelStrategy::try_from(strategy.as_str())?;
    let mut years = BTreeMap::new();
    let current = fiscal_year::for_date(Date::parse(today)?, start, strategy_value)?;
    let current_label = current.display_label.clone();
    years.insert(current.display_label.clone(), current);
    let mut statement = tx
        .prepare("SELECT DISTINCT transaction_date FROM journal_entries ORDER BY transaction_date")
        .map_err(invalid)?;
    let dates = statement
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(invalid)?;
    for date in dates {
        let year =
            fiscal_year::for_date(Date::parse(&date.map_err(invalid)?)?, start, strategy_value)?;
        years.insert(year.display_label.clone(), year);
    }
    let label = selected.unwrap_or(&current_label);
    let year = years
        .get(label)
        .ok_or_else(|| Error::new("invalid_year", "unknown live fiscal year"))?;
    let period = Period::new(&year.start.to_string(), &year.end.to_string())?;
    let fiscal_years: Vec<Value> = years.values().map(|y| json!({"label":y.display_label,"start":y.start.to_string(),"end":y.end.to_string(),"source":"live"})).collect();
    let company = json!({"name":name,"cvr":cvr,"country":country,"currency":currency,"fiscalYearStartMonth":month,"fiscalYearLabelStrategy":strategy});
    let mut base = json!({"slug":slug,"selectedYear":label,"archived":false,"archivedSource":null,"company":company,"fiscalYears":fiscal_years,"periodStart":period.start,"periodEnd":period.end});
    let report = crate::trial_balance_owned(tx, period.clone())?;
    let rows: Vec<Value> = report.accounts.iter().map(|a| Ok(json!({"accountNo":a.account_no,"name":a.name,"type":a.kind,"debit":number(&a.debit)?,"credit":number(&a.credit)?,"balance":number(&a.balance)?}))).collect::<Result<_,Error>>()?;
    let mut trial_balance = base.clone();
    trial_balance["rows"] = json!(rows);
    trial_balance["totalDebit"] = number(&report.total_debit)?;
    trial_balance["totalCredit"] = number(&report.total_credit)?;
    trial_balance["balanced"] = json!(report.balanced);
    let prior_year = if year.start_year > 0 {
        Some(fiscal_year::for_date(
            Date::new(year.start_year - 1, month as u8, 1)?,
            start,
            strategy_value,
        )?)
    } else {
        None
    };
    let current_pnl = rentemester_engine::financial_statements::profit_and_loss(&report);
    let prior_report = prior_year
        .as_ref()
        .map(|p| {
            crate::trial_balance_owned(tx, Period::new(&p.start.to_string(), &p.end.to_string())?)
        })
        .transpose()?;
    let prior_pnl = prior_report
        .as_ref()
        .map(rentemester_engine::financial_statements::profit_and_loss);
    let mut income_statement = base.clone();
    income_statement["priorYearLabel"] = prior_year
        .as_ref()
        .map(|p| json!(p.display_label))
        .unwrap_or(Value::Null);
    income_statement["income"] = statement_lines(
        &current_pnl.income,
        prior_pnl.as_ref().map(|p| &p.income),
        false,
    )?;
    income_statement["expense"] = statement_lines(
        &current_pnl.expense,
        prior_pnl.as_ref().map(|p| &p.expense),
        false,
    )?;
    income_statement["totalIncome"] = number(&current_pnl.income.total)?;
    income_statement["totalExpense"] = number(&current_pnl.expense.total)?;
    income_statement["priorTotalIncome"] = prior_pnl
        .as_ref()
        .map(|p| number(&p.income.total))
        .transpose()?
        .unwrap_or(json!(0));
    income_statement["priorTotalExpense"] = prior_pnl
        .as_ref()
        .map(|p| number(&p.expense.total))
        .transpose()?
        .unwrap_or(json!(0));
    income_statement["result"] = number(&current_pnl.result)?;
    income_statement["priorResult"] = prior_pnl
        .as_ref()
        .map(|p| number(&p.result))
        .transpose()?
        .unwrap_or(json!(0));
    let cumulative = crate::trial_balance_owned(tx, Period::new("0000-01-01", &period.end)?)?;
    let sheet = rentemester_engine::financial_statements::balance_sheet(&cumulative);
    let prior_sheet = prior_year
        .as_ref()
        .filter(|p| years.contains_key(&p.display_label))
        .map(|p| {
            Ok(rentemester_engine::financial_statements::balance_sheet(
                &crate::trial_balance_owned(tx, Period::new("0000-01-01", &p.end.to_string())?)?,
            ))
        })
        .transpose()?;
    let mut balance = base.clone();
    balance["asOfDate"] = json!(period.end);
    balance["priorAsOfDate"] = prior_year
        .as_ref()
        .map(|p| json!(p.end.to_string()))
        .unwrap_or(Value::Null);
    balance["assets"] = balance_section(&sheet.assets, prior_sheet.as_ref().map(|p| &p.assets))?;
    balance["liabilities"] = balance_section(
        &sheet.liabilities,
        prior_sheet.as_ref().map(|p| &p.liabilities),
    )?;
    balance["equity"] = balance_section(&sheet.equity, prior_sheet.as_ref().map(|p| &p.equity))?;
    balance["periodResult"] = number(&sheet.period_result)?;
    balance["totalAssets"] = number(&sheet.assets.total)?;
    balance["totalLiabilitiesAndEquity"] = number(&sheet.total_liabilities_and_equity)?;
    balance["priorTotalLiabilitiesAndEquity"] = prior_sheet
        .as_ref()
        .map(|p| number(&p.total_liabilities_and_equity))
        .transpose()?
        .unwrap_or(Value::Null);
    balance["balanced"] = json!(sheet.balanced);
    let mut accounts = Vec::new();
    let mut statement = tx.prepare("SELECT account_no,name,type,normal_balance,active,allow_direct_posting FROM accounts ORDER BY account_no").map_err(invalid)?;
    let mut records = statement.query([]).map_err(invalid)?;
    while let Some(r) = records.next().map_err(invalid)? {
        accounts.push(json!({"accountNo":r.get::<_,String>(0).map_err(invalid)?,"name":r.get::<_,String>(1).map_err(invalid)?,"type":r.get::<_,String>(2).map_err(invalid)?,"normalBalance":r.get::<_,String>(3).map_err(invalid)?,"active":r.get::<_,i64>(4).map_err(invalid)? != 0,"allowDirectPosting":r.get::<_,i64>(5).map_err(invalid)? != 0}));
    }
    let filter = if let Some(no) = account {
        Some(
            accounts
                .iter()
                .find(|a| a["accountNo"] == no)
                .ok_or_else(|| Error::new("invalid_account", "unknown account"))?,
        )
    } else {
        None
    };
    let mut entries = Vec::new();
    let mut statement = tx.prepare("SELECT je.id,je.entry_no,je.transaction_date,je.text,je.document_id,d.document_no,je.reversal_of_entry_id,original.entry_no,(SELECT rv.id FROM journal_entries rv WHERE rv.reversal_of_entry_id=je.id ORDER BY rv.id LIMIT 1),(SELECT rv.entry_no FROM journal_entries rv WHERE rv.reversal_of_entry_id=je.id ORDER BY rv.id LIMIT 1) FROM journal_entries je LEFT JOIN documents d ON d.id=je.document_id LEFT JOIN journal_entries original ON original.id=je.reversal_of_entry_id WHERE je.transaction_date>=?1 AND je.transaction_date<=?2 ORDER BY je.transaction_date DESC,je.id DESC").map_err(invalid)?;
    let mut records = statement
        .query([&period.start, &period.end])
        .map_err(invalid)?;
    while let Some(r) = records.next().map_err(invalid)? {
        let id: i64 = r.get(0).map_err(invalid)?;
        crate::safe_entry_id(id)?;
        let mut line_statement = tx.prepare("SELECT jl.id,a.account_no,a.name,jl.debit_amount,jl.credit_amount,jl.text FROM journal_lines jl JOIN accounts a ON a.id=jl.account_id WHERE jl.journal_entry_id=?1 ORDER BY jl.id").map_err(invalid)?;
        let mut lines = line_statement.query([id]).map_err(invalid)?;
        let mut wire = Vec::new();
        let mut total = Money::default();
        while let Some(line) = lines.next().map_err(invalid)? {
            let debit = legacy_money(line.get_ref(3).map_err(invalid)?)?;
            let credit = legacy_money(line.get_ref(4).map_err(invalid)?)?;
            total.add(&debit);
            wire.push(json!({"journalLineId":line.get::<_,i64>(0).map_err(invalid)?,"accountNo":line.get::<_,String>(1).map_err(invalid)?,"accountName":line.get::<_,String>(2).map_err(invalid)?,"debit":number(&debit)?,"credit":number(&credit)?,"text":line.get::<_,Option<String>>(5).map_err(invalid)?}));
        }
        if account.is_some_and(|no| !wire.iter().any(|l| l["accountNo"] == no)) {
            continue;
        }
        entries.push(json!({"id":id,"entryNo":r.get::<_,String>(1).map_err(invalid)?,"date":r.get::<_,String>(2).map_err(invalid)?,"text":r.get::<_,String>(3).map_err(invalid)?,"documentId":r.get::<_,Option<i64>>(4).map_err(invalid)?,"documentNo":r.get::<_,Option<String>>(5).map_err(invalid)?,"total":number(&total)?,"lines":wire,"reversalOfEntryId":r.get::<_,Option<i64>>(6).map_err(invalid)?,"reversalOfEntryNo":r.get::<_,Option<String>>(7).map_err(invalid)?,"reversedByEntryId":r.get::<_,Option<i64>>(8).map_err(invalid)?,"reversedByEntryNo":r.get::<_,Option<String>>(9).map_err(invalid)?}));
    }
    base["entries"] = json!(entries);
    base["accountFilter"] = filter
        .map(|a| json!({"accountNo":a["accountNo"],"name":a["name"]}))
        .unwrap_or(Value::Null);
    base["nativePosting"] =
        json!({"ruleVersion":crate::manual_journal_rule_version()?,"accounts":accounts});
    Ok(Snapshot {
        journal: base,
        income_statement,
        balance,
        trial_balance,
        fiscal_years: json!({"slug":slug,"years":fiscal_years}),
    })
}

fn statement_lines(
    current: &rentemester_engine::financial_statements::Section,
    prior: Option<&rentemester_engine::financial_statements::Section>,
    null_missing: bool,
) -> Result<Value, Error> {
    current.lines.iter().map(|line| {
        let previous=prior.and_then(|p|p.lines.iter().find(|l|l.account_no==line.account_no));
        let previous=if null_missing && prior.is_none() {Value::Null} else {previous.map(|l|number(&l.amount)).transpose()?.unwrap_or(json!(0))};
        Ok(json!({"accountNo":line.account_no,"name":line.name,"amount":number(&line.amount)?,"priorAmount":previous}))
    }).collect::<Result<Vec<_>,Error>>().map(|rows|json!(rows))
}
fn balance_section(
    current: &rentemester_engine::financial_statements::Section,
    prior: Option<&rentemester_engine::financial_statements::Section>,
) -> Result<Value, Error> {
    Ok(
        json!({"lines":statement_lines(current,prior,true)?,"total":number(&current.total)?,"priorTotal":prior.map(|p|number(&p.total)).transpose()?.unwrap_or(Value::Null)}),
    )
}

#[cfg(test)]
mod tests {
    fn fixture(month: i32, strategy: &str) -> (tempfile::TempDir, rusqlite::Connection) {
        let root = tempfile::tempdir().unwrap();
        let mut control = rusqlite::Connection::open(root.path().join("control.sqlite")).unwrap();
        let mut company = rusqlite::Connection::open(root.path().join("company.sqlite")).unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut company, "Synthetic Reports")
            .unwrap();
        company.execute("UPDATE companies SET fiscal_year_start_month=?1,fiscal_year_label_strategy=?2 WHERE id=1",rusqlite::params![month,strategy]).unwrap();
        company.execute_batch("INSERT INTO accounts(id,account_no,name,type,normal_balance) VALUES(1,'A','Synthetic cash','asset','debit'),(2,'I','Synthetic revenue','income','credit'),(3,'X','Synthetic expense','expense','debit'); INSERT INTO journal_entries(id,entry_no,transaction_date,text,rule_version,entry_hash) VALUES(1,'S-1','2025-06-01','Synthetic prior sale','fixture','synthetic-hash-1'),(2,'S-2','2026-06-01','Synthetic current expense','fixture','synthetic-hash-2'); INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount) VALUES(1,1,100,0),(1,2,0,100),(2,3,40,0),(2,1,0,40);").unwrap();
        (root, company)
    }
    #[test]
    fn live_reports_preserve_prior_year_and_cumulative_result() {
        let (_root, mut company) = fixture(1, "end-year");
        let tx = company.transaction().unwrap();
        let result = super::read(&tx, "synthetic", "2026-10-18", Some("2026"), None).unwrap();
        assert_eq!(result.income_statement["totalIncome"].as_f64(), Some(0.0));
        assert_eq!(result.income_statement["totalExpense"].as_f64(), Some(40.0));
        assert_eq!(result.income_statement["result"].as_f64(), Some(-40.0));
        assert_eq!(
            result.income_statement["priorTotalIncome"].as_f64(),
            Some(100.0)
        );
        assert_eq!(result.income_statement["priorResult"].as_f64(), Some(100.0));
        assert_eq!(result.balance["totalAssets"].as_f64(), Some(60.0));
        assert_eq!(result.balance["equity"]["total"].as_f64(), Some(60.0));
        assert_eq!(result.balance["periodResult"].as_f64(), Some(60.0));
        assert_eq!(result.balance["balanced"], true);
        assert_eq!(result.balance["assets"]["priorTotal"].as_f64(), Some(100.0));
        assert_eq!(result.balance["equity"]["priorTotal"].as_f64(), Some(100.0));
        let first = super::read(&tx, "synthetic", "2026-10-18", Some("2025"), None).unwrap();
        assert!(first.balance["assets"]["priorTotal"].is_null());
        let (_other_root, mut shifted_company) = fixture(7, "span");
        let shifted_tx = shifted_company.transaction().unwrap();
        let shifted = super::read(
            &shifted_tx,
            "synthetic",
            "2026-10-18",
            Some("2025/26"),
            None,
        )
        .unwrap();
        assert_eq!(shifted.income_statement["periodStart"], "2025-07-01");
        assert_eq!(shifted.income_statement["periodEnd"], "2026-06-30");
        assert_eq!(shifted.income_statement["priorYearLabel"], "2024/25");
        assert_eq!(shifted.balance["priorAsOfDate"], "2025-06-30");
        assert_eq!(shifted.income_statement["result"].as_f64(), Some(-40.0));
    }
}
