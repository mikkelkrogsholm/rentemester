use super::*;
use rusqlite::Connection;

fn fixture() -> Connection {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE audit_log(id INTEGER PRIMARY KEY,event_type TEXT NOT NULL,entity_type TEXT NOT NULL,entity_id TEXT,message TEXT NOT NULL,actor TEXT NOT NULL,created_at TEXT NOT NULL);
        INSERT INTO audit_log VALUES(1,'Journal_Post','journal','j1','<b>exact</b>','User:ALPHA','2026-01-01T00:00:00.000Z');
        INSERT INTO audit_log VALUES(2,'journal_reverse','journal',NULL,'untouched text','user:alpha','2026-01-02T12:00:00.000Z');
        INSERT INTO audit_log VALUES(3,'journal_post','journal','j3','third','agent:beta','2026-01-02T12:00:00.000Z');
        INSERT INTO audit_log VALUES(4,'document_ingest','document','d4','fourth','user:alpha','2026-01-03 00:00:00');").unwrap();
    db
}
fn query(value: Value) -> Query {
    serde_json::from_value(value).unwrap()
}
#[test]
fn stable_sql_pages_count_all_matches_and_preserve_exact_message_and_null_entity() {
    let mut db = fixture();
    let before = db.total_changes();
    let tx = db.transaction().unwrap();
    let first = read(&tx, "synthetic", &query(json!({"limit":2}))).unwrap();
    assert_eq!(
        first,
        json!({"ok":true,"slug":"synthetic","rows":[
        {"id":4,"eventType":"document_ingest","entityType":"document","entityId":"d4","message":"fourth","actor":"user:alpha","createdAt":"2026-01-03 00:00:00"},
        {"id":3,"eventType":"journal_post","entityType":"journal","entityId":"j3","message":"third","actor":"agent:beta","createdAt":"2026-01-02T12:00:00.000Z"}
    ],"total":4,"count":2,"limit":2,"offset":0,"hasMore":true,"nextOffset":2})
    );
    let second = read(&tx, "synthetic", &query(json!({"limit":2,"offset":2}))).unwrap();
    assert_eq!(second["rows"][0]["id"], 2);
    assert_eq!(second["rows"][0]["entityId"], Value::Null);
    assert_eq!(second["rows"][1]["message"], "<b>exact</b>");
    assert_eq!(second["hasMore"], false);
    assert!(second.get("nextOffset").is_none());
    let empty = read(
        &tx,
        "synthetic",
        &query(json!({"offset":9007199254740991_u64})),
    )
    .unwrap();
    assert_eq!(empty["total"], 4);
    assert_eq!(empty["count"], 0);
    assert_eq!(empty["hasMore"], false);
    assert_eq!(tx.total_changes(), before);
    tx.rollback().unwrap();
}
#[test]
fn filters_and_case_insensitive_like_wildcards_dates_utc_and_inclusive_days() {
    let mut db = fixture();
    let tx = db.transaction().unwrap();
    let matched=read(&tx,"a",&query(json!({"eventTypeLike":"JOURNAL_%","actorLike":"ALPHA","fromDate":"2026-01-01","toDate":"2026-01-02"}))).unwrap();
    assert_eq!(matched["total"], 2);
    assert_eq!(matched["rows"][0]["id"], 2);
    assert_eq!(matched["rows"][1]["id"], 1);
    let exact = read(
        &tx,
        "a",
        &query(
            json!({"fromDate":"2026-01-02T13:00:00+01:00","toDate":"2026-01-02T07:00:00-05:00"}),
        ),
    )
    .unwrap();
    assert_eq!(exact["total"], 2);
    let sql_timestamp = read(
        &tx,
        "a",
        &query(json!({"fromDate":"2026-01-03","toDate":"2026-01-03"})),
    )
    .unwrap();
    assert_eq!(sql_timestamp["total"], 1);
    let empty = read(
        &tx,
        "a",
        &query(json!({"fromDate":"","toDate":"","eventTypeLike":"","actorLike":""})),
    )
    .unwrap();
    assert_eq!(empty["total"], 4);
    assert_eq!(empty["limit"], 500);
}
#[test]
fn invalid_filters_fail_before_sql_and_null_unknown_duplicate_types_are_rejected() {
    let mut db = Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    for value in [
        json!({"limit":0}),
        json!({"limit":5001}),
        json!({"offset":9007199254740992_u64}),
        json!({"fromDate":"2026-02-30"}),
        json!({"fromDate":"0000-01-01"}),
        json!({"toDate":"2026-01-01T00:00:00"}),
        json!({"fromDate":"2026-01-02","toDate":"2026-01-01"}),
        json!({"actorLike":"bad\nactor"}),
        json!({"eventTypeLike":"x".repeat(257)}),
        json!({"fromDate":"éé-01-01"}),
    ] {
        let q = query(value);
        assert_eq!(
            read(&tx, "a", &q).unwrap_err().code,
            "invalid_audit_log_query"
        );
    }
    for raw in [
        r#"{"limit":null}"#,
        r#"{"actorLike":null}"#,
        r#"{"limit":1.5}"#,
        r#"{"offset":-1}"#,
        r#"{"limit":"1"}"#,
        r#"{"unexpected":1}"#,
        r#"{"limit":1,"limit":2}"#,
    ] {
        assert!(serde_json::from_str::<Query>(raw).is_err(), "{raw}");
    }
    let default = Query::default();
    assert_eq!(serde_json::to_value(&default).unwrap(), json!({}));
    assert!(
        serde_json::from_value::<Query>(serde_json::to_value(default).unwrap())
            .unwrap()
            .validate()
            .is_ok()
    );
}

#[test]
fn oversized_page_is_refused_without_truncation_and_a_smaller_page_keeps_exact_text() {
    let mut db = fixture();
    let message = "\"".repeat(3 * 1024 * 1024);
    db.execute(
        "UPDATE audit_log SET message=?1 WHERE id IN (3,4)",
        [&message],
    )
    .unwrap();
    let before = db.total_changes();
    let tx = db.transaction().unwrap();
    assert_eq!(
        read(&tx, "a", &query(json!({"limit":2}))).unwrap_err().code,
        "audit_log_page_too_large"
    );
    let one = read(&tx, "a", &query(json!({"limit":1}))).unwrap();
    assert_eq!(one["rows"][0]["message"].as_str().unwrap(), message);
    assert_eq!(one["count"], 1);
    assert_eq!(one["total"], 4);
    assert_eq!(one["hasMore"], true);
    assert_eq!(one["nextOffset"], 1);
    assert_eq!(tx.total_changes(), before);
}
