use super::*;
use rentemester_engine::money::Money;
use rusqlite::OptionalExtension;
use std::collections::BTreeSet;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BudgetAllocation {
    pub dimension_id: String,
    pub member_id: String,
    pub amount: serde_json::Number,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BudgetInput {
    pub account_no: String,
    pub period: String,
    pub allocations: Vec<BudgetAllocation>,
    pub source_ref: String,
    pub plan_hash: Option<String>,
    pub idempotency_key: Option<String>,
    pub confirm: Option<bool>,
}
// Inputs must be exactly expressible in safe integer cents. No quantizing a subcent.
fn exact(n: &serde_json::Number) -> Result<(Money, Value), Error> {
    let s = n.to_string();
    let f = n.as_f64().ok_or_else(|| fail("INVALID_DIMENSION_BUDGET"))?;
    let money = Money::parse(&s)?;
    let minor = money
        .decimal()
        .replace('.', "")
        .parse::<i64>()
        .map_err(|_| fail("INVALID_DIMENSION_BUDGET"))?;
    let canonical = money.decimal();
    let normalized = canonical.trim_end_matches('0').trim_end_matches('.');
    // Compare shortest numeric representations after cent conversion; rejects e.g. 1.001.
    if f < 0.0 || minor > MAX || normalized.parse::<f64>().ok() != Some(f) {
        return Err(fail("INVALID_DIMENSION_BUDGET"));
    }
    let wire = serde_json::from_str(normalized).map_err(invalid)?;
    Ok((money, wire))
}
fn budget_rows(
    tx: &Transaction<'_>,
    sql: &str,
    p: impl rusqlite::Params,
) -> Result<Vec<Value>, Error> {
    let mut q = tx.prepare(sql).map_err(invalid)?;
    let names = q
        .column_names()
        .iter()
        .map(|n| n.to_string())
        .collect::<Vec<_>>();
    let mut rs = q.query(p).map_err(invalid)?;
    let mut out = vec![];
    while let Some(r) = rs.next().map_err(invalid)? {
        let mut row = serde_json::Map::new();
        for (i, name) in names.iter().enumerate() {
            let value = if name == "amount" {
                let m = legacy_money(r.get_ref(i).map_err(invalid)?)?;
                let n = m.decimal().parse::<serde_json::Number>().map_err(invalid)?;
                exact(&n)?.1
            } else {
                match r.get_ref(i).map_err(invalid)? {
                    ValueRef::Null => Value::Null,
                    ValueRef::Integer(v) => json!(v),
                    ValueRef::Text(v) => json!(std::str::from_utf8(v).map_err(invalid)?),
                    _ => return Err(invalid("budget evidence storage type")),
                }
            };
            row.insert(name.clone(), value);
        }
        out.push(Value::Object(row));
    }
    Ok(out)
}
pub fn budgets(tx: &Transaction<'_>) -> Result<Value, Error> {
    crate::validate(tx)?;
    Ok(
        json!({"ok":true,"budgets":budget_rows(tx,"SELECT * FROM current_accounting_dimension_budgets ORDER BY period,account_no,dimension_id,member_id",[])?}),
    )
}
pub fn budget_plan(tx: &Transaction<'_>, input: &BudgetInput) -> Result<Value, Error> {
    crate::validate(tx)?;
    let account = text(&input.account_no, 64)?;
    let source = text(&input.source_ref, 500)?;
    if input.period.len() != 7
        || Date::parse(&format!("{}-01", input.period)).is_err()
        || input.allocations.is_empty()
    {
        return Err(fail("INVALID_DIMENSION_BUDGET"));
    }
    let budget = tx.query_row("SELECT amount FROM budget_lines WHERE account_no=?1 AND period=?2 ORDER BY id DESC LIMIT 1", params![account,input.period], |r| r.get::<_, f64>(0)).optional().map_err(invalid)?
        .ok_or_else(|| fail("ACCOUNT_BUDGET_NOT_FOUND"))?;
    let (account_money, account_wire) = exact(
        &serde_json::Number::from_f64(budget).ok_or_else(|| fail("INVALID_DIMENSION_BUDGET"))?,
    )?;
    let mut allocations = vec![];
    let mut seen = BTreeSet::new();
    let mut total = Money::default();
    for a in &input.allocations {
        let d = text(&a.dimension_id, 64)?;
        let m = text(&a.member_id, 64)?;
        if !seen.insert((d.clone(), m.clone())) {
            return Err(fail("DUPLICATE_DIMENSION_BUDGET_ALLOCATION"));
        }
        let current = member(tx, &d, &m)?.ok_or_else(|| fail("DIMENSION_MEMBER_NOT_ACTIVE"))?;
        if current["status"] != "active"
            || definition(tx, &d)?.is_none_or(|v| v["status"] != "active")
            || current["effective_from"]
                .as_str()
                .is_some_and(|s| s.get(..7).is_none_or(|v| v > input.period.as_str()))
            || current["effective_to"]
                .as_str()
                .is_some_and(|s| s.get(..7).is_none_or(|v| v < input.period.as_str()))
        {
            return Err(fail("DIMENSION_MEMBER_NOT_ACTIVE"));
        }
        let (money, wire) = exact(&a.amount)?;
        total.add(&money);
        allocations.push(json!({"dimensionId":d,"memberId":m,"amount":wire}));
    }
    if total != account_money {
        return Err(fail("DIMENSION_BUDGET_DOES_NOT_RECONCILE"));
    }
    allocations.sort_by_key(budget_canonical);
    let mut plan = json!({"schemaVersion":"rentemester-dimension-budget-v1","accountNo":account,"period":input.period,"accountBudget":account_wire,"allocations":allocations,"sourceRef":source});
    plan["planHash"] = json!(format!(
        "{:x}",
        Sha256::digest(budget_canonical(&plan).as_bytes())
    ));
    Ok(json!({"ok":true,"plan":plan}))
}
// Numeric amounts are bounded exact cents, so fixed decimal spelling is JS JSON spelling.
fn budget_canonical(v: &Value) -> String {
    match v {
        Value::Number(n) => exact(n)
            .map(|(m, _)| {
                m.decimal()
                    .trim_end_matches('0')
                    .trim_end_matches('.')
                    .to_owned()
            })
            .unwrap_or_else(|_| n.to_string()),
        Value::Object(o) => {
            let mut keys = o.keys().collect::<Vec<_>>();
            keys.sort();
            format!(
                "{{{}}}",
                keys.into_iter()
                    .map(|k| format!("{}:{}", json!(k), budget_canonical(&o[k])))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
        Value::Array(a) => format!(
            "[{}]",
            a.iter().map(budget_canonical).collect::<Vec<_>>().join(",")
        ),
        _ => v.to_string(),
    }
}
pub fn budget_apply(
    tx: &Transaction<'_>,
    input: &BudgetInput,
    actor: &str,
) -> Result<Value, Error> {
    if input.confirm != Some(true) {
        return Err(fail("CONFIRMATION_REQUIRED"));
    }
    let by = text(actor, 160)?;
    let planned = budget_plan(tx, input)?;
    let p = &planned["plan"];
    if p["planHash"].as_str() != input.plan_hash.as_deref() {
        return Err(fail("PLAN_HASH_MISMATCH"));
    }
    let allocations = p["allocations"]
        .as_array()
        .ok_or_else(|| invalid("budget plan"))?;
    let key = input
        .idempotency_key
        .as_deref()
        .map(|s| {
            if s.trim() != s {
                return Err(fail("INVALID_IDEMPOTENCY_KEY"));
            }
            text(s, 200)
        })
        .transpose()?;
    if let Some(key) = &key {
        // Exact prefix, not LIKE: caller '%' and '_' are literal. Require the entire indexed set.
        let prefix = format!("{key}:");
        let prior = budget_rows(
            tx,
            "SELECT * FROM accounting_dimension_budget_events WHERE substr(idempotency_key,1,length(?1))=?1 ORDER BY id",
            [&prefix],
        )?;
        if !prior.is_empty() {
            if prior.len() != allocations.len() {
                return Err(fail("IDEMPOTENCY_CONFLICT"));
            }
            let mut ids = vec![];
            for (i, a) in allocations.iter().enumerate() {
                let expected = format!("{prefix}{i}");
                let row = prior
                    .iter()
                    .find(|r| r["idempotency_key"] == expected)
                    .ok_or_else(|| fail("IDEMPOTENCY_CONFLICT"))?;
                if row["plan_hash"] != p["planHash"]
                    || row["event_type"] != "allocated"
                    || row["account_no"] != p["accountNo"]
                    || row["period"] != p["period"]
                    || row["dimension_id"] != a["dimensionId"]
                    || row["member_id"] != a["memberId"]
                    || row["amount"] != a["amount"]
                    || row["source_ref"] != p["sourceRef"]
                {
                    return Err(fail("IDEMPOTENCY_CONFLICT"));
                }
                ids.push(row["id"].clone());
            }
            return Ok(json!({"ok":true,"ids":ids,"idempotent":true,"errors":[]}));
        }
    }
    let current = budget_rows(
        tx,
        "SELECT * FROM current_accounting_dimension_budgets WHERE account_no=?1 AND period=?2 ORDER BY id",
        params![p["accountNo"].as_str(), p["period"].as_str()],
    )?;
    let created = now(tx)?;
    for r in current {
        tx.execute("INSERT INTO accounting_dimension_budget_events(dimension_id,member_id,account_no,period,amount,source_ref,plan_hash,event_type,supersedes_budget_id,reason,actor,principal,created_at)VALUES(?1,?2,?3,?4,?5,?6,?7,'superseded',?8,'reviewed allocation replaced',?9,?9,?10)",params![r["dimension_id"].as_str(),r["member_id"].as_str(),r["account_no"].as_str(),r["period"].as_str(),r["amount"].as_f64(),r["source_ref"].as_str(),r["plan_hash"].as_str(),r["id"].as_i64(),by,created]).map_err(invalid)?;
    }
    let mut ids = vec![];
    for (i, a) in allocations.iter().enumerate() {
        let indexed = key.as_ref().map(|k| format!("{k}:{i}"));
        tx.execute("INSERT INTO accounting_dimension_budget_events(dimension_id,member_id,account_no,period,amount,source_ref,plan_hash,event_type,idempotency_key,actor,principal,created_at)VALUES(?1,?2,?3,?4,?5,?6,?7,'allocated',?8,?9,?9,?10)",params![a["dimensionId"].as_str(),a["memberId"].as_str(),p["accountNo"].as_str(),p["period"].as_str(),a["amount"].as_f64(),p["sourceRef"].as_str(),p["planHash"].as_str(),indexed,by,created]).map_err(invalid)?;
        ids.push(tx.last_insert_rowid());
    }
    Ok(json!({"ok":true,"ids":ids,"idempotent":false,"errors":[]}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_source_hash_and_whole_replacement_literal_key_recovery() {
        let mut control = rusqlite::Connection::open_in_memory().unwrap();
        let mut ledger = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut ledger, "Synthetic dimensions")
            .unwrap();
        ledger.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('X','Synthetic expense','expense','debit'); INSERT INTO budget_lines(account_no,period,amount)VALUES('X','2026-06',20);").unwrap();
        let tx = ledger.transaction().unwrap();
        for v in [
            json!({"confirm":true,"dimensionId":"project","kind":"project","name":"Synthetic project"}),
            json!({"confirm":true,"dimensionId":"project","memberId":"alpha","name":"Alpha"}),
            json!({"confirm":true,"dimensionId":"project","memberId":"beta","name":"Beta"}),
        ] {
            let input: MasterInput = serde_json::from_value(v.clone()).unwrap();
            master(
                &tx,
                if v.get("memberId").is_some() {
                    "member"
                } else {
                    "define"
                },
                &input,
                "user:synthetic",
            )
            .unwrap();
        }
        let mut input:BudgetInput=serde_json::from_value(json!({"accountNo":"X","period":"2026-06","sourceRef":"review:synthetic","allocations":[{"dimensionId":"project","memberId":"alpha","amount":12.0},{"dimensionId":"project","memberId":"beta","amount":8.0}],"confirm":true,"idempotencyKey":"literal%_"})).unwrap();
        let plan = budget_plan(&tx, &input).unwrap();
        // Independently generated with Node crypto + source stableJson (not Rust serialization).
        assert_eq!(
            plan["plan"]["planHash"],
            "73bbd87ac552ddcde101d0b2f8d9c32cbdd1a1bf6ee4dcd94ccf22b2903057bf"
        );
        input.plan_hash = plan["plan"]["planHash"].as_str().map(str::to_owned);
        let first = budget_apply(&tx, &input, "user:synthetic").unwrap();
        input.allocations[0].amount = 13.into();
        input.allocations[1].amount = 7.into();
        input.idempotency_key = Some("literalZZ".into());
        input.plan_hash = budget_plan(&tx, &input).unwrap()["plan"]["planHash"]
            .as_str()
            .map(str::to_owned);
        budget_apply(&tx, &input, "user:synthetic").unwrap();
        assert_eq!(
            tx.query_row(
                "SELECT COUNT(*) FROM accounting_dimension_budget_events",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            6
        );
        assert_eq!(
            budgets(&tx).unwrap()["budgets"].as_array().unwrap().len(),
            2
        );
        input.allocations[0].amount = 12.into();
        input.allocations[1].amount = 8.into();
        input.idempotency_key = Some("literal%_".into());
        input.plan_hash = plan["plan"]["planHash"].as_str().map(str::to_owned);
        assert_eq!(
            budget_apply(&tx, &input, "user:synthetic").unwrap()["ids"],
            first["ids"]
        );
        assert_eq!(
            tx.query_row(
                "SELECT COUNT(*) FROM accounting_dimension_budget_events",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            6
        );
        input.idempotency_key = Some(" literal%_ ".into());
        assert!(budget_apply(&tx, &input, "user:synthetic").is_err());
        input.allocations[0].amount = serde_json::Number::from_f64(12.001).unwrap();
        assert!(budget_plan(&tx, &input).is_err());
        assert!(exact(&serde_json::Number::from_f64(0.001).unwrap()).is_err());
        assert!(exact(&serde_json::Number::from_f64(90071992547410.0).unwrap()).is_err());
    }
}
