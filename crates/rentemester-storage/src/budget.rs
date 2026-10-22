//! Immutable account/month plans. Budget changes never post journal entries.
pub mod dimensions;
use crate::{invalid, legacy_money};
use rentemester_engine::{
    Error,
    document::money,
    fiscal_year::{self, Date, LabelStrategy, StartMonth},
    money::Money,
    sales_invoice::wire,
};
use rusqlite::{Transaction, params};
use serde::Deserialize;
use serde_json::{Number, Value, json};
use std::collections::BTreeMap;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Set {
    pub account_no: String,
    pub period: String,
    pub amount: Number,
    pub notes: Option<String>,
}
fn refused(message: impl Into<String>) -> Error {
    Error::new("budget_refused", message)
}
fn period(s: &str) -> Result<(), Error> {
    if s.len() != 7 || !s.is_ascii() {
        return Err(refused("period must be YYYY-MM"));
    }
    Date::parse(&format!("{s}-01"))?;
    Ok(())
}
pub fn set(tx: &Transaction<'_>, input: &Set, actor: &str) -> Result<Value, Error> {
    set_with_principal(tx, input, actor, crate::idempotency::PrincipalKind::User)
}
pub fn set_with_principal(
    tx: &Transaction<'_>,
    input: &Set,
    actor: &str,
    kind: crate::idempotency::PrincipalKind,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    period(&input.period)?;
    let no = input.account_no.trim();
    let exists: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM accounts WHERE account_no=?1)",
            [no],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    let amount = money(&input.amount)?;
    if !exists
        || input.amount.as_f64().is_some_and(|v| v < 0.0)
        || amount.is_negative()
        || input.notes.as_ref().is_some_and(|s| s.len() > 4096)
    {
        return Err(refused("existing account and non-negative budget required"));
    }
    let notes = input
        .notes
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let id:i64=tx.query_row("INSERT INTO budget_lines(account_no,period,amount,notes) VALUES(?1,?2,?3,?4) RETURNING id",params![no,input.period,crate::journal_post::money(&amount)?,notes],|r|r.get(0)).map_err(invalid)?;
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES('budget_set','budget_line',?1,?2,?3)",params![id.to_string(),format!("Set budget for {no} {} to {}",input.period,amount.decimal()),format!("{}:{actor}", match kind { crate::idempotency::PrincipalKind::User => "user", crate::idempotency::PrincipalKind::ServiceAccount => "service_account" })]).map_err(invalid)?;
    Ok(json!({"id":id,"accountNo":no,"period":input.period,"amount":wire(&amount)?}))
}
struct Cell {
    name: Option<String>,
    kind: Option<String>,
    budget: Money,
    actual: Money,
}
pub fn read(
    tx: &Transaction<'_>,
    slug: &str,
    today: &str,
    selected: Option<&str>,
    compare: bool,
) -> Result<Value, Error> {
    crate::validate(tx)?;
    let company = crate::company_setup::profile(tx)?;
    let month = company["fiscalYearStartMonth"].as_i64().unwrap_or(1) as i32;
    let strategy = LabelStrategy::try_from(
        company["fiscalYearLabelStrategy"]
            .as_str()
            .unwrap_or("end-year"),
    )?;
    let start = StartMonth::new(month)?;
    let current = fiscal_year::for_date(Date::parse(today)?, start, strategy)?;
    let current_label = current.display_label.clone();
    let mut years = BTreeMap::new();
    years.insert(current_label.clone(), current);
    let mut q=tx.prepare("SELECT transaction_date FROM journal_entries UNION SELECT period||'-01' FROM budget_lines").map_err(invalid)?;
    for date in q
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(invalid)?
    {
        let year = fiscal_year::for_date(Date::parse(&date.map_err(invalid)?)?, start, strategy)?;
        years.insert(year.display_label.clone(), year);
    }
    let label = selected.unwrap_or(&current_label);
    let year = years
        .get(label)
        .ok_or_else(|| refused("unknown live fiscal year"))?;
    let from = year.start.to_string()[..7].to_owned();
    let to = year.end.to_string()[..7].to_owned();
    let mut periods = vec![];
    let mut y: u16 = from[..4].parse().map_err(invalid)?;
    let mut m: u8 = from[5..].parse().map_err(invalid)?;
    loop {
        let p = format!("{y:04}-{m:02}");
        if p > to {
            break;
        }
        let done = p == to;
        periods.push(p);
        if done {
            break;
        }
        if m == 12 {
            y += 1;
            m = 1;
        } else {
            m += 1;
        }
    }
    let mut base = json!({"slug":slug,"selectedYear":label,"archived":false,"company":company,"fiscalYears":years.values().map(|v|json!({"label":v.display_label,"start":v.start.to_string(),"end":v.end.to_string(),"source":"live"})).collect::<Vec<_>>(),"periodStart":from,"periodEnd":to,"periods":periods,"dimensionActualsAvailable":true});
    let mut cells: BTreeMap<(String, String), Cell> = BTreeMap::new();
    let mut lines = vec![];
    let mut total_budget = Money::default();
    let mut q=tx.prepare("SELECT b.id,b.account_no,a.name,a.type,b.period,b.amount,b.notes,b.created_at FROM budget_lines b LEFT JOIN accounts a ON a.account_no=b.account_no WHERE b.id IN(SELECT MAX(id) FROM budget_lines GROUP BY account_no,period) AND b.period BETWEEN ?1 AND ?2 ORDER BY b.period,b.account_no").map_err(invalid)?;
    let mut rows = q.query(params![from, to]).map_err(invalid)?;
    while let Some(r) = rows.next().map_err(invalid)? {
        let no: String = r.get(1).map_err(invalid)?;
        let name: Option<String> = r.get(2).map_err(invalid)?;
        let p: String = r.get(4).map_err(invalid)?;
        let budget = legacy_money(r.get_ref(5).map_err(invalid)?)?;
        total_budget.add(&budget);
        lines.push(json!({"id":r.get::<_,i64>(0).map_err(invalid)?,"accountNo":no,"accountName":name,"period":p,"amount":wire(&budget)?,"notes":r.get::<_,Option<String>>(6).map_err(invalid)?,"createdAt":r.get::<_,String>(7).map_err(invalid)?}));
        cells.insert(
            (no, p),
            Cell {
                name,
                kind: r.get(3).map_err(invalid)?,
                budget,
                actual: Money::default(),
            },
        );
    }
    base["totalBudget"] = wire(&total_budget)?;
    if !compare {
        base["lines"] = json!(lines);
        return Ok(base);
    }
    // Read each amount through exact Money before summing (never SQL REAL sums).
    let mut q=tx.prepare("SELECT a.account_no,a.name,a.type,substr(j.transaction_date,1,7),l.debit_amount,l.credit_amount FROM journal_lines l JOIN journal_entries j ON j.id=l.journal_entry_id JOIN accounts a ON a.id=l.account_id WHERE j.status IN ('posted','reversed') AND j.transaction_date BETWEEN ?1 AND ?2 ORDER BY j.id,l.id").map_err(invalid)?;
    let mut rows = q
        .query(params![year.start.to_string(), year.end.to_string()])
        .map_err(invalid)?;
    while let Some(r) = rows.next().map_err(invalid)? {
        let no: String = r.get(0).map_err(invalid)?;
        let kind: String = r.get(2).map_err(invalid)?;
        let p: String = r.get(3).map_err(invalid)?;
        let key = (no, p);
        if !matches!(kind.as_str(), "income" | "expense") && !cells.contains_key(&key) {
            continue;
        }
        let cell = cells.entry(key).or_insert(Cell {
            name: r.get(1).map_err(invalid)?,
            kind: Some(kind.clone()),
            budget: Money::default(),
            actual: Money::default(),
        });
        let debit = legacy_money(r.get_ref(4).map_err(invalid)?)?;
        let credit = legacy_money(r.get_ref(5).map_err(invalid)?)?;
        cell.actual.add(&if kind == "expense" {
            debit.subtract(&credit)
        } else {
            credit.subtract(&debit)
        });
    }
    let mut total_actual = Money::default();
    let mut comparison = vec![];
    for ((no, p), cell) in cells {
        total_actual.add(&cell.actual);
        let variance = if cell.kind.as_deref() == Some("expense") {
            cell.budget.subtract(&cell.actual)
        } else {
            cell.actual.subtract(&cell.budget)
        };
        let budget = wire(&cell.budget)?;
        let v = wire(&variance)?;
        comparison.push(json!({"accountNo":no,"accountName":cell.name,"accountType":cell.kind,"period":p,"budget":budget,"actual":wire(&cell.actual)?,"variance":v,"variancePercent":if cell.budget==Money::default(){Value::Null}else{json!(v.as_f64().unwrap()/budget.as_f64().unwrap())}}));
    }
    comparison.sort_by_key(|v| {
        (
            v["period"].as_str().unwrap().to_owned(),
            v["accountNo"].as_str().unwrap().to_owned(),
        )
    });
    base["lines"] = json!(comparison);
    base["totalActual"] = wire(&total_actual)?;
    base["totalVariance"] = wire(&total_actual.subtract(&total_budget))?;
    Ok(base)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{Connection, TransactionBehavior};
    fn fixture(shifted: bool) -> Connection {
        let mut control = Connection::open_in_memory().unwrap();
        let mut db = Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic budget company")
            .unwrap();
        if shifted {
            db.execute(
                "UPDATE companies SET fiscal_year_start_month=3,fiscal_year_label_strategy='span'",
                [],
            )
            .unwrap();
        }
        db.execute_batch("INSERT INTO accounts(id,account_no,name,type,normal_balance) VALUES(1,'B','Synthetic cash','asset','debit'),(2,'I','Synthetic revenue','income','credit'),(3,'X','Synthetic expense','expense','debit'); INSERT INTO journal_entries(id,entry_no,transaction_date,text,rule_version,entry_hash) VALUES(1,'S-1','2026-02-28','Synthetic sale','fixture','synthetic-hash-1'),(2,'S-2','2026-03-01','Synthetic expense','fixture','synthetic-hash-2'),(3,'S-3','2026-02-01','Synthetic expense','fixture','synthetic-hash-3'); INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount) VALUES(1,1,90.31,0),(1,2,0,90.31),(2,3,5,0),(2,1,0,5),(3,3,0.1,0),(3,1,0,0.1),(3,3,0.2,0),(3,1,0,0.2);").unwrap();
        db
    }
    fn input(no: &str, p: &str, amount: f64) -> Set {
        serde_json::from_value(json!({"accountNo":no,"period":p,"amount":amount})).unwrap()
    }
    #[test]
    fn immutable_latest_revision_exact_actual_and_natural_variances() {
        let mut db = fixture(false);
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let first = set(&tx, &input("X", "2026-02", 1.005), "synthetic").unwrap();
        assert_eq!(first["amount"], 1.01);
        set(&tx, &input("X", "2026-02", 0.4), "synthetic").unwrap();
        set(&tx, &input("I", "2026-02", 80.0), "synthetic").unwrap();
        let plan = read(&tx, "synthetic", "2026-02-28", None, false).unwrap();
        assert_eq!(plan["lines"].as_array().unwrap().len(), 2);
        assert_eq!(plan["totalBudget"], 80.4);
        let report = read(&tx, "synthetic", "2026-02-28", None, true).unwrap();
        let lines = report["lines"].as_array().unwrap();
        let expense = lines
            .iter()
            .find(|r| r["accountNo"] == "X" && r["period"] == "2026-02")
            .unwrap();
        assert_eq!(expense["budget"], 0.4);
        assert_eq!(expense["actual"], 0.3);
        assert_eq!(expense["variance"], 0.1);
        let income = lines.iter().find(|r| r["accountNo"] == "I").unwrap();
        assert_eq!(income["actual"], 90.31);
        assert_eq!(income["variance"], 10.31);
        assert_eq!(
            tx.query_row("SELECT COUNT(*) FROM journal_entries", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            3
        );
        assert_eq!(
            tx.query_row("SELECT COUNT(*) FROM budget_lines", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            3
        );
        assert_eq!(tx.query_row("SELECT COUNT(*) FROM audit_log WHERE event_type='budget_set' AND actor='user:synthetic'",[],|r|r.get::<_,i64>(0)).unwrap(),3);
        assert!(tx.execute("UPDATE budget_lines SET amount=0", []).is_err());
        assert!(tx.execute("DELETE FROM budget_lines", []).is_err());
        tx.commit().unwrap();
        let tx = db.transaction().unwrap();
        assert_eq!(
            read(&tx, "synthetic", "2026-02-28", None, false).unwrap()["totalBudget"],
            80.4
        );
    }
    #[test]
    fn original_and_reversed_counter_entry_remain_part_of_exact_actual() {
        let mut db = fixture(false);
        db.execute_batch("INSERT INTO journal_entries(id,entry_no,transaction_date,text,status,rule_version,entry_hash,reversal_of_entry_id) VALUES(4,'R1','2026-02-02','Original','posted','fixture','hash4',NULL),(5,'R2','2026-02-03','Counter-entry','reversed','fixture','hash5',4);INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount)VALUES(4,3,10,0),(4,1,0,10),(5,3,0,10),(5,1,10,0);").unwrap();
        let tx = db.transaction().unwrap();
        let value = read(&tx, "synthetic", "2026-02-28", None, true).unwrap();
        let expense = value["lines"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["accountNo"] == "X" && r["period"] == "2026-02")
            .unwrap();
        assert_eq!(expense["actual"], 0.3);
    }
    #[test]
    fn shifted_fiscal_bounds_and_budget_only_years() {
        let mut db = fixture(true);
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        set(&tx, &input("X", "2027-04", 50.0), "synthetic").unwrap();
        let report = read(&tx, "synthetic", "2026-02-28", Some("2025/26"), true).unwrap();
        assert_eq!(report["periodStart"], "2025-03");
        assert_eq!(report["periodEnd"], "2026-02");
        assert_eq!(report["totalActual"], 90.61);
        let future = read(&tx, "synthetic", "2026-02-28", Some("2027/28"), false).unwrap();
        assert_eq!(future["totalBudget"], 50.0);
        assert_eq!(future["periods"].as_array().unwrap().len(), 12);
        assert!(read(&tx, "synthetic", "2026-02-28", Some("nonsense"), false).is_err());
    }
    #[test]
    fn malformed_and_negative_budget_inputs_have_no_effect() {
        let mut db = fixture(false);
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        for i in [
            input("unknown", "2026-02", 1.0),
            input("X", "2026-13", 1.0),
            input("X", "2026-2", 1.0),
            input("X", "2026-02", -1.0),
            input("X", "2026-02", -0.001),
            input("X", "2026-02", 1e25),
        ] {
            assert!(set(&tx, &i, "synthetic").is_err());
        }
        assert_eq!(
            tx.query_row("SELECT COUNT(*) FROM budget_lines", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        for p in ["", "éééé-1", "2026-00", "2026-02 "] {
            assert!(period(p).is_err());
        }
        assert!(period("2028-02").is_ok());
    }
}
