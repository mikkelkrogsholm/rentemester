use super::*;
use crate::tasks::{self as writes, Context};
use serde_json::json;
fn db() -> Connection {
    let mut db = Connection::open_in_memory().unwrap();
    crate::control_migration::initialize_empty(&mut db).unwrap();
    db
}
fn ctx(key: &str, expected: Option<u64>) -> Context<'_> {
    Context {
        actor: "user:synthetic",
        principal: "synthetic-member",
        idempotency_key: key,
        expected_version: expected,
        now: "2026-01-01T00:00:00.000Z",
        source_check: None,
    }
}
fn populated() -> Connection {
    let mut db = db();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let input = json!({"seriesId":"monthly","title":"Synthetic routine","scope":{"kind":"company","companySlug":"synthetic"},"template":{"title":"Synthetic routine","scope":{"kind":"company","companySlug":"synthetic"}},"cadence":"month","every":1,"anchor":"calendar","startDate":"2026-01-01","endDate":null,"fiscalYearStartMonth":1,"workDayOffset":0,"deadlineDayOffset":null,"relevance":"activity","active":true});
    writes::series::save_in_tx(&tx, &input, &ctx("save", Some(0)), |_| Ok(()), |_| Ok(())).unwrap();
    let payload = json!({"companySlugs":["synthetic"],"asOfDate":"2026-02-01"});
    writes::series::materialize_in_tx(
        &tx,
        &payload,
        &["synthetic".into()],
        false,
        "2026-02-01",
        &ctx("bulk", None),
        |_| Ok(()),
        |_| Ok(()),
        |_| Ok(()),
    )
    .unwrap();
    writes::series::materialize_in_tx(
        &tx,
        &payload,
        &["synthetic".into()],
        false,
        "2026-02-01",
        &ctx("duplicate", None),
        |_| Ok(()),
        |_| Ok(()),
        |_| Ok(()),
    )
    .unwrap();
    tx.commit().unwrap();
    db
}
#[test]
fn codec2_routine_roundtrip_is_source_free_preserves_receipts_and_resets_runtime() {
    let source = populated();
    source
        .execute(
            "UPDATE rm_task_runtime SET lease_owner='old',last_error='old'",
            [],
        )
        .unwrap();
    let bytes = export(&source).unwrap();
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["format"], FORMAT2);
    let mut target = db();
    initialize(&mut target, &bytes, &["synthetic".into()]).unwrap();
    assert_eq!(export(&target).unwrap(), bytes);
    assert_eq!(
        target
            .query_row::<i64, _, _>("SELECT count(*) FROM rm_task_events", [], |r| r.get(0))
            .unwrap(),
        2
    );
    assert_eq!(
        target
            .query_row::<i64, _, _>("SELECT count(*) FROM rm_task_receipts", [], |r| r.get(0))
            .unwrap(),
        5
    );
    assert_eq!(
        target
            .query_row::<i64, _, _>("SELECT count(*) FROM user", [], |r| r.get(0))
            .unwrap(),
        0
    );
    assert_eq!(
        target
            .query_row::<Option<String>, _, _>("SELECT lease_owner FROM rm_task_runtime", [], |r| r
                .get(0))
            .unwrap(),
        None
    );
    let tx = target
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let payload = json!({"companySlugs":["synthetic"],"asOfDate":"2026-02-01"});
    let replay = writes::series::materialize_in_tx(
        &tx,
        &payload,
        &["synthetic".into()],
        false,
        "2026-02-01",
        &ctx("bulk", None),
        |_| Ok(()),
        |_| Ok(()),
        |_| Ok(()),
    )
    .unwrap();
    assert_eq!(replay.materialized.created, 2);
    tx.rollback().unwrap();
}
#[test]
fn manual_only_codec1_bytes_and_digest_stay_identical() {
    let source = super::tests::populated();
    let bytes = export(&source).unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&bytes).unwrap()["format"],
        FORMAT
    );
    let digest = rentemester_engine::sha256_bytes(&bytes);
    let mut target = db();
    initialize(&mut target, &bytes, &["synthetic".into()]).unwrap();
    assert_eq!(export(&target).unwrap(), bytes);
    assert_eq!(
        rentemester_engine::sha256_bytes(&export(&target).unwrap()),
        digest
    );
}
fn change_receipt(original: &Value, operation: &str, column: &str, value: &str) -> Value {
    let mut changed = original.clone();
    let table = &mut changed["tables"]["rm_task_receipts"];
    let cols = table["columns"].as_array().unwrap();
    let op = cols.iter().position(|v| v == "operation").unwrap();
    let col = cols.iter().position(|v| v == column).unwrap();
    let row = table["rows"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|r| r[op]["value"] == operation)
        .unwrap();
    row[col]["value"] = json!(value);
    changed
}
#[test]
fn codec2_refuses_wrong_principal_result_counts_scope_identity_and_codec1_downgrade_atomically() {
    let original: Value = serde_json::from_slice(&export(&populated()).unwrap()).unwrap();
    let mut cases = vec![];
    cases.push(change_receipt(
        &original,
        "series-save",
        "principal",
        "other-member",
    ));
    cases.push(change_receipt(
        &original,
        "series-save",
        "result_json",
        "{}",
    ));
    cases.push(change_receipt(
        &original,
        "series-materialize",
        "operation",
        "unknown-future",
    ));
    for patch in [
        json!({"created":99,"existing":0,"taskIds":[]}),
        json!({"created":1,"existing":0,"taskIds":["missing-task"]}),
    ] {
        let mut result = json!({"scope":{"kind":"workspace","companySlugs":["synthetic"]},"includesWorkspace":false,"materialized":patch});
        cases.push(change_receipt(
            &original,
            "series-materialize",
            "result_json",
            &result.to_string(),
        ));
        result["scope"]["companySlugs"] = json!([]);
        cases.push(change_receipt(
            &original,
            "series-materialize",
            "result_json",
            &result.to_string(),
        ));
    }
    let mut downgrade = original.clone();
    downgrade["format"] = json!(FORMAT);
    cases.push(downgrade);
    for value in cases {
        let mut target = db();
        assert!(
            initialize(
                &mut target,
                &serde_json::to_vec(&value).unwrap(),
                &["synthetic".into()]
            )
            .is_err()
        );
        assert_eq!(
            target
                .query_row::<i64, _, _>("SELECT count(*) FROM rm_task_series_events", [], |r| r
                    .get(0))
                .unwrap(),
            0
        );
        assert_eq!(
            target
                .query_row::<i64, _, _>("SELECT count(*) FROM rm_task_events", [], |r| r.get(0))
                .unwrap(),
            0
        );
    }
}
