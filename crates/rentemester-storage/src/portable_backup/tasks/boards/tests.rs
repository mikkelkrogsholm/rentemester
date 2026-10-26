use super::*;
use crate::tasks::{self as writes, Command, Context};
use serde_json::json;
fn db() -> Connection {
    let mut c = Connection::open_in_memory().unwrap();
    crate::control_migration::initialize_empty(&mut c).unwrap();
    c
}
fn context(key: &str, version: Option<u64>) -> Context<'_> {
    Context {
        actor: "user:synthetic",
        principal: "synthetic-member",
        idempotency_key: key,
        expected_version: version,
        now: "2026-10-26T11:00:00.000Z",
        source_check: None,
    }
}
#[test]
fn codec3_board_recovery_preserves_exact_receipts_and_atomically_refuses_tampering() {
    let mut source = db();
    let task = json!({"taskId":"synthetic-task","title":"Synthetic board task","scope":{"kind":"company","companySlug":"synthetic"}});
    writes::mutate(
        &mut source,
        Command::Create {
            input: &task,
            generated_id: "unused",
        },
        context("create", None),
        |_, _| Ok(()),
    )
    .unwrap();
    let b = engine::default_board(&domain::Scope::Company {
        company_slug: "synthetic".into(),
    });
    let mut input = json!({"boardId":b.board_id,"scope":b.scope,"columns":b.columns,"relocations":{"open":"new-open"}});
    input["columns"][0]["columnId"] = json!("new-open");
    let tx = source
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let saved = writes::boards::save_in_tx(
        &tx,
        &input,
        &context("board", Some(0)),
        |_, _| Ok(()),
        |_| Ok(()),
    )
    .unwrap();
    tx.commit().unwrap();
    let bytes = export(&source).unwrap();
    let original: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(original["format"], FORMAT3);
    let mut target = db();
    initialize(&mut target, &bytes, &["synthetic".into()]).unwrap();
    assert_eq!(export(&target).unwrap(), bytes);
    let tx = target
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    assert_eq!(
        json!(
            writes::boards::save_in_tx(
                &tx,
                &input,
                &context("board", Some(0)),
                |_, _| Ok(()),
                |_| Ok(())
            )
            .unwrap()
        ),
        json!(saved)
    );
    tx.rollback().unwrap();
    for change in ["principal", "result", "codec2"] {
        let mut value = original.clone();
        if change == "codec2" {
            value["format"] = json!(FORMAT2);
        } else {
            let table = &mut value["tables"]["rm_task_receipts"];
            let cols = table["columns"].as_array().unwrap();
            let op = cols.iter().position(|v| v == "operation").unwrap();
            let col = cols
                .iter()
                .position(|v| {
                    v == if change == "principal" {
                        "principal"
                    } else {
                        "result_json"
                    }
                })
                .unwrap();
            let row = table["rows"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|r| r[op]["value"] == "task-board-save")
                .unwrap();
            row[col]["value"] = json!(if change == "principal" {
                "wrong-member"
            } else {
                "{}"
            });
        }
        let mut empty = db();
        assert!(
            initialize(
                &mut empty,
                &serde_json::to_vec(&value).unwrap(),
                &["synthetic".into()]
            )
            .is_err(),
            "{change}"
        );
        assert_eq!(
            empty
                .query_row::<i64, _, _>("SELECT count(*) FROM rm_task_events", [], |r| r.get(0))
                .unwrap(),
            0
        );
    }
}
