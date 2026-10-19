use super::*;
fn db() -> rusqlite::Connection {
    let mut c = rusqlite::Connection::open_in_memory().unwrap();
    let mut l = rusqlite::Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic corporate").unwrap();
    c
}
fn input(id: &str, kind: &str) -> Ingest {
    serde_json::from_value(json!({"recordId":id,"type":kind,"bytesBase64":"unused-in-storage","filename":"synthetic.txt","source":"synthetic-governance","receivedAt":"2026-01-01","uploader":"synthetic-uploader"})).unwrap()
}
fn set(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|v| v.to_string()).collect()
}
#[test]
fn exact_original_append_history_replay_duplicate_and_reasoned_replacement() {
    let mut c = db();
    let tx = c.transaction().unwrap();
    let i = input("record-synthetic", "annual_report");
    let links = [Link {
        r#type: "company".into(),
        id: "a".into(),
    }];
    let r = ingest(
        &tx,
        &i,
        b"synthetic immutable bytes",
        "unused",
        &links,
        "user:u",
    )
    .unwrap();
    assert_eq!(r["type"], "annual_report");
    assert_eq!(r["sha256"], hash(b"synthetic immutable bytes"));
    assert_eq!(
        download(&tx, "record-synthetic").unwrap(),
        b"synthetic immutable bytes"
    );
    assert_eq!(
        ingest(
            &tx,
            &i,
            b"synthetic immutable bytes",
            "unused",
            &links,
            "user:u"
        )
        .unwrap(),
        r
    );
    assert!(ingest(&tx, &i, b"changed", "unused", &links, "user:u").is_err());
    assert!(
        ingest(
            &tx,
            &input("other-record", "other"),
            b"synthetic immutable bytes",
            "unused",
            &links,
            "user:u"
        )
        .is_err()
    );
    enrich(
        &tx,
        "record-synthetic",
        &Enrich {
            record_id: None,
            assertion: "Reviewed synthetic provenance".into(),
        },
        "user:u",
        "2026-01-02T00:00:00.000Z",
    )
    .unwrap();
    ingest(
        &tx,
        &input("replacement", "articles"),
        b"replacement bytes",
        "unused",
        &links,
        "user:u",
    )
    .unwrap();
    supersede(
        &tx,
        "record-synthetic",
        &Supersede {
            record_id: None,
            replacement_record_id: "replacement".into(),
            reason: "Corrected source".into(),
        },
        "user:u",
        "2026-01-03T00:00:00.000Z",
    )
    .unwrap();
    assert_eq!(
        inspect(&tx, "record-synthetic").unwrap()["history"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    assert_eq!(
        download(&tx, "record-synthetic").unwrap(),
        b"synthetic immutable bytes"
    );
    assert!(
        tx.execute(
            "UPDATE rm_corporate_record_bytes SET filename='rewrite'",
            []
        )
        .is_err()
    );
    assert!(
        tx.execute("DELETE FROM rm_corporate_record_events", [])
            .is_err()
    );
}
#[test]
fn scope_filter_precedes_pagination_and_sensitivity_detail_gate() {
    let mut c = db();
    let tx = c.transaction().unwrap();
    let mut i = input("record-restricted", "board_resolution");
    i.sensitivity = Some("restricted".into());
    ingest(
        &tx,
        &i,
        b"restricted",
        "unused",
        &[Link {
            r#type: "company".into(),
            id: "a".into(),
        }],
        "user:u",
    )
    .unwrap();
    assert_eq!(list(&tx, &set(&["a"]), 0, 25).unwrap()["count"], 1);
    assert!(!detail_visible(&tx, "record-restricted", &set(&["a"]), &set(&[])).unwrap());
    assert!(detail_visible(&tx, "record-restricted", &set(&["a"]), &set(&["a"])).unwrap());
    link(
        &tx,
        "record-restricted",
        &Link {
            r#type: "company".into(),
            id: "b".into(),
        },
        "user:u",
        "2026-01-02T00:00:00.000Z",
    )
    .unwrap();
    assert_eq!(
        list(&tx, &set(&["a"]), 0, 1).unwrap(),
        json!({"rows":[],"count":0,"nextCursor":null})
    );
    assert!(!visible(&tx, "record-restricted", &set(&["a"])).unwrap());
    assert!(
        detail_visible(
            &tx,
            "record-restricted",
            &set(&["a", "b"]),
            &set(&["a", "b"])
        )
        .unwrap()
    );
}
#[test]
fn corrupt_original_and_invalid_link_input_refuse_without_rewriting() {
    let mut c = db();
    let tx = c.transaction().unwrap();
    let i = input("record-source", "other");
    assert!(ingest(&tx, &i, b"", "unused", &[], "user:u").is_err());
    assert!(
        ingest(
            &tx,
            &i,
            b"valid",
            "unused",
            &[Link {
                r#type: "party".into(),
                id: "missing".into()
            }],
            "user:u"
        )
        .is_err()
    );
    assert!(!exists(&tx, "record-source").unwrap());
    ingest(
        &tx,
        &i,
        b"valid",
        "unused",
        &[Link {
            r#type: "company".into(),
            id: "a".into(),
        }],
        "user:u",
    )
    .unwrap();
    tx.execute_batch("DROP TRIGGER rm_corporate_record_bytes_no_update;UPDATE rm_corporate_record_bytes SET original_bytes=x'00';").unwrap();
    assert!(download(&tx, "record-source").is_err());
    assert!(
        link(
            &tx,
            "record-source",
            &Link {
                r#type: "invented".into(),
                id: "a".into()
            },
            "user:u",
            "2026-01-02"
        )
        .is_err()
    );
}
