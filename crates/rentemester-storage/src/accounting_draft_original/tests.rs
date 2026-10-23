use super::*;
use rusqlite::{Connection, TransactionBehavior};
const AUTHOR: &str = "service_account:synthetic";
const SUBMITTER: &str = "user:submitter";
const REVIEWER: &str = "user:reviewer";
fn latest() -> Date {
    Date::parse("2026-02-15").unwrap()
}
fn fixture() -> Connection {
    let mut control = Connection::open_in_memory().unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic draft").unwrap();
    db.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('3729','Synthetic expense','expense','debit'),('8729','Synthetic equity','equity','credit')").unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let metadata = serde_json::from_value(json!({"source":"synthetic","documentType":"cash_register_receipt","issueDate":"2026-01-01","amountIncVat":12.5,"vatAmount":0,"currency":"DKK"})).unwrap();
    let hash = "a".repeat(64);
    crate::documents::register(
        &tx,
        crate::documents::Original {
            filename: "synthetic.txt",
            hash: &hash,
            path: &format!("documents/originals/{hash}.txt"),
            mime: "text/plain",
        },
        &metadata,
        SUBMITTER,
        "2026-01-01",
    )
    .unwrap();
    tx.commit().unwrap();
    db
}
fn payload() -> Value {
    json!({"documentId":1,"transactionDate":"2026-01-01","text":"Synthetic receipt","currency":"DKK","lines":[{"accountNo":"3729","debitAmount":12.5},{"accountNo":"8729","creditAmount":12.5}]})
}
fn run(db: &mut Connection, op: Operation, input: Value, actor: &str) -> Value {
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let value = execute(&tx, op, &input, actor, latest()).unwrap();
    tx.commit().unwrap();
    value
}
fn snapshot(db: &Connection) -> Vec<String> {
    let mut q = db
        .prepare("SELECT name FROM sqlite_schema WHERE type='table' ORDER BY name")
        .unwrap();
    let names = q
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let mut result = vec![];
    for name in names {
        let mut q = db
            .prepare(&format!(
                "SELECT * FROM \"{}\" ORDER BY rowid",
                name.replace('"', "\"\"")
            ))
            .unwrap();
        let count = q.column_count();
        let mut rows = q.query([]).unwrap();
        while let Some(r) = rows.next().unwrap() {
            result.push(format!(
                "{name}:{:?}",
                (0..count)
                    .map(|i| format!("{:?}", r.get_ref(i).unwrap()))
                    .collect::<Vec<_>>()
            ));
        }
    }
    result
}
fn transition(db: &mut Connection, op: Operation, state: &Value, actor: &str) -> Value {
    let mut input =
        json!({"draftId":"draft-one","expectedEventHash":state["eventHash"],"confirm":true});
    if op == Operation::Revise {
        input["payload"] = state["payload"].clone();
        input["payload"]["text"] = json!(format!("Revision {}", state["version"]));
    }
    if op == Operation::Reject {
        input["reason"] = json!("  Review requested  ");
    }
    let result = run(db, op, input, actor);
    assert_eq!(result["ok"], true, "{result}");
    result["accountingDraft"].clone()
}
#[test]
fn seven_events_actual_rules_exact_replay_and_source_read_shapes() {
    let mut db = fixture();
    assert_eq!(
        run(
            &mut db,
            Operation::Show,
            json!({"draftId":"missing"}),
            SUBMITTER
        ),
        json!({"ok":true,"accountingDraft":null})
    );
    let created = run(
        &mut db,
        Operation::Create,
        json!({"draftId":" draft-one ","payload":payload(),"confirm":true}),
        AUTHOR,
    );
    assert_eq!(created["ok"], true, "{created}");
    let mut state = created["accountingDraft"].clone();
    assert!(state.get("reason").is_none());
    assert!(state.get("journalEntryId").is_none());
    state = transition(&mut db, Operation::Revise, &state, AUTHOR);
    state = transition(&mut db, Operation::Submit, &state, SUBMITTER);
    for actor in [AUTHOR, SUBMITTER] {
        let before = snapshot(&db);
        assert_eq!(
            run(
                &mut db,
                Operation::ApproveAndPost,
                json!({"draftId":"draft-one","expectedEventHash":state["eventHash"],"confirm":true}),
                actor
            )["ok"],
            false
        );
        assert_eq!(snapshot(&db), before);
    }
    state = transition(&mut db, Operation::Reject, &state, REVIEWER);
    assert_eq!(state["reason"], "Review requested");
    state = transition(&mut db, Operation::Revise, &state, AUTHOR);
    state = transition(&mut db, Operation::Submit, &state, SUBMITTER);
    let input =
        json!({"draftId":"draft-one","expectedEventHash":state["eventHash"],"confirm":true});
    state = transition(&mut db, Operation::ApproveAndPost, &state, REVIEWER);
    let receipt = &state["journal"];
    assert_eq!(receipt["ok"], true);
    assert!(!receipt["appliedRules"].as_array().unwrap().is_empty());
    let stored: (i64, String, String) = db
        .query_row(
            "SELECT id,entry_no,entry_hash FROM journal_entries",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(receipt["entryId"], stored.0);
    assert_eq!(receipt["entryNo"], stored.1);
    assert_eq!(receipt["entryHash"], stored.2);
    assert_eq!(state["journalEntryId"], receipt["entryId"]);
    let before = snapshot(&db);
    let replay = run(&mut db, Operation::ApproveAndPost, input.clone(), REVIEWER);
    assert_eq!(
        replay["accountingDraft"]["journal"]["appliedRules"],
        json!([])
    );
    for key in ["entryId", "entryNo", "entryHash"] {
        assert_eq!(replay["accountingDraft"]["journal"][key], receipt[key]);
    }
    assert_eq!(
        run(&mut db, Operation::ApproveAndPost, input, "user:other")["ok"],
        false
    );
    state.as_object_mut().unwrap().remove("journal");
    assert_eq!(
        run(
            &mut db,
            Operation::Show,
            json!({"draftId":"draft-one"}),
            SUBMITTER
        )["accountingDraft"],
        state
    );
    assert_eq!(
        run(&mut db, Operation::List, json!({}), SUBMITTER)["accountingDrafts"],
        json!([state])
    );
    assert_eq!(snapshot(&db), before);
    assert_eq!(
        db.query_row("SELECT count(*) FROM accounting_draft_events", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        7
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM journal_lines", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert!(crate::journal_hash::verify(&db).unwrap().ok);
}
#[test]
fn refusals_source_change_and_late_failure_are_atomic_even_on_outer_commit() {
    let mut db = fixture();
    let created = run(
        &mut db,
        Operation::Create,
        json!({"draftId":"draft-one","payload":payload(),"confirm":true}),
        AUTHOR,
    )["accountingDraft"]
        .clone();
    let before = snapshot(&db);
    assert_eq!(
        run(
            &mut db,
            Operation::Create,
            json!({"draftId":"draft-one","payload":payload(),"confirm":true}),
            AUTHOR
        )["ok"],
        false
    );
    assert_eq!(
        run(
            &mut db,
            Operation::Submit,
            json!({"draftId":"draft-one","expectedEventHash":"0".repeat(64),"confirm":true}),
            SUBMITTER
        )["ok"],
        false
    );
    assert_eq!(snapshot(&db), before);
    db.execute(
        "UPDATE documents SET supplier_name='Changed evidence' WHERE id=1",
        [],
    )
    .unwrap();
    let before = snapshot(&db);
    assert_eq!(
        run(
            &mut db,
            Operation::Submit,
            json!({"draftId":"draft-one","expectedEventHash":created["eventHash"],"confirm":true}),
            SUBMITTER
        )["ok"],
        false
    );
    assert_eq!(snapshot(&db), before);
    let revised = transition(&mut db, Operation::Revise, &created, AUTHOR);
    let submitted = transition(&mut db, Operation::Submit, &revised, SUBMITTER);
    // Fail after journal insertion, at approval append; every journal/sequence/
    // audit write must disappear even if the outer transaction is committed.
    db.execute_batch("CREATE TEMP TRIGGER synthetic_approval_failure BEFORE INSERT ON accounting_draft_events WHEN NEW.event_type='approved_posted' BEGIN SELECT RAISE(ABORT,'synthetic late failure'); END").unwrap();
    let before = snapshot(&db);
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    assert!(execute(&tx, Operation::ApproveAndPost, &json!({"draftId":"draft-one","expectedEventHash":submitted["eventHash"],"confirm":true}), REVIEWER, latest()).is_err());
    tx.commit().unwrap();
    assert_eq!(snapshot(&db), before);
}
#[test]
fn strict_fields_confirmation_numeric_payload_and_bounds() {
    for (op, input) in [
        (Operation::List, json!({"confirm":true})),
        (Operation::Show, json!({"draftId":"BAD"})),
        (
            Operation::Submit,
            json!({"draftId":"one","expectedEventHash":"A".repeat(64),"confirm":true}),
        ),
        (
            Operation::Reject,
            json!({"draftId":"one","expectedEventHash":"a".repeat(64),"confirm":true,"reason":" "}),
        ),
        (
            Operation::Create,
            json!({"draftId":"one","payload":payload(),"confirm":false}),
        ),
    ] {
        assert!(validate_input(op, &input).is_err());
    }
    for key in ["createdBy", "createdByProgram"] {
        let mut p = payload();
        p[key] = json!(AUTHOR);
        assert!(
            validate_input(
                Operation::Create,
                &json!({"draftId":"one","payload":p,"confirm":true})
            )
            .is_err()
        );
    }
    let mut p = payload();
    p["lines"][0]["debitAmount"] = json!("12.50");
    assert!(
        validate_input(
            Operation::Create,
            &json!({"draftId":"one","payload":p,"confirm":true})
        )
        .is_err()
    );
    let mut p = payload();
    p["text"] = json!("x".repeat(262144));
    assert!(
        validate_input(
            Operation::Revise,
            &json!({"draftId":"one","expectedEventHash":"a".repeat(64),"payload":p,"confirm":true})
        )
        .is_err()
    );
}
