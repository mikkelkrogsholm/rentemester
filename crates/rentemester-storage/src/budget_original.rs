//! Original calendar-month contracts over the shared append-only budget writer.
use crate::{idempotency::PrincipalKind, invalid, legacy_money};
use rentemester_engine::{Error, document::money, money::Money, sales_invoice::wire};
use rusqlite::{Transaction, params};
use serde::Deserialize;
use serde_json::{Number, Value, json};
use std::collections::BTreeMap;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Set {
    pub account_no: String,
    pub period: String,
    pub amount: Option<Number>,
    pub notes: Option<String>,
}
pub(crate) fn valid_period(p: &str) -> bool {
    let p = p.trim();
    p.len() == 7
        && p.is_ascii()
        && rentemester_engine::calendar::valid_iso_date(&format!("{}-01", p.trim()))
}
pub fn set(
    tx: &Transaction<'_>,
    input: &Set,
    subject: &str,
    kind: PrincipalKind,
) -> Result<Value, Error> {
    let no = input.account_no.trim();
    let mut errors = vec![];
    let exists: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM accounts WHERE account_no=?1)",
            [no],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    if no.is_empty() {
        errors.push("accountNo is required".to_string())
    } else if !exists {
        errors.push(format!(
            "account {no} does not exist in the chart of accounts"
        ))
    }
    if !valid_period(input.period.trim()) {
        errors.push("period must be a YYYY-MM calendar month".into())
    }
    let amount = input
        .amount
        .as_ref()
        .and_then(|n| n.as_f64().filter(|v| v.is_finite() && *v >= 0.0))
        .and(input.amount.as_ref())
        .and_then(|n| money(n).ok());
    if amount.is_none() {
        errors.push("amount must be a non-negative number".into())
    }
    if input.notes.as_ref().is_some_and(|s| s.len() > 4096) {
        errors.push("notes exceeds4096 bytes".into())
    }
    if !errors.is_empty() {
        return Ok(json!({"ok":false,"appliedRules":["budget:set"],"errors":errors}));
    }
    let result = crate::budget::set_with_principal(
        tx,
        &crate::budget::Set {
            account_no: no.into(),
            period: input.period.trim().into(),
            amount: input.amount.clone().unwrap(),
            notes: input.notes.clone(),
        },
        subject,
        kind,
    )?;
    Ok(
        json!({"ok":true,"budgetLineId":result["id"],"accountNo":result["accountNo"],"period":result["period"],"amount":result["amount"],"appliedRules":["budget:set"],"errors":[]}),
    )
}
pub fn list(
    tx: &Transaction<'_>,
    period: Option<&str>,
    account_no: Option<&str>,
) -> Result<Value, Error> {
    if period.is_some_and(|p| !valid_period(p)) {
        return Ok(
            json!({"ok":false,"count":0,"rows":[],"errors":["period filter must be a YYYY-MM calendar month"]}),
        );
    }
    let mut q=tx.prepare("SELECT b.id,b.account_no,a.name,b.period,b.amount,b.notes,b.created_at FROM budget_lines b LEFT JOIN accounts a ON a.account_no=b.account_no WHERE b.id IN(SELECT MAX(id) FROM budget_lines GROUP BY account_no,period) AND (?1 IS NULL OR b.period=?1) AND (?2 IS NULL OR b.account_no=?2) ORDER BY b.period,b.account_no").map_err(invalid)?;
    let mut rows = q.query(params![period, account_no]).map_err(invalid)?;
    let mut out = vec![];
    while let Some(r) = rows.next().map_err(invalid)? {
        out.push(json!({"id":r.get::<_,i64>(0).map_err(invalid)?,"accountNo":r.get::<_,String>(1).map_err(invalid)?,"accountName":r.get::<_,Option<String>>(2).map_err(invalid)?,"period":r.get::<_,String>(3).map_err(invalid)?,"amount":wire(&legacy_money(r.get_ref(4).map_err(invalid)?)?)?,"notes":r.get::<_,Option<String>>(5).map_err(invalid)?,"createdAt":r.get::<_,String>(6).map_err(invalid)?}));
    }
    Ok(json!({"ok":true,"count":out.len(),"rows":out,"errors":[]}))
}
struct Cell {
    name: Option<String>,
    kind: Option<String>,
    budget: Money,
    actual: Money,
}
pub fn vs_actual(tx: &Transaction<'_>, from: &str, to: &str) -> Result<Value, Error> {
    let mut errors = vec![];
    if !valid_period(from) {
        errors.push("periodStart must be a YYYY-MM calendar month")
    }
    if !valid_period(to) {
        errors.push("periodEnd must be a YYYY-MM calendar month")
    }
    if errors.is_empty() && from > to {
        errors.push("periodStart must be before or equal to periodEnd")
    }
    let mut result = json!({"ok":errors.is_empty(),"appliedRules":["budget:vs-actual"],"periodStart":from,"periodEnd":to,"lines":[],"totalBudget":0,"totalActual":0,"totalVariance":0,"errors":errors});
    if !errors.is_empty() {
        return Ok(result);
    }
    let mut cells = BTreeMap::new();
    let budgets = list(tx, None, None)?;
    for b in budgets["rows"].as_array().unwrap() {
        let p = b["period"].as_str().unwrap();
        if p < from || p > to {
            continue;
        }
        let no = b["accountNo"].as_str().unwrap();
        let kind: Option<String> = tx
            .query_row("SELECT type FROM accounts WHERE account_no=?1", [no], |r| {
                r.get(0)
            })
            .map_err(invalid)?;
        cells.insert(
            (p.to_string(), no.to_string()),
            Cell {
                name: b["accountName"].as_str().map(str::to_string),
                kind,
                budget: money(b["amount"].as_number().unwrap())?,
                actual: Money::default(),
            },
        );
    }
    let mut q=tx.prepare("SELECT a.account_no,a.name,a.type,substr(j.transaction_date,1,7),l.debit_amount,l.credit_amount FROM journal_entries j JOIN journal_lines l ON l.journal_entry_id=j.id JOIN accounts a ON a.id=l.account_id WHERE j.status IN ('posted','reversed') AND substr(j.transaction_date,1,7) BETWEEN ?1 AND ?2 ORDER BY j.id,l.id").map_err(invalid)?;
    let mut rows = q.query(params![from, to]).map_err(invalid)?;
    while let Some(r) = rows.next().map_err(invalid)? {
        let no: String = r.get(0).map_err(invalid)?;
        let kind: String = r.get(2).map_err(invalid)?;
        let p: String = r.get(3).map_err(invalid)?;
        let key = (p, no);
        if !matches!(kind.as_str(), "income" | "expense") && !cells.contains_key(&key) {
            continue;
        }
        let c = cells.entry(key).or_insert(Cell {
            name: r.get(1).map_err(invalid)?,
            kind: Some(kind.clone()),
            budget: Money::default(),
            actual: Money::default(),
        });
        let d = legacy_money(r.get_ref(4).map_err(invalid)?)?;
        let cr = legacy_money(r.get_ref(5).map_err(invalid)?)?;
        c.actual.add(&if kind == "expense" {
            d.subtract(&cr)
        } else {
            cr.subtract(&d)
        });
    }
    let (mut budget, mut actual) = (Money::default(), Money::default());
    let mut lines = vec![];
    for ((p, no), c) in cells {
        budget.add(&c.budget);
        actual.add(&c.actual);
        let variance = if c.kind.as_deref() == Some("expense") {
            c.budget.subtract(&c.actual)
        } else {
            c.actual.subtract(&c.budget)
        };
        lines.push(json!({"accountNo":no,"accountName":c.name,"accountType":c.kind,"period":p,"budget":wire(&c.budget)?,"actual":wire(&c.actual)?,"variance":wire(&variance)?}));
    }
    result["lines"] = json!(lines);
    result["totalBudget"] = wire(&budget)?;
    result["totalActual"] = wire(&actual)?;
    result["totalVariance"] = wire(&actual.subtract(&budget))?;
    Ok(result)
}
