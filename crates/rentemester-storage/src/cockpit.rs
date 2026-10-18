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
}
fn number(money: &Money) -> Result<Value, Error> {
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
    let mut statement = tx.prepare("SELECT je.id,je.entry_no,je.transaction_date,je.text,je.document_id,d.document_no FROM journal_entries je LEFT JOIN documents d ON d.id=je.document_id WHERE je.transaction_date>=?1 AND je.transaction_date<=?2 ORDER BY je.transaction_date DESC,je.id DESC").map_err(invalid)?;
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
        entries.push(json!({"id":id,"entryNo":r.get::<_,String>(1).map_err(invalid)?,"date":r.get::<_,String>(2).map_err(invalid)?,"text":r.get::<_,String>(3).map_err(invalid)?,"documentId":r.get::<_,Option<i64>>(4).map_err(invalid)?,"documentNo":r.get::<_,Option<String>>(5).map_err(invalid)?,"total":number(&total)?,"lines":wire}));
    }
    base["entries"] = json!(entries);
    base["accountFilter"] = filter
        .map(|a| json!({"accountNo":a["accountNo"],"name":a["name"]}))
        .unwrap_or(Value::Null);
    base["nativePosting"] =
        json!({"ruleVersion":crate::manual_journal_rule_version()?,"accounts":accounts});
    Ok(Snapshot {
        journal: base,
        trial_balance,
        fiscal_years: json!({"slug":slug,"years":fiscal_years}),
    })
}
