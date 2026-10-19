//! Company overview composed from owned native readers. No postings or parallel ledger arithmetic.
use crate::{cockpit::number, invalid};
use rentemester_engine::{
    Error, financial_statements,
    fiscal_year::{self, Date, LabelStrategy, StartMonth},
    money::Money,
    trial_balance::Period,
};
use rusqlite::Transaction;
use serde_json::{Value, json};
fn amount(v: &Value) -> Result<Money, Error> {
    Money::parse(
        &v.as_number()
            .ok_or_else(|| invalid("missing overview amount"))?
            .to_string(),
    )
}
fn ratio(n: &Value, d: &Value) -> Result<Value, Error> {
    let n = n
        .as_f64()
        .ok_or_else(|| invalid("missing overview numerator"))?;
    let d = d
        .as_f64()
        .ok_or_else(|| invalid("missing overview denominator"))?;
    if d == 0.0 {
        return Ok(Value::Null);
    }
    serde_json::Number::from_f64(n / d)
        .map(Value::Number)
        .ok_or_else(|| invalid("overview ratio out of range"))
}
fn exceptions(tx: &Transaction<'_>) -> Result<Value, Error> {
    let rows = crate::exceptions::rows(tx, "open")?;
    let mut groups = std::collections::BTreeMap::<String, (usize, u8)>::new();
    for row in &rows {
        let rank = match row["severity"].as_str() {
            Some("high") => 2,
            Some("medium") => 1,
            _ => 0,
        };
        let group = groups
            .entry(row["type"].as_str().unwrap().into())
            .or_default();
        group.0 += 1;
        group.1 = group.1.max(rank);
    }
    let mut groups:Vec<_>=groups.into_iter().map(|(kind,(n,rank))|{
        let (label,link)=match kind.as_str(){
            "UNMATCHED_BANK_TRANSACTION"=>(format!("{n} {} mangler afstemning",if n==1{"banktransaktion"}else{"banktransaktioner"}),Some("bank")),
            "BANK_BALANCE_GAP"=>(format!("{n} {} mellem bogført og faktisk banksaldo",if n==1{"afvigelse"}else{"afvigelser"}),Some("bank")),
            "MAIL_INTAKE_NO_ATTACHMENT"=>(format!("{n} {} uden vedhæftet bilag",if n==1{"indkommen mail"}else{"indkomne mails"}),None),
            "MAIL_INTAKE_AMBIGUOUS_METADATA"=>(format!("{n} bilag med uklare oplysninger fra mail"),None),
            "MAIL_INTAKE_INGEST_BLOCKED"=>(format!("{n} {} ikke indlæses",if n==1{"mail kunne"}else{"mails kunne"}),None),
            "ASSET_WRITEOFF_MISSING_DOCUMENTATION"=>(format!("{n} {} dokumentation",if n==1{"aktiv-afskrivning mangler"}else{"aktiv-afskrivninger mangler"}),None),
            "ASSET_WRITEOFF_ELIGIBILITY_UNCERTAIN"=>(format!("{n} {} med usikker fradragsret",if n==1{"aktiv-afskrivning"}else{"aktiv-afskrivninger"}),None),
            _=>(format!("{n} {} kræver gennemgang",if n==1{"undtagelse"}else{"undtagelser"}),None),
        };(rank,n,kind.clone(),json!({"type":kind,"count":n,"severity":(["low","medium","high"][rank as usize]),"label":label,"link":link}))
    }).collect();
    groups.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)).then(a.2.cmp(&b.2)));
    Ok(
        json!({"count":rows.len(),"rows":rows.iter().take(6).collect::<Vec<_>>(),"groups":groups.into_iter().map(|g|g.3).collect::<Vec<_>>()}),
    )
}
fn current_receivables(
    tx: &Transaction<'_>,
    slug: &str,
    today: &str,
    company: &Value,
) -> Result<Value, Error> {
    // Every invoice period, including old/unposted invoices. Balances are shared sales-reader results.
    let month = StartMonth::new(
        company["fiscalYearStartMonth"]
            .as_i64()
            .ok_or_else(|| invalid("fiscal month"))? as i32,
    )?;
    let strategy = LabelStrategy::try_from(
        company["fiscalYearLabelStrategy"]
            .as_str()
            .ok_or_else(|| invalid("fiscal strategy"))?,
    )?;
    let mut q = tx
        .prepare("SELECT DISTINCT invoice_date FROM documents WHERE document_type='issued_invoice'")
        .map_err(invalid)?;
    let mut periods = std::collections::BTreeMap::new();
    for date in q
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(invalid)?
    {
        let period = fiscal_year::for_date(Date::parse(&date.map_err(invalid)?)?, month, strategy)?;
        periods.insert(period.display_label.clone(), period.start.to_string());
    }
    let mut total = Money::default();
    let mut count = 0;
    for (label, start) in periods {
        // Use the known invoice period as the context date, so a year with unposted invoices exists.
        let invoices = crate::sales::list(tx, slug, &start, Some(&label))?;
        for row in invoices["invoices"]
            .as_array()
            .ok_or_else(|| invalid("invoice list"))?
        {
            if row["currency"] != "DKK" {
                return Ok(Value::Null);
            }
            let open = amount(&row["openBalance"])?;
            if !open.is_negative() && open != Money::default() {
                total.add(&open);
                count += 1;
            }
        }
    }
    Ok(json!({"openCount":count,"openTotal":number(&total)?,"asOfDate":today,"basis":"current"}))
}
pub fn read(
    tx: &Transaction<'_>,
    slug: &str,
    today: &str,
    selected: Option<&str>,
    as_of: Option<&str>,
) -> Result<Value, Error> {
    Date::parse(today)?;
    let reference = as_of.unwrap_or(today);
    Date::parse(reference)?;
    let snapshot = crate::cockpit::read(tx, slug, today, selected, None)?;
    let base = &snapshot.trial_balance;
    let year = base["selectedYear"]
        .as_str()
        .ok_or_else(|| invalid("overview year"))?;
    let start = base["periodStart"]
        .as_str()
        .ok_or_else(|| invalid("overview start"))?;
    let end = base["periodEnd"]
        .as_str()
        .ok_or_else(|| invalid("overview end"))?;
    let current =
        crate::cockpit::read(tx, slug, today, None, None)?.trial_balance["selectedYear"].clone();
    let is_current = base["selectedYear"] == current;
    let mut y: i32 = start[..4].parse().map_err(invalid)?;
    let mut m: u8 = start[5..7].parse().map_err(invalid)?;
    let names = [
        "jan", "feb", "mar", "apr", "maj", "jun", "jul", "aug", "sep", "okt", "nov", "dec",
    ];
    let mut months = vec![];
    for _ in 0..12 {
        let from = format!("{y:04}-{m:02}-01");
        let mut to = String::new();
        for d in (28..=31).rev() {
            if let Ok(date) = Date::new(y as u16, m, d) {
                to = date.to_string();
                break;
            }
        }
        let pl = financial_statements::profit_and_loss(&crate::trial_balance_owned(
            tx,
            Period::new(&from, &to)?,
        )?);
        months.push(json!({"month":m,"label":names[m as usize-1],"income":number(&pl.income.total)?,"expense":number(&pl.expense.total)?}));
        if m == 12 {
            m = 1;
            y += 1
        } else {
            m += 1
        }
    }
    let bank = crate::bank::read(tx, slug, today, Some(year))?;
    let vat = crate::vat_report::read(tx, slug, reference, Some(year))?;
    let vat = if vat["vatRegistered"] == true {
        json!({"periodStart":vat["periodStart"],"periodEnd":vat["periodEnd"],"periodLabel":vat["periodLabel"],"outputVat":vat["outputVat"],"outputVatAdjustment":vat["outputVatAdjustment"],"inputVat":vat["inputVat"],"payable":vat["payable"],"deadline":vat["deadline"],"daysRemaining":vat["daysRemaining"],"periodStatus":vat["periodStatus"],"errors":vat["vatReportErrors"]})
    } else {
        Value::Null
    };
    let receivables = if is_current {
        current_receivables(tx, slug, today, &base["company"])?
    } else {
        Value::Null
    };
    let payables = crate::payables::list(tx, slug, end, "open")?;
    let documents = crate::documents::list(tx, slug)?;
    let entries = snapshot.journal["entries"]
        .as_array()
        .ok_or_else(|| invalid("journal entries"))?;
    let recent:Vec<_>=entries.iter().take(8).map(|e|json!({"id":e["id"],"entryNo":e["entryNo"],"date":e["date"],"text":e["text"],"amount":e["total"]})).collect();
    let last = recent
        .first()
        .map(|e| e["date"].clone())
        .unwrap_or(Value::Null);
    let equity = amount(&snapshot.balance["equity"]["total"])?;
    Ok(
        json!({"slug":slug,"selectedYear":year,"archived":false,"archivedSource":null,"company":base["company"],"fiscalYears":base["fiscalYears"],"periodStart":start,"periodEnd":end,"asOfDate":today,"profitAndLoss":{"omsaetning":snapshot.income_statement["totalIncome"],"udgifter":snapshot.income_statement["totalExpense"],"resultat":snapshot.income_statement["result"],"months":months},"bank":{"balance":bank["bookedBalance"],"actualBalance":bank["actualBalance"],"difference":bank["difference"],"bankStatementStatus":bank["bankStatementStatus"],"bankStatementProblem":bank["bankStatementProblem"]},"receivables":receivables,"payables":{"openCount":payables["count"],"openTotal":payables["totalOpenBalance"],"asOfDate":end},"documents":{"linkedCount":documents["linkedCount"],"unlinkedCount":documents["unlinkedCount"],"basis":"current"},"vat":vat,"exceptions":exceptions(tx)?,"recentEntries":recent,"lastPostedDate":last,"keyFigures":{"bruttomargin":ratio(&snapshot.income_statement["result"],&snapshot.income_statement["totalIncome"] )?,"egenkapitalandel":ratio(&number(&equity)?,&snapshot.balance["totalAssets"])?},"availability":{"exceptionReview":true,"accountantExport":true,"historicalReceivables":false}}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, rusqlite::Connection) {
        let dir = tempfile::tempdir().unwrap();
        let mut c = rusqlite::Connection::open(dir.path().join("c.sqlite")).unwrap();
        let mut l = rusqlite::Connection::open(dir.path().join("l.sqlite")).unwrap();
        crate::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic Dashboard").unwrap();
        l.execute_batch("UPDATE companies SET fiscal_year_start_month=7,fiscal_year_label_strategy='span',vat_period_type=NULL WHERE id=1;
            INSERT INTO accounts(id,account_no,name,type,normal_balance) VALUES(1,'A','Synthetic cash','asset','debit'),(2,'I','Synthetic revenue','income','credit'),(3,'X','Synthetic expense','expense','debit');
            INSERT INTO bank_accounts(slug,name,ledger_account_no,currency) VALUES('synthetic','Synthetic bank','A','DKK');
            INSERT INTO journal_entries(id,entry_no,transaction_date,text,rule_version,entry_hash) VALUES(1,'S-1','2026-06-30','Synthetic prior income','fixture','synthetic-1'),(2,'S-2','2026-07-01','Synthetic current income','fixture','synthetic-2'),(3,'S-3','2027-06-30','Synthetic current expense','fixture','synthetic-3');
            INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount) VALUES(1,1,50.03,0),(1,2,0,50.03),(2,1,500.01,0),(2,2,0,500.01),(3,3,100.02,0),(3,1,0,100.02);
            INSERT INTO exceptions(type,severity,message,required_action) VALUES('UNMATCHED_BANK_TRANSACTION','medium','Synthetic bank requires review','Synthetic matching guidance'),('UNMATCHED_BANK_TRANSACTION','high','Synthetic second bank review','Synthetic second guidance'),('SYNTHETIC_NEW_TYPE','low','Synthetic new task',NULL);
            INSERT INTO documents(source,document_type,sha256_hash,invoice_no,invoice_date,amount_inc_vat,currency,payload_json) VALUES('synthetic','issued_invoice','synthetic-doc-1','SYN-INVOICE-1','2024-09-01',100.01,'DKK','{\"currency\":\"DKK\",\"dueDate\":\"2024-10-01\"}'),('synthetic','issued_invoice','synthetic-doc-2','SYN-INVOICE-2','2026-10-01',200.02,'DKK','{\"currency\":\"DKK\",\"dueDate\":\"2026-11-01\"}');").unwrap();
        (dir, l)
    }
    #[test]
    fn exact_live_reader_parity_shifted_months_status_and_restart_without_effects() {
        let (dir, mut l) = fixture();
        let before = l.total_changes();
        let tx = l.transaction().unwrap();
        let overview = read(&tx, "synthetic", "2026-10-19", None, None).unwrap();
        let report = crate::cockpit::read(&tx, "synthetic", "2026-10-19", None, None).unwrap();
        assert_eq!(
            overview["profitAndLoss"]["omsaetning"],
            report.income_statement["totalIncome"]
        );
        assert_eq!(
            overview["profitAndLoss"]["resultat"],
            report.income_statement["result"]
        );
        assert_eq!(
            overview["profitAndLoss"]["udgifter"],
            report.income_statement["totalExpense"]
        );
        assert_eq!(overview["profitAndLoss"]["resultat"].as_f64(), Some(399.99));
        assert_eq!(overview["periodStart"], "2026-07-01");
        assert_eq!(overview["periodEnd"], "2027-06-30");
        assert_eq!(
            overview["profitAndLoss"]["months"][0]["income"].as_f64(),
            Some(500.01)
        );
        assert_eq!(
            overview["profitAndLoss"]["months"][11]["expense"].as_f64(),
            Some(100.02)
        );
        assert_eq!(
            overview["bank"]["balance"],
            crate::bank::read(&tx, "synthetic", "2026-10-19", None).unwrap()["bookedBalance"]
        );
        assert_eq!(overview["receivables"]["openCount"], 2);
        assert_eq!(overview["receivables"]["openTotal"].as_f64(), Some(300.03));
        assert_eq!(
            overview["keyFigures"]["egenkapitalandel"].as_f64(),
            Some(1.0)
        );
        assert_eq!(overview["exceptions"]["count"], 3);
        assert_eq!(overview["exceptions"]["groups"][0]["severity"], "high");
        assert_eq!(overview["exceptions"]["groups"][0]["count"], 2);
        assert_eq!(
            overview["exceptions"]["rows"][1]["requiredAction"],
            "Synthetic second guidance"
        );
        assert_eq!(
            overview["recentEntries"][0]["text"],
            "Synthetic current expense"
        );
        assert_eq!(overview["documents"]["unlinkedCount"], 2);
        assert!(overview["vat"].is_null());
        let historical = read(&tx, "synthetic", "2026-10-19", Some("2025/26"), None).unwrap();
        assert!(historical["receivables"].is_null());
        assert_eq!(
            historical["profitAndLoss"]["resultat"].as_f64(),
            Some(50.03)
        );
        assert!(read(&tx, "synthetic", "2026-10-19", Some("unknown"), None).is_err());
        assert!(read(&tx, "synthetic", "2026-10-19", None, Some("2026-02-30")).is_err());
        tx.commit().unwrap();
        assert_eq!(l.total_changes(), before);
        drop(l);
        let mut reopened = rusqlite::Connection::open(dir.path().join("l.sqlite")).unwrap();
        let tx = reopened.transaction().unwrap();
        assert_eq!(
            read(&tx, "synthetic", "2026-10-19", None, None).unwrap(),
            overview
        );
    }
    #[test]
    fn empty_denominators_are_unknown_and_vat_uses_existing_reader() {
        let mut c = rusqlite::Connection::open_in_memory().unwrap();
        let mut l = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic Empty Dashboard").unwrap();
        l.execute_batch("UPDATE companies SET vat_period_type='quarter' WHERE id=1")
            .unwrap();
        let tx = l.transaction().unwrap();
        let overview = read(&tx, "synthetic", "2026-10-19", None, None).unwrap();
        assert!(overview["keyFigures"]["bruttomargin"].is_null());
        assert!(overview["keyFigures"]["egenkapitalandel"].is_null());
        let vat = crate::vat_report::read(&tx, "synthetic", "2026-10-19", None).unwrap();
        for key in [
            "periodStart",
            "periodEnd",
            "periodLabel",
            "deadline",
            "daysRemaining",
            "periodStatus",
            "outputVat",
            "inputVat",
            "payable",
        ] {
            assert_eq!(overview["vat"][key], vat[key], "{key}");
        }
        assert_eq!(overview["receivables"]["openCount"], 0);
        assert_eq!(overview["recentEntries"], json!([]));
        assert!(overview["lastPostedDate"].is_null());
    }
}
