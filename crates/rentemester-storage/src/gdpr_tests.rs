use super::*;
use rusqlite::Connection;
fn fixture() -> Connection {
    let mut c = Connection::open_in_memory().unwrap();
    let mut l = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic GDPR").unwrap();
    l
}
fn input(cvr: Option<&str>, name: Option<&str>) -> Input {
    Input {
        cvr: cvr.map(str::to_owned),
        name: name.map(str::to_owned),
        as_of: None,
        confirm: true,
    }
}
#[test]
fn audited_overlay_is_subject_scoped_append_only_and_does_not_delete_originals() {
    let mut l = fixture();
    l.execute_batch("INSERT INTO customers(id,name,address,email,vat_or_cvr)VALUES(1,'Synthetic Subject','Synthetic Street','subject@example.invalid','SYN-ONE'),(2,'Synthetic Other','Other Street','other@example.invalid','SYN-TWO');INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor)VALUES('synthetic_contact','customer','SYN-ONE','Synthetic Subject provided contact information','user:synthetic');").unwrap();
    let tx = l.transaction().unwrap();
    let i = input(Some("SYN-ONE"), None);
    let before = export(&tx, "synthetic", &i, "2026-10-19", "reviewer").unwrap();
    assert!(
        before["export"]["records"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["source"] == "customers" && r["erasable"] == true)
    );
    let result = erase(&tx, &i, "2026-10-19", "owner").unwrap();
    assert_eq!(result["erasedCount"], 2);
    assert_eq!(result["refusedCount"], 0);
    let after = export(&tx, "synthetic", &i, "2026-10-19", "reviewer").unwrap();
    let customer = after["export"]["records"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["source"] == "customers")
        .unwrap();
    assert_eq!(customer["personalData"]["name"], "[redigeret — GDPR]");
    assert!(customer["personalData"]["email"].is_null());
    assert!(after["export"]["coverage"]["complete"] == false);
    assert_eq!(
        tx.query_row("SELECT name FROM customers WHERE id=1", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "Synthetic Subject"
    );
    assert_eq!(
        tx.query_row("SELECT email FROM customers WHERE id=2", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "other@example.invalid"
    );
    assert!(
        rows(&tx, "SELECT subject_key FROM gdpr_erasures", [])
            .unwrap()
            .iter()
            .all(|r| r["subject_key"].as_str().unwrap().starts_with("sha256:"))
    );
    let audit = rows(
        &tx,
        "SELECT actor,message,entity_id FROM audit_log WHERE event_type LIKE 'gdpr_%'",
        [],
    )
    .unwrap();
    assert!(audit.iter().all(
        |r| !r["message"].as_str().unwrap().contains("Synthetic Subject")
            && r["entity_id"].as_str().unwrap().starts_with("sha256:")
    ));
    assert_eq!(audit[0]["actor"], "user:reviewer");
    assert!(
        tx.execute("UPDATE gdpr_erasures SET reason='changed'", [])
            .is_err()
    );
    assert!(tx.execute("DELETE FROM gdpr_erasures", []).is_err());
    let repeat = erase(&tx, &i, "2026-10-19", "owner").unwrap();
    assert_eq!(repeat["erasedCount"], 0);
    assert_eq!(repeat["alreadyErasedCount"], 2);
}
#[test]
fn actual_retention_overrides_future_export_and_expired_journal_still_refused() {
    let mut l = fixture();
    l.execute_batch("UPDATE companies SET fiscal_year_start_month=7;INSERT INTO customers(id,name,vat_or_cvr)VALUES(1,'Synthetic Retained','SYN-RETAINED');INSERT INTO documents(id,source,document_type,sender_name,sender_vat_cvr,invoice_date,sha256_hash)VALUES(1,'synthetic','cash_register_receipt','Synthetic Retained','SYN-RETAINED','2026-07-01','synthetic-document-hash');INSERT INTO journal_entries(id,entry_no,transaction_date,text,rule_version,entry_hash)VALUES(1,'SYN-OLD','2010-01-01','Synthetic Retained old journal','fixture','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa');").unwrap();
    let tx = l.transaction().unwrap();
    let i = input(Some("SYN-RETAINED"), None);
    let result = export(&tx, "synthetic", &i, "2026-10-19", "reviewer").unwrap();
    let doc = result["export"]["records"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["source"] == "documents")
        .unwrap();
    assert_eq!(doc["retainUntil"], "2032-06-30");
    assert_eq!(doc["underRetention"], true);
    let mut future = input(Some("SYN-RETAINED"), None);
    future.as_of = Some("2099-01-01".into());
    assert!(
        export(&tx, "synthetic", &future, "2026-10-19", "reviewer").unwrap()["export"]["records"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["source"] == "documents" && r["erasable"] == true)
    );
    assert!(erase(&tx, &future, "2026-10-19", "owner").is_err());
    let erased = erase(&tx, &i, "2026-10-19", "owner").unwrap();
    assert_eq!(erased["erasedCount"], 0);
    assert!(
        erased["refused"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["source"] == "journal_entries"
                && r["reason"].as_str().unwrap().contains("integrity"))
    );
    assert_eq!(
        tx.query_row("SELECT COUNT(*) FROM gdpr_erasures", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        tx.query_row(
            "SELECT entry_hash FROM journal_entries WHERE id=1",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "a".repeat(64)
    );
    let retention = crate::retention::read(&tx, "synthetic", "2026-10-19").unwrap();
    assert_eq!(retention["report"]["rows"][0]["nextExpiry"], "2032-06-30");
    assert_eq!(retention["report"]["rows"][1]["expired"], 1);
}
#[test]
fn ambiguous_names_mismatched_identifiers_and_wildcards_cannot_expand_subject() {
    let mut l = fixture();
    l.execute_batch("INSERT INTO customers(id,name,vat_or_cvr)VALUES(1,'Synthetic Shared','SYN-A'),(2,'Synthetic Shared','SYN-B'),(3,'Synthetic %_ literal','SYN-C');INSERT INTO audit_log(event_type,entity_type,message,actor)VALUES('synthetic','fixture','Synthetic unrelated audit','user:synthetic');").unwrap();
    let tx = l.transaction().unwrap();
    assert!(
        export(
            &tx,
            "synthetic",
            &input(None, Some("Synthetic Shared")),
            "2026-10-19",
            "reader"
        )
        .is_err()
    );
    assert!(
        erase(
            &tx,
            &input(Some("SYN-A"), Some("Synthetic %_ literal")),
            "2026-10-19",
            "owner"
        )
        .is_err()
    );
    assert!(export(&tx, "synthetic", &input(None, None), "2026-10-19", "reader").is_err());
    let result = export(
        &tx,
        "synthetic",
        &input(Some("SYN-A"), None),
        "2026-10-19",
        "reader",
    )
    .unwrap();
    assert_eq!(result["export"]["records"].as_array().unwrap().len(), 1);
    let literal = export(
        &tx,
        "synthetic",
        &input(None, Some("Synthetic %_ literal")),
        "2026-10-19",
        "reader",
    )
    .unwrap();
    assert_eq!(literal["export"]["records"].as_array().unwrap().len(), 1);
}
#[test]
fn legacy_name_tombstone_is_strengthened_without_resurfacing_or_cross_subject_redaction() {
    let mut l = fixture();
    l.execute_batch("INSERT INTO customers(id,name,vat_or_cvr)VALUES(1,'Synthetic Alias','SYN-ALIAS');INSERT INTO gdpr_erasures(subject_key,source,source_row_id,redacted_fields,rule_id,reason)VALUES('Synthetic Alias','customers',1,'[\"name\",\"email\"]','synthetic','legacy fixture');").unwrap();
    {
        let tx = l.transaction().unwrap();
        let report = export(
            &tx,
            "synthetic",
            &input(Some("SYN-ALIAS"), None),
            "2026-10-19",
            "reviewer",
        )
        .unwrap();
        assert_eq!(report["export"]["records"][0]["erased"], true);
        tx.commit().unwrap();
    }
    l.execute_batch(
        "INSERT INTO customers(id,name,vat_or_cvr)VALUES(2,'Synthetic Alias','SYN-OTHER');",
    )
    .unwrap();
    let tx = l.transaction().unwrap();
    assert_eq!(
        export(
            &tx,
            "synthetic",
            &input(Some("SYN-ALIAS"), None),
            "2026-10-19",
            "reviewer"
        )
        .unwrap()["export"]["records"][0]["erased"],
        true
    );
    assert_eq!(
        export(
            &tx,
            "synthetic",
            &input(Some("SYN-OTHER"), None),
            "2026-10-19",
            "reviewer"
        )
        .unwrap()["export"]["records"][0]["erased"],
        false
    );
}

#[test]
fn unknown_retention_is_not_erasable_and_customer_creation_cannot_resurface_pii() {
    let mut l = fixture();
    l.execute_batch("INSERT INTO customers(name,vat_or_cvr)VALUES('Synthetic Unknown','SYN-UNKNOWN');INSERT INTO documents(source,document_type,sha256_hash,sender_name,sender_vat_cvr,invoice_date)VALUES('synthetic','cash_register_receipt','synthetic-unknown','Synthetic Unknown','SYN-UNKNOWN','not-a-date');").unwrap();
    let tx = l.transaction().unwrap();
    let i = input(Some("SYN-UNKNOWN"), None);
    let result = export(&tx, "synthetic", &i, "2026-10-19", "reviewer").unwrap();
    assert!(
        result["export"]["records"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["retentionUnknown"] == true && r["erasable"] == false)
    );
    let result = erase(&tx, &i, "2026-10-19", "owner").unwrap();
    assert_eq!(result["erasedCount"], 0);
    let status = crate::retention::read(&tx, "synthetic", "2026-10-19").unwrap();
    assert_eq!(status["report"]["rows"][0]["unknown"], 1);
    let customer: crate::sales::Customer =
        serde_json::from_value(json!({"name":"Synthetic New","vatOrCvr":"SYN-NEW"})).unwrap();
    crate::sales::create_customer(&tx, &customer, "owner").unwrap();
    let free = input(Some("SYN-NEW"), None);
    erase(&tx, &free, "2026-10-19", "owner").unwrap();
    let before = tx.total_changes();
    assert!(
        crate::sales::create_customer(&tx, &customer, "owner")
            .unwrap_err()
            .message
            .contains("erased contact identity")
    );
    assert_eq!(tx.total_changes(), before);
}
