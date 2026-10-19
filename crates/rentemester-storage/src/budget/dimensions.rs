//! Dimension drilldown keeps account totals visible and never allocates an account budget by inference.
use super::*;
use crate::dimensions::Allocation;
use std::collections::BTreeSet;

#[derive(Default)]
struct Actual {
    selected: Money,
    classified: Money,
    lines: BTreeSet<i64>,
}
fn natural(amount: &Money, kind: &str) -> Money {
    if kind == "expense" {
        amount.clone()
    } else {
        Money::default().subtract(amount)
    }
}
/// Optional exact selectors; a member must always be scoped to its dimension.
pub fn read(
    tx: &Transaction<'_>,
    slug: &str,
    today: &str,
    year: Option<&str>,
    dimension: Option<&str>,
    member: Option<&str>,
) -> Result<Value, Error> {
    if member.is_some() && dimension.is_none() {
        return Err(refused("member requires dimension"));
    }
    for value in [dimension, member].into_iter().flatten() {
        if value.is_empty()
            || value.len() > 64
            || value.trim() != value
            || value.chars().any(char::is_control)
        {
            return Err(refused("invalid dimension selector"));
        }
    }
    let mut base = super::read(tx, slug, today, year, true)?;
    let from = base["periodStart"].as_str().unwrap().to_owned();
    let to = base["periodEnd"].as_str().unwrap().to_owned();
    let mut options = vec![];
    let mut known = BTreeSet::new();
    let mut q = tx.prepare("SELECT d.dimension_id,d.name,m.member_id,m.name FROM current_accounting_dimension_definitions d LEFT JOIN current_accounting_dimension_members m ON m.dimension_id=d.dimension_id ORDER BY d.dimension_id,m.member_id").map_err(invalid)?;
    for r in q
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })
        .map_err(invalid)?
    {
        let (id, name, m, mname) = r.map_err(invalid)?;
        if known.insert((id.clone(), None)) {
            options.push(json!({"value":id,"label":name,"dimensionId":id,"memberId":null}));
        }
        if let Some(m) = m {
            known.insert((id.clone(), Some(m.clone())));
            options.push(json!({"value":format!("{id}:{m}"),"label":format!("{name}: {}",mname.unwrap_or_else(||m.clone())),"dimensionId":id,"memberId":m}));
        }
    }
    if let Some(d) = dimension
        && !known.contains(&(d.to_owned(), member.map(str::to_owned)))
    {
        return Err(refused("unknown dimension or member"));
    }
    let mut actuals: BTreeMap<(String, String), Actual> = BTreeMap::new();
    let mut raw = vec![];
    let mut notes = vec![];
    let mut q = tx.prepare("SELECT a.account_no,a.type,substr(j.transaction_date,1,7),l.id,l.debit_amount,l.credit_amount,l.currency,j.id,j.entry_hash,d.id,d.journal_entry_id,d.journal_entry_hash,d.line_currency,d.line_amount_minor,d.allocations_json FROM journal_lines l JOIN journal_entries j ON j.id=l.journal_entry_id JOIN accounts a ON a.id=l.account_id JOIN current_accounting_dimension_assignments d ON d.journal_line_id=l.id WHERE j.status IN ('posted','reversed') AND substr(j.transaction_date,1,7) BETWEEN ?1 AND ?2 ORDER BY l.id,d.id").map_err(invalid)?;
    let mut rows = q.query(params![from, to]).map_err(invalid)?;
    let mut seen = BTreeSet::new();
    while let Some(r) = rows.next().map_err(invalid)? {
        let no: String = r.get(0).map_err(invalid)?;
        let kind: String = r.get(1).map_err(invalid)?;
        let period: String = r.get(2).map_err(invalid)?;
        let line: i64 = r.get(3).map_err(invalid)?;
        if !seen.insert(line) {
            return Err(refused("multiple current assignments on one line"));
        }
        let net = legacy_money(r.get_ref(4).map_err(invalid)?)?
            .subtract(&legacy_money(r.get_ref(5).map_err(invalid)?)?);
        let currency: String = r.get(6).map_err(invalid)?;
        let minor: i64 = r.get(13).map_err(invalid)?;
        let amount = Money::parse(&minor.to_string())?.percent("1")?;
        let magnitude = if net.is_negative() {
            Money::default().subtract(&net)
        } else {
            net.clone()
        };
        if r.get::<_, i64>(7).map_err(invalid)? != r.get::<_, i64>(10).map_err(invalid)?
            || r.get::<_, String>(8).map_err(invalid)? != r.get::<_, String>(11).map_err(invalid)?
            || currency != r.get::<_, String>(12).map_err(invalid)?
            || amount != magnitude
        {
            notes.push(format!(
                "Journal-linje {line}: tildelingens kildebevis stemmer ikke; beløbet er ufordelt."
            ));
            continue;
        }
        let allocations: Vec<Allocation> =
            serde_json::from_str(&r.get::<_, String>(14).map_err(invalid)?).map_err(invalid)?;
        let mut totals: BTreeMap<&str, Money> = BTreeMap::new();
        for a in &allocations {
            if a.amount_minor <= 0 || a.currency != currency {
                return Err(refused("invalid current dimension amount/currency"));
            }
            totals
                .entry(&a.dimension_id)
                .or_default()
                .add(&Money::parse(&a.amount_minor.to_string())?.percent("1")?);
        }
        if totals.values().any(|v| v != &magnitude) {
            return Err(refused("current assignment does not reconcile"));
        }
        for a in allocations {
            let amount = Money::parse(&a.amount_minor.to_string())?.percent("1")?;
            let signed = natural(
                &if net.is_negative() {
                    Money::default().subtract(&amount)
                } else {
                    amount
                },
                &kind,
            );
            if dimension.is_none_or(|d| d == a.dimension_id)
                && member.is_none_or(|m| m == a.member_id)
            {
                raw.push(json!({"dimensionId":a.dimension_id,"memberId":a.member_id,"accountNo":no,"period":period,"actual":wire(&signed)?,"journalLineId":line}));
            }
            if dimension == Some(a.dimension_id.as_str()) {
                let cell = actuals.entry((no.clone(), period.clone())).or_default();
                cell.classified.add(&signed);
                if member.is_none_or(|m| m == a.member_id) {
                    cell.selected.add(&signed);
                    cell.lines.insert(line);
                }
            }
        }
    }
    // Existing reviewed dimension budgets are usable only if each full dimension set
    // reconciles exactly to the latest account plan. A later account revision invalidates it.
    type ReviewedBudgets = BTreeMap<(String, String, String), Vec<(String, Money, String, i64)>>;
    let mut budgets: ReviewedBudgets = BTreeMap::new();
    let mut q=tx.prepare("SELECT dimension_id,member_id,account_no,period,amount,source_ref,id FROM current_accounting_dimension_budgets WHERE period BETWEEN ?1 AND ?2 ORDER BY dimension_id,member_id,account_no,period,id").map_err(invalid)?;
    let mut rows = q.query(params![from, to]).map_err(invalid)?;
    while let Some(r) = rows.next().map_err(invalid)? {
        budgets
            .entry((
                r.get(0).map_err(invalid)?,
                r.get(2).map_err(invalid)?,
                r.get(3).map_err(invalid)?,
            ))
            .or_default()
            .push((
                r.get(1).map_err(invalid)?,
                legacy_money(r.get_ref(4).map_err(invalid)?)?,
                r.get(5).map_err(invalid)?,
                r.get(6).map_err(invalid)?,
            ));
    }
    let mut comparison = vec![];
    let mut dimension_budgets = vec![];
    let account_lines = base["lines"].as_array().unwrap();
    let mut account_totals = vec![];
    if let Some(dimension) = dimension {
        for account in account_lines {
            let no = account["accountNo"].as_str().unwrap();
            let period = account["period"].as_str().unwrap();
            let actual = actuals
                .entry((no.to_owned(), period.to_owned()))
                .or_default();
            let account_actual = Money::parse(&account["actual"].to_string())?;
            let account_budget = Money::parse(&account["budget"].to_string())?;
            let mut reviewed = None;
            if let Some(set) =
                budgets.get(&(dimension.to_owned(), no.to_owned(), period.to_owned()))
            {
                let mut sum = Money::default();
                let mut members = BTreeSet::new();
                for (m, amount, _, _) in set {
                    sum.add(amount);
                    members.insert(m);
                }
                if sum == account_budget && members.len() == set.len() {
                    let mut selected = Money::default();
                    for (m, amount, source, id) in set {
                        if member.is_none_or(|chosen| chosen == m) {
                            selected.add(amount);
                            dimension_budgets.push(json!({"dimensionId":dimension,"memberId":m,"accountNo":no,"period":period,"budget":wire(amount)?,"sourceRef":source,"id":id}));
                        }
                    }
                    reviewed = Some(selected);
                } else {
                    notes.push(format!("Konto {no} {period}: tidligere dimensionsbudget stemmer ikke med seneste konto-budget; variance er ukendt."));
                }
            }
            let variance = reviewed.as_ref().map(|budget| {
                if account["accountType"] == "expense" {
                    budget.subtract(&actual.selected)
                } else {
                    actual.selected.subtract(budget)
                }
            });
            comparison.push(json!({"accountNo":no,"period":period,"actual":wire(&actual.selected)?,"accountActual":account["actual"],"accountBudget":account["budget"],"unallocatedActual":wire(&account_actual.subtract(&actual.classified))?,"otherMembersActual":wire(&actual.classified.subtract(&actual.selected))?,"budget":reviewed.as_ref().map(wire).transpose()?,"variance":variance.as_ref().map(wire).transpose()?,"journalLineIds":actual.lines}));
            account_totals.push(json!({"accountNo":no,"period":period,"actual":account["actual"]}));
        }
    }
    base.as_object_mut().unwrap().remove("lines");
    for key in ["totalActual", "totalBudget", "totalVariance"] {
        base.as_object_mut().unwrap().remove(key);
    }
    base["rows"] = json!(raw);
    base["comparison"] = json!(comparison);
    base["accountTotals"] = json!(account_totals);
    base["dimensionBudgets"] = json!(dimension_budgets);
    base["dimensionOptions"] = json!(options);
    base["budgetScope"] = json!("account");
    base["selection"] = json!({"dimensionId":dimension,"memberId":member});
    base["notes"] = json!(notes);
    base["serverCalculated"] = json!(true);
    Ok(base)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dimensions::{self, AssignmentInput, MasterInput};
    fn fixture() -> rusqlite::Connection {
        let mut c = rusqlite::Connection::open_in_memory().unwrap();
        let mut l = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic dimension budget").unwrap();
        l.execute_batch("UPDATE companies SET fiscal_year_start_month=7,fiscal_year_label_strategy='span'; INSERT INTO accounts(id,account_no,name,type,normal_balance)VALUES(1,'X','Synthetic expense','expense','debit'),(2,'E','Synthetic equity','equity','credit');INSERT INTO journal_entries(id,entry_no,transaction_date,text,rule_version,entry_hash)VALUES(1,'B1','2026-06-01','Classified expense','fixture','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'),(2,'B2','2026-06-02','Unclassified expense','fixture','bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb'),(3,'B3','2026-07-01','Next fiscal year','fixture','cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc');INSERT INTO journal_lines(id,journal_entry_id,account_id,debit_amount,credit_amount)VALUES(1,1,1,12.34,0),(2,1,2,0,12.34),(3,2,1,3.21,0),(4,2,2,0,3.21),(5,3,1,9,0),(6,3,2,0,9);").unwrap();
        {
            let tx = l.transaction().unwrap();
            for value in [
                json!({"confirm":true,"dimensionId":"project","kind":"project","name":"Synthetic Project"}),
                json!({"confirm":true,"dimensionId":"project","memberId":"alpha","name":"Alpha"}),
                json!({"confirm":true,"dimensionId":"project","memberId":"beta","name":"Beta"}),
            ] {
                let input: MasterInput = serde_json::from_value(value.clone()).unwrap();
                dimensions::master(
                    &tx,
                    if value.get("memberId").is_some() {
                        "member"
                    } else {
                        "define"
                    },
                    &input,
                    "user:synthetic",
                )
                .unwrap();
            }
            super::super::set(
                &tx,
                &serde_json::from_value(json!({"accountNo":"X","period":"2026-06","amount":20}))
                    .unwrap(),
                "synthetic",
            )
            .unwrap();
            tx.commit().unwrap();
        }
        l
    }
    fn assignment(tx: &Transaction<'_>, beta: bool) -> AssignmentInput {
        let mut input:AssignmentInput=serde_json::from_value(json!({"confirm":true,"journalLineId":1,"allocations":if beta {json!([{"dimensionId":"project","memberId":"beta","amountMinor":1234,"currency":"DKK"}])}else{json!([{"dimensionId":"project","memberId":"alpha","amountMinor":1001,"currency":"DKK"},{"dimensionId":"project","memberId":"beta","amountMinor":233,"currency":"DKK"}])}})).unwrap();
        input.plan_hash = Some(
            dimensions::plan(tx, &input).unwrap()["plan"]["planHash"]
                .as_str()
                .unwrap()
                .into(),
        );
        input
    }
    fn report(tx: &Transaction<'_>, member: Option<&str>) -> Value {
        read(
            tx,
            "synthetic",
            "2026-10-19",
            Some("2025/26"),
            Some("project"),
            member,
        )
        .unwrap()
    }
    #[test]
    fn exact_member_scope_unallocated_replaced_and_stale_evidence() {
        let mut l = fixture();
        let tx = l.transaction().unwrap();
        let before = tx.total_changes();
        let first =
            dimensions::apply(&tx, &assignment(&tx, false), false, "user:synthetic").unwrap();
        let initial = report(&tx, Some("alpha"));
        let row = &initial["comparison"][0];
        assert_eq!(row["actual"], 10.01);
        assert_eq!(row["otherMembersActual"], 2.33);
        assert_eq!(row["unallocatedActual"], 3.21);
        assert_eq!(row["accountActual"], 15.55);
        assert_eq!(row["accountBudget"], 20.0);
        assert!(row["budget"].is_null() && row["variance"].is_null());
        assert_eq!(report(&tx, None)["comparison"][0]["actual"], 12.34);
        let mut replacement = assignment(&tx, true);
        replacement.expected_assignment_id = first["id"].as_i64();
        replacement.reason = Some("Synthetic review corrected project member".into());
        dimensions::apply(&tx, &replacement, true, "user:synthetic").unwrap();
        let corrected = report(&tx, Some("alpha"));
        assert_eq!(corrected["comparison"][0]["actual"], 0.0);
        assert_eq!(corrected["comparison"][0]["unallocatedActual"], 3.21);
        assert!(dimensions::apply(&tx, &replacement, true, "user:synthetic").is_err());
        assert_eq!(report(&tx, Some("alpha")), corrected);
        assert_eq!(report(&tx, Some("beta"))["comparison"][0]["actual"], 12.34);
        let changes = tx.total_changes();
        assert_eq!(
            read(
                &tx,
                "synthetic",
                "2026-10-19",
                Some("2025/26"),
                Some("project"),
                Some("beta")
            )
            .unwrap()["periodEnd"],
            "2026-06"
        );
        assert_eq!(tx.total_changes(), changes);
        assert!(changes > before);
        assert!(read(&tx, "synthetic", "2026-10-19", None, None, Some("alpha")).is_err());
        assert!(read(&tx, "synthetic", "2026-10-19", None, Some("unknown"), None).is_err());
        assert_eq!(
            read(
                &tx,
                "synthetic",
                "2026-10-19",
                Some("2026/27"),
                Some("project"),
                None
            )
            .unwrap()["comparison"][0]["accountActual"],
            9.0
        );
        assert_eq!(
            tx.query_row("SELECT COUNT(*) FROM journal_entries", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            3
        );
        tx.commit().unwrap();
        let tx = l.transaction().unwrap();
        assert_eq!(report(&tx, Some("beta"))["comparison"][0]["actual"], 12.34);
    }
    #[test]
    fn reviewed_budget_requires_current_full_reconciliation_and_stale_line_is_disclosed() {
        let mut l = fixture();
        let tx = l.transaction().unwrap();
        dimensions::apply(&tx, &assignment(&tx, false), false, "user:synthetic").unwrap();
        tx.execute_batch("INSERT INTO accounting_dimension_budget_events(dimension_id,member_id,account_no,period,amount,source_ref,plan_hash,event_type,actor,principal,created_at)VALUES('project','alpha','X','2026-06',12,'review:synthetic','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa','allocated','user:synthetic','user:synthetic','2026-06-01'),('project','beta','X','2026-06',8,'review:synthetic','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa','allocated','user:synthetic','user:synthetic','2026-06-01');").unwrap();
        let r = report(&tx, Some("alpha"));
        assert_eq!(r["comparison"][0]["budget"], 12.0);
        assert_eq!(r["comparison"][0]["variance"], 1.99);
        super::super::set(
            &tx,
            &serde_json::from_value(json!({"accountNo":"X","period":"2026-06","amount":21}))
                .unwrap(),
            "synthetic",
        )
        .unwrap();
        let r = report(&tx, Some("alpha"));
        assert!(r["comparison"][0]["budget"].is_null());
        assert!(!r["notes"].as_array().unwrap().is_empty());
    }
}
