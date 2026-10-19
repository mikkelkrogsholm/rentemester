use super::*;
use rusqlite::Connection;
fn fixture() -> Connection {
    let mut c = Connection::open_in_memory().unwrap();
    let mut l = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic Dimensions").unwrap();
    l.execute_batch("INSERT INTO accounts(id,account_no,name,type,normal_balance)VALUES(1,'A','Synthetic asset','asset','debit'),(2,'E','Synthetic equity','equity','credit');INSERT INTO journal_entries(id,entry_no,transaction_date,text,rule_version,entry_hash)VALUES(1,'SYN-1','2026-01-01','Synthetic opening','fixture','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa');INSERT INTO journal_lines(id,journal_entry_id,account_id,debit_amount,credit_amount)VALUES(1,1,1,100.01,0),(2,1,2,0,100.01);").unwrap();
    {
        let tx = l.transaction().unwrap();
        for dimension in ["project", "department"] {
            let d:MasterInput=serde_json::from_value(json!({"confirm":true,"dimensionId":dimension,"kind":dimension,"name":"Synthetic dimension"})).unwrap();
            master(&tx, "define", &d, "user:synthetic").unwrap();
            for m in ["alpha", "beta"] {
                let d: MasterInput = serde_json::from_value(
                    json!({"confirm":true,"dimensionId":dimension,"memberId":m,"name":m}),
                )
                .unwrap();
                master(&tx, "member", &d, "user:synthetic").unwrap();
            }
        }
        tx.commit().unwrap();
    }
    l
}
fn input(member: &str) -> AssignmentInput {
    serde_json::from_value(json!({"confirm":true,"journalLineId":1,"source":"reviewed","allocations":[{"dimensionId":"project","memberId":member,"amountMinor":10001,"currency":"DKK"}]})).unwrap()
}
fn reviewed(tx: &Transaction<'_>, member: &str) -> AssignmentInput {
    let mut i = input(member);
    i.plan_hash = Some(
        plan(tx, &i).unwrap()["plan"]["planHash"]
            .as_str()
            .unwrap()
            .to_owned(),
    );
    i
}
#[test]
fn exact_minor_reconciliation_for_each_dimension_and_import_provenance() {
    let mut l = fixture();
    let tx = l.transaction().unwrap();
    let mut i = input("alpha");
    let a = &mut i.allocations[0];
    a.amount_minor = 10000;
    assert_eq!(
        plan(&tx, &i).unwrap_err().message,
        "ALLOCATION_DOES_NOT_RECONCILE"
    );
    i.allocations.push(Allocation {
        dimension_id: "project".into(),
        member_id: "beta".into(),
        amount_minor: 1,
        currency: "DKK".into(),
    });
    i.allocations.push(Allocation {
        dimension_id: "department".into(),
        member_id: "alpha".into(),
        amount_minor: 10001,
        currency: "DKK".into(),
    });
    let p = plan(&tx, &i).unwrap();
    assert_eq!(p["plan"]["lineAmountMinor"], 10001);
    assert_eq!(p["plan"]["journalEntryHash"], "a".repeat(64));
    assert_eq!(p["plan"]["allocations"].as_array().unwrap().len(), 3);
    i.source = Some("imported".into());
    assert_eq!(
        plan(&tx, &i).unwrap_err().message,
        "IMPORTED_PROVENANCE_REQUIRES_REVIEW"
    );
    i.reviewed_import = Some(true);
    i.source_ref = Some("Synthetic import row 4".into());
    assert_eq!(
        plan(&tx, &i).unwrap()["plan"]["sourceRef"],
        "Synthetic import row 4"
    );
    i.allocations[0].currency = "EUR".into();
    assert_eq!(plan(&tx, &i).unwrap_err().message, "INVALID_ALLOCATION");
}
#[test]
fn assignment_replacement_is_hash_bound_reasoned_idempotent_and_preserves_journal() {
    let mut l = fixture();
    let tx = l.transaction().unwrap();
    let before:(String,f64,f64)=tx.query_row("SELECT je.entry_hash,jl.debit_amount,jl.credit_amount FROM journal_entries je JOIN journal_lines jl ON jl.journal_entry_id=je.id WHERE jl.id=1",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
    let mut first = reviewed(&tx, "alpha");
    first.plan_hash = Some("b".repeat(64));
    assert_eq!(
        apply(&tx, &first, false, "user:synthetic")
            .unwrap_err()
            .message,
        "PLAN_HASH_MISMATCH"
    );
    first = reviewed(&tx, "alpha");
    first.idempotency_key = Some("synthetic-first".into());
    let original = apply(&tx, &first, false, "user:synthetic").unwrap();
    assert_eq!(
        apply(&tx, &first, false, "user:synthetic").unwrap()["idempotent"],
        true
    );
    let mut replacement = reviewed(&tx, "beta");
    assert_eq!(
        apply(&tx, &replacement, false, "user:synthetic")
            .unwrap_err()
            .message,
        "CURRENT_ASSIGNMENT_REQUIRES_SUPERSESSION"
    );
    replacement.expected_assignment_id = Some(999);
    replacement.reason = Some("Synthetic correction".into());
    assert_eq!(
        apply(&tx, &replacement, true, "user:reviewer")
            .unwrap_err()
            .message,
        "CURRENT_ASSIGNMENT_CONFLICT"
    );
    replacement.expected_assignment_id = original["id"].as_i64();
    replacement.idempotency_key = Some("synthetic-replace".into());
    let replaced = apply(&tx, &replacement, true, "user:reviewer").unwrap();
    assert_eq!(
        apply(&tx, &replacement, true, "user:reviewer").unwrap()["idempotent"],
        true
    );
    let history = assignments(&tx, 1).unwrap();
    let events = history["assignments"].as_array().unwrap();
    assert_eq!(events.len(), 3);
    assert_eq!(events[0]["id"], original["id"]);
    assert_eq!(events[1]["supersedes_assignment_id"], original["id"]);
    assert_eq!(events[1]["reason"], "Synthetic correction");
    assert_eq!(events[1]["actor"], "user:reviewer");
    assert_eq!(events[2]["id"], replaced["id"]);
    assert_eq!(events[0]["journal_entry_hash"], "a".repeat(64));
    assert!(
        tx.execute(
            "UPDATE accounting_dimension_assignment_events SET reason='changed' WHERE id=1",
            []
        )
        .is_err()
    );
    assert!(
        tx.execute(
            "DELETE FROM accounting_dimension_assignment_events WHERE id=1",
            []
        )
        .is_err()
    );
    let after=tx.query_row("SELECT je.entry_hash,jl.debit_amount,jl.credit_amount FROM journal_entries je JOIN journal_lines jl ON jl.journal_entry_id=je.id WHERE jl.id=1",[],|r|Ok((r.get::<_,String>(0)?,r.get::<_,f64>(1)?,r.get::<_,f64>(2)?))).unwrap();
    assert_eq!(before, after);
}
#[test]
fn preview_is_read_only_and_lifecycle_is_revalidated_at_apply() {
    let mut l = fixture();
    let before = l.total_changes();
    let tx = l.transaction().unwrap();
    let i = reviewed(&tx, "alpha");
    assert_eq!(tx.total_changes(), before);
    let lifecycle: MasterInput = serde_json::from_value(
        json!({"confirm":true,"dimensionId":"project","memberId":"alpha","action":"deactivate"}),
    )
    .unwrap();
    master(&tx, "member-lifecycle", &lifecycle, "user:synthetic").unwrap();
    assert_eq!(
        apply(&tx, &i, false, "user:synthetic").unwrap_err().message,
        "INACTIVE_MEMBER_REQUIRES_REVIEWED_OVERRIDE"
    );
    assert_eq!(assignments(&tx, 1).unwrap()["assignments"], json!([]));
    let m = members(&tx, Some("project")).unwrap();
    assert_eq!(m["members"].as_array().unwrap().len(), 3);
    assert_eq!(m["members"][0]["status"], "active");
    assert_eq!(m["members"][1]["status"], "inactive");
    let invalid:MasterInput=serde_json::from_value(json!({"confirm":true,"dimensionId":"project","memberId":"bad-date","name":"Synthetic","effectiveFrom":"2026-02-30"})).unwrap();
    assert!(master(&tx, "member", &invalid, "user:synthetic").is_err());
}
#[test]
fn failed_replacement_rolls_back_retirement_and_preserves_current_assignment() {
    let mut l = fixture();
    let (first_hash, second_id) = {
        let tx = l.transaction().unwrap();
        let first = reviewed(&tx, "alpha");
        let first_id = apply(&tx, &first, false, "user:synthetic").unwrap()["id"].as_i64();
        let mut second = reviewed(&tx, "beta");
        second.expected_assignment_id = first_id;
        second.reason = Some("Synthetic replacement".into());
        let result = apply(&tx, &second, true, "user:synthetic").unwrap();
        tx.commit().unwrap();
        (first.plan_hash.unwrap(), result["id"].as_i64().unwrap())
    };
    {
        let tx = l.transaction().unwrap();
        let mut third = input("alpha");
        third.plan_hash = Some(first_hash);
        third.expected_assignment_id = Some(second_id);
        third.reason = Some("Synthetic return to first allocation".into());
        assert!(apply(&tx, &third, true, "user:synthetic").is_err());
    } // caller/fence drops the entire failed transaction.
    let tx = l.transaction().unwrap();
    assert_eq!(
        assignments(&tx, 1).unwrap()["assignments"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        rows(
            &tx,
            "SELECT id FROM current_accounting_dimension_assignments WHERE journal_line_id=1",
            []
        )
        .unwrap()[0]["id"],
        second_id
    );
}
