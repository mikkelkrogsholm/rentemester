use super::*;
use rusqlite::Connection;
fn fixture() -> Connection {
    let mut c = Connection::open_in_memory().unwrap();
    let mut l = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic Alias Maintenance").unwrap();
    l
}
fn customer(name: &str, cvr: &str) -> crate::sales::Customer {
    serde_json::from_value(json!({"name":name,"vatOrCvr":cvr,"address":"Synthetic address","email":"synthetic@example.invalid"})).unwrap()
}
fn erase_subject(tx: &Transaction<'_>, cvr: &str) {
    erase(
        tx,
        &Input {
            cvr: Some(cvr.into()),
            name: None,
            as_of: None,
            confirm: true,
        },
        "2026-10-19",
        "synthetic",
    )
    .unwrap();
}
#[test]
fn unrelated_contacts_and_distinct_cvr_names_work_without_resurrecting_erased_identity_or_audit() {
    let mut l = fixture();
    let tx = l
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let original = crate::sales::create_customer(
        &tx,
        &customer("Synthetic Shared", "SYN-ERASED"),
        "synthetic",
    )
    .unwrap()["id"]
        .as_i64()
        .unwrap();
    erase_subject(&tx, "SYN-ERASED");
    let cutoff: i64 = tx
        .query_row("SELECT MAX(id) FROM audit_log", [], |r| r.get(0))
        .unwrap();
    assert!(
        crate::sales::create_customer(
            &tx,
            &customer("Synthetic Reintroduced", "SYN-ERASED"),
            "synthetic"
        )
        .is_err()
    );
    assert!(
        crate::sales::update_customer(
            &tx,
            original,
            &json!({"name":"Synthetic Changed","vatOrCvr":"SYN-CHANGED"}),
            "synthetic"
        )
        .is_err()
    );
    let unrelated = crate::sales::create_customer(
        &tx,
        &customer("Synthetic Unrelated", "SYN-OTHER"),
        "synthetic",
    )
    .unwrap()["id"]
        .as_i64()
        .unwrap();
    crate::sales::update_customer(
        &tx,
        unrelated,
        &json!({"name":"Synthetic Revised"}),
        "synthetic",
    )
    .unwrap();
    assert!(
        crate::sales::update_customer(
            &tx,
            unrelated,
            &json!({"vatOrCvr":"SYN-ERASED"}),
            "synthetic"
        )
        .is_err()
    );
    let same_name = crate::sales::create_customer(
        &tx,
        &customer("Synthetic Shared", "SYN-DIFFERENT"),
        "synthetic",
    )
    .unwrap()["id"]
        .as_i64()
        .unwrap();
    assert_ne!(same_name, original);
    let contacts = crate::sales::contacts(&tx, "synthetic", "2026-10-19").unwrap();
    let rows = contacts["customers"].as_array().unwrap();
    let erased = rows.iter().find(|v| v["id"] == original).unwrap();
    assert_eq!(erased["name"], "[redigeret — GDPR]");
    assert!(erased["vatOrCvr"].is_null());
    assert!(erased["email"].is_null());
    assert_eq!(
        rows.iter().find(|v| v["id"] == same_name).unwrap()["name"],
        "Synthetic Shared"
    );
    for row in super::rows(
        &tx,
        "SELECT message,entity_id FROM audit_log WHERE id>?1",
        [cutoff],
    )
    .unwrap()
    {
        assert!(!row["message"].as_str().unwrap().contains("Synthetic"));
        assert!(row["entity_id"].as_str().unwrap().starts_with("sha256:"));
    }
    assert_eq!(
        tx.query_row("SELECT name FROM customers WHERE id=?1", [original], |r| {
            r.get::<_, String>(0)
        })
        .unwrap(),
        "Synthetic Shared"
    );
    assert_eq!(
        tx.query_row("SELECT COUNT(*) FROM journal_entries", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
#[test]
fn legacy_aliases_still_fence_identity_enrichment_and_unknown_name_ownership() {
    let mut l = fixture();
    let tx = l
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("INSERT INTO customers(id,name)VALUES(1,'Synthetic Legacy');INSERT INTO gdpr_erasures(subject_key,source,source_row_id,redacted_fields,rule_id,reason)VALUES('Synthetic Legacy','customers',1,'[\"name\"]','synthetic','legacy');").unwrap();
    assert!(
        crate::sales::create_customer(
            &tx,
            &customer("Synthetic Legacy", "SYN-NEW-CVR"),
            "synthetic"
        )
        .is_err()
    );
    assert!(
        crate::sales::update_customer(
            &tx,
            1,
            &json!({"name":"Synthetic Renamed","vatOrCvr":"SYN-NEW-CVR"}),
            "synthetic"
        )
        .is_err()
    );
    assert!(
        crate::sales::create_customer(
            &tx,
            &customer("Synthetic Distinct", "SYN-DISTINCT"),
            "synthetic"
        )
        .is_ok()
    );
    tx.execute_batch("INSERT INTO gdpr_erasures(subject_key,source,source_row_id,redacted_fields,rule_id,reason)VALUES('SYN-RAW-CVR','vendors',999,'[\"name\"]','synthetic','legacy');").unwrap();
    assert!(
        crate::sales::create_customer(
            &tx,
            &customer("Synthetic New Name", "SYN-RAW-CVR"),
            "synthetic"
        )
        .is_err()
    );
}
#[test]
fn vendor_overlay_and_retirement_preserve_tombstones_foreign_keys_and_pseudonymous_audit() {
    let mut l = fixture();
    let tx = l
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let v = |name: &str, cvr: &str| {
        serde_json::from_value::<crate::vendors::Vendor>(json!({"name":name,"vatOrCvr":cvr}))
            .unwrap()
    };
    let id = crate::vendors::create(&tx, &v("Synthetic Supplier", "DK12345678"), "synthetic")
        .unwrap()["id"]
        .as_i64()
        .unwrap();
    erase_subject(&tx, "DK12345678");
    assert!(
        crate::vendors::update(&tx, id, &json!({"name":"Synthetic Escaped"}), "synthetic").is_err()
    );
    let other = crate::vendors::create(
        &tx,
        &v("Synthetic Other Supplier", "DK87654321"),
        "synthetic",
    )
    .unwrap()["id"]
        .as_i64()
        .unwrap();
    crate::vendors::update(&tx, other, &json!({"phone":"synthetic"}), "synthetic").unwrap();
    let read = crate::sales::contacts(&tx, "synthetic", "2026-10-19").unwrap();
    assert_eq!(
        read["vendors"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["id"] == id)
            .unwrap()["name"],
        "[redigeret — GDPR]"
    );
    tx.execute_batch(
        "INSERT INTO documents(id,source,sha256_hash)VALUES(1,'synthetic','synthetic-hash');",
    )
    .unwrap();
    tx.execute("INSERT INTO document_vendor_identity_links(company_id,document_id,vendor_id,supplier_identity,provenance,rationale,created_by,created_at)VALUES(1,1,?1,'synthetic','synthetic','synthetic','user:synthetic','2026-10-19')",[id]).unwrap();
    assert!(crate::contact_delete::delete(&tx, id, true, true, "synthetic").is_err());
    assert_eq!(
        tx.query_row("SELECT archived FROM vendors WHERE id=?1", [id], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    let clean = crate::sales::create_customer(
        &tx,
        &customer("Synthetic Retired", "SYN-RETIRED"),
        "synthetic",
    )
    .unwrap()["id"]
        .as_i64()
        .unwrap();
    erase_subject(&tx, "SYN-RETIRED");
    let tombstones: i64 = tx
        .query_row("SELECT COUNT(*) FROM gdpr_erasures", [], |r| r.get(0))
        .unwrap();
    crate::contact_delete::delete(&tx, clean, false, true, "synthetic").unwrap();
    let next = crate::sales::create_customer(
        &tx,
        &customer("Synthetic After Retire", "SYN-AFTER"),
        "synthetic",
    )
    .unwrap()["id"]
        .as_i64()
        .unwrap();
    assert!(next > clean);
    assert_eq!(
        tx.query_row("SELECT COUNT(*) FROM gdpr_erasures", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        tombstones
    );
    assert!(
        crate::sales::create_customer(
            &tx,
            &customer("Synthetic New Label", "SYN-RETIRED"),
            "synthetic"
        )
        .is_err()
    );
    let message: String = tx
        .query_row(
            "SELECT message FROM audit_log WHERE event_type='customer_delete'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!message.contains("Synthetic Retired"));
    assert!(message.contains("sha256:"));
}

#[test]
fn legacy_structured_cvr_survives_rename_and_document_counterparty_is_not_captured() {
    let mut l = fixture();
    let tx = l
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("INSERT INTO customers(id,name,vat_or_cvr)VALUES(1,'Synthetic Old Alias','SYN-OLD');INSERT INTO gdpr_erasures(subject_key,source,source_row_id,redacted_fields,rule_id,reason)VALUES('Synthetic Old Alias','customers',1,'[\"name\"]','synthetic','legacy');INSERT INTO documents(id,source,sha256_hash,sender_name,sender_vat_cvr,recipient_name,recipient_vat_cvr)VALUES(1,'synthetic','synthetic-doc','Synthetic Sender','SYN-SENDER','Synthetic Recipient','SYN-RECIPIENT');INSERT INTO gdpr_erasures(subject_key,source,source_row_id,redacted_fields,rule_id,reason)VALUES('Synthetic Sender','documents',1,'[\"name\"]','synthetic','legacy');").unwrap();
    assert!(
        crate::sales::create_customer(
            &tx,
            &customer("Synthetic New Alias", "SYN-OLD"),
            "synthetic"
        )
        .is_err()
    );
    assert!(
        crate::sales::create_customer(
            &tx,
            &customer("Synthetic New Sender", "SYN-SENDER"),
            "synthetic"
        )
        .is_err()
    );
    assert!(
        crate::sales::create_customer(
            &tx,
            &customer("Synthetic Recipient", "SYN-RECIPIENT"),
            "synthetic"
        )
        .is_ok()
    );
}
