use super::*;
use rentemester_engine::document::Metadata;
use rusqlite::{Connection, TransactionBehavior};
const NOW: &str = "2026-10-18T12:00:00.000Z";
fn register(tx: &Transaction<'_>, eu: bool) {
    let sender = if eu {
        json!({"name":"Synthetic supplier","address":"Synthetic 1","vatOrCvr":"DE123456789","countryCode":"DE","identifierKind":"eu_vat"})
    } else {
        json!({"name":"Synthetic supplier","address":"Synthetic 1","vatOrCvr":"DK12345678"})
    };
    let meta:Metadata=serde_json::from_value(json!({"source":"synthetic","documentType":"purchase_sale","issueDate":"2026-10-18","deliveryDescription":"Synthetic supplies","invoiceNo":"SYN-1","amountIncVat":100,"vatAmount":0,"sender":sender,"recipient":{"name":"Synthetic buyer","address":"Synthetic 2","vatOrCvr":"DK87654321"}})).unwrap();
    crate::documents::register(
        tx,
        crate::documents::Original {
            filename: "synthetic.txt",
            hash: &"a".repeat(64),
            path: "synthetic-original",
            mime: "text/plain",
        },
        &meta,
        "synthetic",
        NOW,
    )
    .unwrap();
}
fn observation(status: &str) -> ProviderResult {
    ProviderResult {
        status: status.into(),
        name: Some("Synthetic GmbH".into()),
        address: None,
        raw_response: Some("PRIVATE_PROVIDER_BODY".into()),
    }
}
#[test]
fn readonly_then_negative_recovery_cache_and_safe_source_snapshot() {
    let mut control = Connection::open_in_memory().unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic preflight").unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    register(&tx, true);
    let before = tx.total_changes();
    let dry = inspect(&tx, 1, NOW).unwrap();
    assert_eq!(before, tx.total_changes());
    assert_eq!(dry["classification"], "EU");
    assert_eq!(dry["wouldCallProvider"], true);
    assert_eq!(dry["ok"], false);
    let p = prepare_apply(&tx, 1, "user:synthetic", NOW).unwrap();
    assert_eq!(p.provider_vat(), Some("DE123456789"));
    let negative = finish_apply(
        &tx,
        p,
        Some(observation("unavailable")),
        "user:synthetic",
        NOW,
    )
    .unwrap();
    assert_eq!(negative["ok"], false);
    assert_eq!(negative["exception"]["status"], "open");
    assert_eq!(negative["evidence"].as_array().unwrap().len(), 3);
    assert!(!negative.to_string().contains("PRIVATE_PROVIDER_BODY"));
    assert!(!negative.to_string().contains("detail_json"));
    let p = prepare_apply(&tx, 1, "user:synthetic", NOW).unwrap();
    let recovered =
        finish_apply(&tx, p, Some(observation("valid")), "user:synthetic", NOW).unwrap();
    assert_eq!(recovered["ok"], true);
    assert_eq!(recovered["cached"], true);
    // Source does not invent automatic exception resolution.
    assert_eq!(recovered["exception"]["id"], negative["exception"]["id"]);
    assert_eq!(recovered["exception"]["status"], "open");
    let before = tx.total_changes();
    let p = prepare_apply(&tx, 1, "user:synthetic", NOW).unwrap();
    assert_eq!(p.provider_vat(), None);
    assert_eq!(
        finish_apply(&tx, p, None, "user:synthetic", NOW).unwrap(),
        recovered
    );
    assert_eq!(before, tx.total_changes());
    assert_eq!(
        inspect(&tx, 1, "2027-02-01T12:00:00.000Z").unwrap()["cached"],
        false
    );
    // A second document with the same supplier must not borrow local success.
    tx.execute("INSERT INTO documents(source,sha256_hash,sender_vat_cvr,supplier_country_code,supplier_identifier_kind,supplier_identity_status,stored_path) VALUES('synthetic',?1,'DE123456789','DE','eu_vat','resolved','synthetic-second')",["b".repeat(64)]).unwrap();
    assert_eq!(inspect(&tx, 2, NOW).unwrap()["cached"], false);
}
#[test]
fn local_completion_has_no_missing_observation_and_no_event_but_conflict_is_durable() {
    let mut control = Connection::open_in_memory().unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic local").unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    register(&tx, false);
    let p = prepare_apply(&tx, 1, "user:synthetic", NOW).unwrap();
    assert_eq!(p.provider_vat(), None);
    assert_eq!(
        finish_apply(&tx, p, None, "user:synthetic", NOW).unwrap()["ok"],
        true
    );
    assert_eq!(
        tx.query_row::<i64, _, _>("SELECT count(*) FROM vat_validation_events", [], |r| r
            .get(0))
            .unwrap(),
        0
    );
    tx.execute(
        "UPDATE documents SET supplier_identity_status='human_resolution_required' WHERE id=1",
        [],
    )
    .unwrap();
    let p = prepare_apply(&tx, 1, "user:synthetic", NOW).unwrap();
    assert_eq!(p.provider_vat(), None);
    let result = finish_apply(&tx, p, None, "user:synthetic", NOW).unwrap();
    assert_eq!(result["classification"], "CONFLICT");
    assert_eq!(result["ok"], false);
    assert_eq!(result["evidence"][0]["event_type"], "preflight_failed");
    assert_eq!(result["exception"]["status"], "open");
}
#[test]
fn provider_admission_refuses_fact_change_and_observation_branch_mismatch() {
    let mut control = Connection::open_in_memory().unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic fences").unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    register(&tx, true);
    let p = prepare_apply(&tx, 1, "user:synthetic", NOW).unwrap();
    assert!(finish_apply(&tx, p, None, "user:synthetic", NOW).is_err());
    let p = prepare_apply(&tx, 1, "user:synthetic", NOW).unwrap();
    tx.execute(
        "UPDATE documents SET sender_vat_cvr='DE987654321' WHERE id=1",
        [],
    )
    .unwrap();
    assert!(finish_apply(&tx, p, Some(observation("valid")), "user:synthetic", NOW).is_err());
    assert_eq!(
        tx.query_row::<i64, _, _>(
            "SELECT count(*) FROM vat_validation_events WHERE event_type='provider_result'",
            [],
            |r| r.get(0)
        )
        .unwrap(),
        0
    );
}
