//! Historical bank cashflow on an admitted company transaction; no financial writes.
use crate::bank;
use rentemester_engine::{
    Error,
    cashflow::{self as policy, Movement},
    fiscal_year::{self, Date, FiscalYear, LabelStrategy, StartMonth},
    fx::FxRate,
};
use rusqlite::{Transaction, params, types::ValueRef};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
#[cfg(test)]
mod tests;
fn sql(_: impl std::fmt::Display) -> Error {
    policy::refused("Admitted bank cashflow source is unavailable or invalid")
}
fn rate(value: ValueRef<'_>) -> Result<Option<FxRate>, Error> {
    match value {
        ValueRef::Null => Ok(None),
        ValueRef::Integer(n) => FxRate::parse(&n.to_string()).map(Some),
        ValueRef::Real(n) if n.is_finite() => FxRate::parse(&n.to_string()).map(Some),
        _ => Err(sql("invalid bank FX rate")),
    }
}
/// Complete response body. Authorization and physical namespace lifetime belong to the caller.
pub fn read(
    tx: &Transaction<'_>,
    slug: &str,
    today: &str,
    selected: Option<&str>,
) -> Result<Value, Error> {
    Date::parse(today)?;
    crate::journal_preview::require_current(tx)?;
    crate::migration::integrity(tx)?;
    crate::archive_import::validate_history(tx)?;
    if selected.is_some_and(|s| s.is_empty() || s.len() > 32) {
        return Err(Error::new(
            "invalid_year",
            "exact known fiscal-year descriptor required",
        ));
    }
    let (name,cvr,country,currency,month,strategy):(String,Option<String>,String,String,i32,String)=tx.query_row(
        "SELECT name,cvr,country,currency,fiscal_year_start_month,fiscal_year_label_strategy FROM companies WHERE id=1",[],
        |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).map_err(sql)?;
    let company = json!({"name":name,"cvr":cvr,"country":country,"currency":currency,"fiscalYearStartMonth":month,"fiscalYearLabelStrategy":strategy});
    let ledger_uuid: String = tx
        .query_row(
            "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
            [],
            |r| r.get(0),
        )
        .map_err(sql)?;
    if ledger_uuid.len() != 32
        || !ledger_uuid
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(sql("invalid ledger identity"));
    }
    let start_month = StartMonth::new(month)?;
    let strategy = LabelStrategy::try_from(strategy.as_str())?;
    let current = fiscal_year::for_date(Date::parse(today)?, start_month, strategy)?;
    let current_label = current.display_label.clone();
    let mut years = BTreeMap::<String, FiscalYear>::new();
    years.insert(current_label.clone(), current);
    let mut dates=tx.prepare("SELECT transaction_date FROM bank_transactions UNION SELECT transaction_date FROM journal_entries ORDER BY transaction_date").map_err(sql)?;
    for date in dates
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(sql)?
    {
        let year = fiscal_year::for_date(Date::parse(&date.map_err(sql)?)?, start_month, strategy)?;
        years.insert(year.display_label.clone(), year);
    }
    let mut archive_years = BTreeSet::new();
    let mut q = tx
        .prepare("SELECT fiscal_year FROM import_archive_years ORDER BY fiscal_year")
        .map_err(sql)?;
    for year in q.query_map([], |r| r.get::<_, u16>(0)).map_err(sql)? {
        archive_years.insert(format!("{:04}", year.map_err(sql)?));
    }
    let mut inventory:BTreeMap<String,Value>=years.iter().map(|(label,y)|(label.clone(),json!({"label":label,"start":y.start.to_string(),"end":y.end.to_string(),"source":"live","selectable":true}))).collect();
    for label in &archive_years {
        inventory.entry(label.clone()).or_insert_with(
            || json!({"label":label,"start":null,"end":null,"source":"archive","selectable":true}),
        );
    }
    let label = selected.unwrap_or(&current_label);
    let fiscal_years: Vec<_> = inventory.into_values().rev().collect();
    let mut body = json!({"slug":slug,"ledgerUuid":ledger_uuid,"selectedYear":label,"archived":false,"company":company,"currency":"DKK","fiscalYears":fiscal_years});
    let Some(year) = years.get(label) else {
        if !archive_years.contains(label) {
            return Err(Error::new(
                "invalid_year",
                "unknown cashflow fiscal-year descriptor",
            ));
        }
        body.as_object_mut().unwrap().extend(json!({"archived":true,"periodStart":null,"periodEnd":null,
            "hasTransactions":null,"months":[],"balanceSeries":[],"openingBalance":null,"closingBalance":null,
            "totalIn":null,"totalOut":null,"status":"unavailable","coverage":{"transactionCount":null,"valuedTransactionCount":null,"unvaluedTransactionCount":null,"balancePointCount":0,"unavailableBalancePointCount":0},
            "balanceStatus":{"opening":"unavailable","closing":"unavailable","series":"unavailable"},
            "balanceReason":{"opening":"archive_bank_source_unavailable","closing":"archive_bank_source_unavailable","series":"archive_bank_source_unavailable"},
            "limitations":["Arkivets regnskabsposteringer indeholder ikke et verificeret bankkontoudtog eller bankperiodens grænser. Pengestrømmen er utilgængelig."]}).as_object().unwrap().clone());
        return Ok(json!({"ok":true,"cashflow":body}));
    };
    let start = year.start.to_string();
    let end = year.end.to_string();
    body["periodStart"] = json!(start);
    body["periodEnd"] = json!(end);
    let mut movements = vec![];
    let mut point_dates = BTreeSet::new();
    let mut q=tx.prepare("SELECT id,transaction_date,amount,currency,amount_dkk,fx_rate_to_dkk FROM bank_transactions WHERE transaction_date BETWEEN ?1 AND ?2 ORDER BY transaction_date,id").map_err(sql)?;
    let mut rows = q.query(params![start, end]).map_err(sql)?;
    while let Some(row) = rows.next().map_err(sql)? {
        let id: i64 = row.get(0).map_err(sql)?;
        // Validate explicit native profile provenance without reinterpreting legacy generic rows.
        bank::source_profile(tx, id)?;
        let date: String = row.get(1).map_err(sql)?;
        let amount = bank::signed_money(row.get_ref(2).map_err(sql)?)?;
        let currency: String = row.get(3).map_err(sql)?;
        let dkk = match row.get_ref(4).map_err(sql)? {
            ValueRef::Null => None,
            value => Some(bank::signed_money(value)?),
        };
        let rate = rate(row.get_ref(5).map_err(sql)?)?;
        let amount_dkk = policy::valued_amount(&currency, &amount, dkk.as_ref(), rate.as_ref())?;
        point_dates.insert(date.clone());
        movements.push(Movement { date, amount_dkk });
    }
    let aggregate = policy::aggregate(&start, &end, &movements)?;
    body.as_object_mut()
        .unwrap()
        .extend(aggregate.as_object().unwrap().clone());
    let opening = bank::statement_balance(tx, &policy::preceding_day(&start)?)?;
    let closing = bank::statement_balance(tx, &end)?;
    body["openingBalance"] = opening
        .balance
        .as_ref()
        .map(rentemester_engine::sales_invoice::wire)
        .transpose()?
        .unwrap_or(Value::Null);
    body["closingBalance"] = closing
        .balance
        .as_ref()
        .map(rentemester_engine::sales_invoice::wire)
        .transpose()?
        .unwrap_or(Value::Null);
    let mut series = vec![];
    let mut unavailable_points = 0;
    let mut limitations = BTreeSet::new();
    if body["coverage"]["unvaluedTransactionCount"]
        .as_u64()
        .unwrap()
        > 0
    {
        limitations.insert("Nogle bankposter mangler dokumenteret DKK-værdi. Berørte måneder og samlet pengestrøm er ukendt.".to_owned());
    }
    for date in &point_dates {
        let position = bank::statement_balance(tx, date)?;
        if let Some(value) = position.balance {
            series.push(
                json!({"date":date,"balance":rentemester_engine::sales_invoice::wire(&value)?}),
            );
        } else {
            unavailable_points += 1;
        }
    }
    if unavailable_points > 0 {
        limitations.insert("Datopunkter uden entydig samlet DKK-kontoudtogssaldo er udeladt fra saldoens udvikling.".into());
    }
    if opening.balance.is_none() {
        limitations
            .insert("Primosaldoen er ikke dokumenteret af et uafhængigt bankkontoudtog.".into());
    }
    if closing.balance.is_none() {
        limitations.insert(
            "Ultimosaldoen er ikke dokumenteret af et entydigt samlet DKK-bankkontoudtog.".into(),
        );
    }
    let series_status = if point_dates.is_empty() {
        "none"
    } else if unavailable_points == 0 {
        "complete"
    } else if unavailable_points == point_dates.len() {
        "unavailable"
    } else {
        "partial"
    };
    body["coverage"]["balancePointCount"] = json!(series.len());
    body["coverage"]["unavailableBalancePointCount"] = json!(unavailable_points);
    body["balanceSeries"] = json!(series);
    body["balanceStatus"] =
        json!({"opening":opening.status,"closing":closing.status,"series":series_status});
    body["balanceReason"] = json!({"opening":opening.problem,"closing":closing.problem,"series":if unavailable_points>0{Some("aggregate_statement_evidence_unavailable")}else{None}});
    if body["status"] == "complete"
        && (opening.balance.is_none() || closing.balance.is_none() || unavailable_points > 0)
    {
        body["status"] = json!("partial");
    }
    body["limitations"] = json!(limitations.into_iter().collect::<Vec<_>>());
    Ok(json!({"ok":true,"cashflow":body}))
}
