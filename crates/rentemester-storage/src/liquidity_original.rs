//! Source-shaped read-only cash planning, sharing native financial fact readers.
use crate::{invalid, legacy_money};
use rentemester_engine::{
    Error, calendar::valid_iso_date, document::money, money::Money, sales_invoice::wire,
};
use rusqlite::{Transaction, params};
use serde_json::{Value, json};
pub struct Monthly {
    pub start_date: String,
    pub months: String,
}
pub struct Weekly {
    pub start_date: String,
    pub weeks: Option<String>,
}
pub(crate) struct Facts;
pub(crate) struct Fact {
    pub date: String,
    pub amount: Money,
    pub currency: String,
    pub reference: String,
    pub assumption: bool,
}
fn refused(s: &str) -> Error {
    Error::new("liquidity_refused", s)
}
fn integer(s: &str) -> Option<usize> {
    let n = s.trim().parse::<f64>().ok()?;
    (n.is_finite() && n >= 1.0 && n.fract() == 0.0 && n <= 18.0).then_some(n as usize)
}
fn days(s: &str, n: i64) -> Result<String, Error> {
    let d = rentemester_engine::vat_period::date(s)?
        .checked_add(time::Duration::days(n))
        .ok_or_else(|| refused("forecast calendar bound exceeded"))?
        .to_string();
    rentemester_engine::fiscal_year::Date::parse(&d)?;
    Ok(d)
}
fn next_month(p: &str) -> Result<String, Error> {
    let y = p[..4].parse::<u32>().map_err(invalid)?;
    let m = p[5..7].parse::<u32>().map_err(invalid)?;
    if m == 12 {
        if y == 9999 {
            return Err(refused("forecast calendar bound exceeded"));
        }
        Ok(format!("{:04}-01", y + 1))
    } else {
        Ok(format!("{y:04}-{:02}", m + 1))
    }
}
fn amount(v: &Value) -> Result<Money, Error> {
    money(
        v.as_number()
            .ok_or_else(|| refused("source amount unavailable"))?,
    )
}
fn budgets(
    tx: &Transaction<'_>,
    start: &str,
    end: &str,
) -> Result<Vec<(String, Money, String)>, Error> {
    let mut q=tx.prepare("SELECT b.period,b.amount,b.id,b.account_no FROM budget_lines b JOIN accounts a ON a.account_no=b.account_no WHERE a.type='expense' AND b.id IN(SELECT MAX(id) FROM budget_lines GROUP BY account_no,period) AND b.period BETWEEN ?1 AND ?2 ORDER BY b.period,b.account_no,b.id").map_err(invalid)?;
    let mut rows = q.query(params![&start[..7], &end[..7]]).map_err(invalid)?;
    let mut out = vec![];
    while let Some(r) = rows.next().map_err(invalid)? {
        out.push((
            r.get(0).map_err(invalid)?,
            legacy_money(r.get_ref(1).map_err(invalid)?)?,
            format!(
                "budget-revision:{}:account:{}",
                r.get::<_, i64>(2).map_err(invalid)?,
                r.get::<_, String>(3).map_err(invalid)?
            ),
        ));
    }
    Ok(out)
}
fn recurring(tx: &Transaction<'_>, start: &str, end: &str) -> Result<Vec<Fact>, Error> {
    let mut q=tx.prepare("SELECT id,payment_terms_days,payload_json FROM recurring_invoice_templates WHERE active=1 ORDER BY id").map_err(invalid)?;
    let mut rs = q.query([]).map_err(invalid)?;
    let mut out = vec![];
    while let Some(r) = rs.next().map_err(invalid)? {
        let id: i64 = r.get(0).map_err(invalid)?;
        let terms: i64 = r.get(1).map_err(invalid)?;
        let payload: Value =
            serde_json::from_str(&r.get::<_, String>(2).map_err(invalid)?).map_err(invalid)?;
        let gross = if payload["totals"]["grossAmount"].is_number() {
            amount(&payload["totals"]["grossAmount"])?
        } else {
            let mut raw = payload.clone();
            raw["issueDate"] = json!(start);
            let input: rentemester_engine::sales_invoice::Input =
                serde_json::from_value(raw).map_err(invalid)?;
            amount(&rentemester_engine::sales_invoice::compute(&input)?["totals"]["grossAmount"])?
        };
        if gross.is_negative() || gross == Money::default() {
            continue;
        }
        for index in 0..1000u32 {
            let issue = crate::recurring_invoices::scheduled_date(tx, id, index)?;
            let due = days(&issue, terms)?;
            if due.as_str() > end {
                break;
            }
            if due.as_str() < start {
                continue;
            }
            let generated:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM recurring_invoice_generations WHERE template_id=?1 AND period_index=?2)",params![id,index],|r|r.get(0)).map_err(invalid)?;
            if !generated {
                out.push(Fact {
                    date: due,
                    amount: gross.clone(),
                    currency: payload["currency"].as_str().unwrap_or("DKK").into(),
                    reference: format!("recurring:{id}:{index}"),
                    assumption: true,
                });
            }
        }
    }
    Ok(out)
}
pub fn monthly(tx: &Transaction<'_>, input: &Monthly) -> Result<Value, Error> {
    let mut errors = vec![];
    if !valid_iso_date(&input.start_date) {
        errors.push("startDate must be a YYYY-MM-DD date".to_string())
    }
    let n = integer(&input.months);
    if n.is_none() {
        let v = input.months.trim().parse::<f64>().unwrap_or(f64::NAN);
        errors.push(
            if v.is_finite() && v.fract() == 0.0 && v > 18.0 {
                "months must not exceed 18"
            } else {
                "months must be a positive integer"
            }
            .into(),
        )
    }
    let mut result = json!({"ok":false,"appliedRules":["liquidity-forecast"],"openingBalance":0,"closingBalance":0,"periods":[],"errors":errors});
    if !errors.is_empty() {
        return Ok(result);
    }
    let n = n.unwrap();
    let start = format!("{}-01", &input.start_date[..7]);
    let mut months = vec![];
    let mut cursor = start[..7].to_string();
    for _ in 0..n {
        months.push(cursor.clone());
        cursor = next_month(&cursor)?;
    }
    let end = days(&format!("{cursor}-01"), -1)?;
    let company = crate::company_setup::profile(tx)?;
    let mut caveats = vec![];
    let opening = Facts::opening(
        tx,
        &days(&start, -1)?,
        company["currency"].as_str().unwrap_or(""),
        &mut caveats,
    )?;
    let Some(opening) = opening else {
        result["errors"] = json!(caveats);
        return Ok(result);
    };
    let invoices = Facts::receivables(tx, "", &input.start_date, &company)?;
    let recurring = recurring(tx, &start, &end)?;
    // Original currencies cannot be added to DKK cash without dated settlement FX.
    if invoices
        .iter()
        .chain(recurring.iter())
        .any(|f| f.date >= start && f.date <= end && f.currency != "DKK")
    {
        result["errors"] =
            json!(["Foreign-currency inflows require dated FX; monthly cash forecast unavailable"]);
        return Ok(result);
    }
    let budgets = budgets(tx, &start, &end)?;
    let mut cash = opening.clone();
    let mut periods = vec![];
    for p in months {
        let mut inv = Money::default();
        let mut rec = Money::default();
        let mut cost = Money::default();
        for f in &invoices {
            if f.date.starts_with(&p) {
                inv.add(&f.amount)
            }
        }
        for f in &recurring {
            if f.date.starts_with(&p) {
                rec.add(&f.amount)
            }
        }
        for (month, a, _) in &budgets {
            if *month == p {
                cost.add(a)
            }
        }
        let net = inv
            .subtract(&cost)
            .subtract(&Money::default().subtract(&rec));
        let before = cash.clone();
        cash.add(&net);
        periods.push(json!({"period":p,"openingBalance":wire(&before)?,"invoiceInflow":wire(&inv)?,"recurringInflow":wire(&rec)?,"budgetedCostOutflow":wire(&cost)?,"netChange":wire(&net)?,"closingBalance":wire(&cash)?}));
    }
    Ok(
        json!({"ok":true,"appliedRules":["liquidity-forecast"],"startDate":input.start_date,"months":n,"openingBalance":wire(&opening)?,"closingBalance":wire(&cash)?,"periods":periods,"errors":[]}),
    )
}
const INCLUDED: &[&str] = &[
    "observed opening cash",
    "issued receivables",
    "registered payables",
    "approved DKK commitments",
    "filing-safe closed/reported VAT obligations (settlement status unknown)",
    "dated reviewed DKK supplements scoped by workspace company identity",
];
const EXCLUDED: &[&str] = &[
    "monthly account budgets without a documented cash date (informational scenario upper-bound only)",
    "open VAT periods (estimated scenario assumption, not legal obligation)",
    "foreign-currency commitments or supplements without explicit dated FX",
    "unregistered obligations",
    "unreviewed, malformed, or wrong-company supplements",
];
pub fn weekly(
    control_tx: &Transaction<'_>,
    tx: &Transaction<'_>,
    slug: &str,
    input: &Weekly,
) -> Result<Value, Error> {
    let n = match input.weeks.as_deref() {
        None => Some(13),
        Some(s) => integer(s).filter(|n| *n <= 13),
    };
    let mut result = json!({"ok":false,"openingCash":0,"lowestPoint":0,"completeness":{"included":[],"excluded":[]},"periods":[],"errors":["startDate and weeks (1-13) are required"],"appliedRules":["liquidity-forecast-13-week-v1"]});
    if !valid_iso_date(&input.start_date) || n.is_none() {
        return Ok(result);
    }
    let n = n.unwrap();
    let start = &input.start_date;
    let end = days(start, n as i64 * 7 - 1)?;
    let company = crate::company_setup::profile(tx)?;
    let mut caveats = vec![];
    let opening = Facts::opening(
        tx,
        &days(start, -1)?,
        company["currency"].as_str().unwrap_or(""),
        &mut caveats,
    )?;
    let Some(opening) = opening else {
        result["errors"] = json!(caveats);
        result["appliedRules"] = json!(["liquidity-forecast-13-week-v3"]);
        return Ok(result);
    };
    let invoices = Facts::receivables(tx, slug, start, &company)?;
    let payables = crate::payables::list(tx, slug, start, "open")?;
    let commitments = crate::supplier_commitments::list(tx, slug, start)?;
    let vat = Facts::vat(tx, start, &end)?;
    let budgets = budgets(tx, start, &end)?;
    let reviewed =
        crate::intercompany_dispositions::reviewed_liquidity(control_tx, slug, start, &end)?;
    let mut cash = opening.clone();
    let mut scenario = opening.clone();
    let mut lowest = opening.clone();
    let mut periods = vec![];
    for i in 0..n {
        let from = days(start, i as i64 * 7)?;
        let to = days(&from, 6)?;
        let inside = |d: &str| d >= from.as_str() && d <= to.as_str();
        let (
            mut inv,
            mut payable,
            mut commitment,
            mut scenarios,
            mut obligation,
            mut intercompany,
            mut undated,
        ) = (
            Money::default(),
            Money::default(),
            Money::default(),
            Money::default(),
            Money::default(),
            Money::default(),
            Money::default(),
        );
        let mut sources = vec![];
        let mut excluded = vec![];
        for f in &invoices {
            if inside(&f.date) {
                if f.currency == "DKK" {
                    inv.add(&f.amount)
                } else {
                    excluded.push(json!({"source":f.reference,"amount":wire(&f.amount)?,"currency":f.currency,"reason":"dated_fx_required"}));
                }
            }
        }
        if inv != Money::default() {
            sources.push(json!({"source":"issued_receivables","amount":wire(&inv)?,"reference":"invoice-list"}));
        }
        for p in payables["rows"]
            .as_array()
            .ok_or_else(|| refused("payables unavailable"))?
        {
            if p["dueDate"].as_str().is_some_and(inside) {
                if p["currency"] == "DKK" {
                    payable.add(&amount(&p["openBalance"])?);
                } else {
                    excluded.push(json!({"source":format!("payable:{}",p["payableId"]),"amount":p["openBalance"],"currency":p["currency"],"reason":"dated_fx_required"}));
                }
            }
        }
        if payable != Money::default() {
            sources.push(json!({"source":"registered_payables","amount":wire(&Money::default().subtract(&payable))?,"reference":"payables"}));
        }
        for o in commitments["occurrences"]
            .as_array()
            .ok_or_else(|| refused("commitments unavailable"))?
        {
            if !o["date"].as_str().is_some_and(inside) {
                continue;
            }
            let a = amount(&o["amount"])?;
            if o["currency"] == "DKK" {
                commitment.add(&a);
                sources.push(json!({"source":"approved_commitment","amount":wire(&Money::default().subtract(&a))?,"reference":o["commitmentId"]}));
            } else {
                excluded.push(json!({"source":o["commitmentId"],"amount":o["amount"],"currency":o["currency"],"reason":"dated_fx_required"}));
            }
        }
        for f in &vat {
            if inside(&f.date) {
                let negative = Money::default().subtract(&f.amount);
                if f.assumption {
                    scenarios.add(&negative)
                } else {
                    obligation.add(&f.amount)
                }
                let mut source = json!({"source":if f.assumption{"estimated_vat_assumption"}else{"canonical_vat_obligation"},"amount":wire(&negative)?,"reference":f.reference,"settlementStatus":"unknown"});
                if f.assumption {
                    source["assumption"] = json!(true)
                }
                sources.push(source);
            }
        }
        let first = i == 0 || from[..7] != days(&from, -7)?[..7];
        for (month, a, reference) in &budgets {
            if first && *month == from[..7] && *a != Money::default() {
                let negative = Money::default().subtract(a);
                undated.add(&negative);
                sources.push(json!({"source":"effective_budget_assumption","amount":wire(&negative)?,"reference":reference,"assumption":true}));
            }
        }
        for f in &reviewed {
            if f.currency != "DKK" {
                if i == 0 {
                    excluded.push(json!({"source":f.reference,"amount":wire(&f.amount)?,"currency":f.currency,"reason":"dated_fx_required"}));
                }
                continue;
            }
            if inside(&f.due_date) {
                let signed = if f.inflow {
                    f.amount.clone()
                } else {
                    Money::default().subtract(&f.amount)
                };
                intercompany.add(&signed);
                sources.push(json!({"source":"approved_intercompany_disposition","amount":wire(&signed)?,"reference":f.reference,"assumption":true}));
            }
        }
        let before = cash.clone();
        let movement = inv
            .subtract(&payable)
            .subtract(&commitment)
            .subtract(&obligation);
        cash.add(&movement);
        scenario.add(&movement);
        scenario.add(&scenarios);
        scenario.add(&intercompany);
        if cash.subtract(&lowest).is_negative() {
            lowest = cash.clone()
        }
        periods.push(json!({"weekStart":from,"openingCash":wire(&before)?,"receivables":wire(&inv)?,"payables":wire(&payable)?,"commitments":wire(&commitment)?,"budgets":0,"undatedBudgetAssumptions":wire(&undated)?,"scenarios":wire(&scenarios)?,"obligations":wire(&obligation)?,"intercompany":wire(&intercompany)?,"excluded":excluded,"closingCash":wire(&cash)?,"scenarioClosingCash":wire(&scenario)?,"sources":sources}));
    }
    let mut excluded: Vec<String> = EXCLUDED.iter().map(|s| s.to_string()).collect();
    excluded.extend(caveats);
    Ok(
        json!({"ok":true,"startDate":start,"openingCash":wire(&opening)?,"lowestPoint":wire(&lowest)?,"completeness":{"included":INCLUDED,"excluded":excluded},"periods":periods,"errors":[],"appliedRules":["liquidity-forecast-13-week-v3"]}),
    )
}

#[cfg(test)]
mod tests;
