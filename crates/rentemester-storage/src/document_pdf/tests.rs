use super::*;
use crate::documents::{Original, register};
use rusqlite::TransactionBehavior;
const AT: &str = "2026-10-20T12:00:00.000Z";
fn identity() -> ParserIdentity {
    ParserIdentity {
        parser_id: "pdfjs-dist".into(),
        parser_version: "6.2.108".into(),
        contract_version: pdf::LEGACY_CONTRACT.into(),
    }
}
fn result(no_text: bool) -> ParseResult {
    pdf::decode(if no_text {
        include_bytes!("legacy-no-text.json")
    } else {
        include_bytes!("legacy-text.json")
    })
    .unwrap()
}
fn fixture() -> Connection {
    let mut c = Connection::open_in_memory().unwrap();
    let mut l = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic PDF evidence").unwrap();
    l
}
fn document(tx: &Transaction<'_>, r: &ParseResult) -> i64 {
    let metadata=serde_json::from_value(json!({"source":"synthetic","documentType":"cash_register_receipt","issueDate":"2026-10-18","amountIncVat":125,"vatAmount":25})).unwrap();
    register(
        tx,
        Original {
            filename: "synthetic.pdf",
            hash: &r.input_sha256,
            path: &format!("documents/originals/{}.pdf", r.input_sha256),
            mime: "application/pdf",
        },
        &metadata,
        "synthetic",
        "2026-10-20",
    )
    .unwrap()["document"]["id"]
        .as_i64()
        .unwrap()
}
fn parsed() -> Connection {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let r = result(false);
    let id = document(&tx, &r);
    let p = prepare(&tx, id, &r.input_sha256, &identity()).unwrap();
    record_result(&tx, &p, &r, "user:synthetic", "synthetic-parser", AT).unwrap();
    tx.commit().unwrap();
    db
}
#[test]
fn actual_legacy_hashes_cache_no_text_pending_and_public_pagination() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    for (empty, expected, page_hash) in [
        (
            false,
            "52fbc8e31452a05ba4a7d09fc9be1616a1a71a9df8d54788849790bb869257f3",
            "8602dd753ee023c75ee22de7b0b0ba877d3a5bd0b498ea040d464e2e166e4069",
        ),
        (
            true,
            "e38a46d3cded2bc18c9887e2e9afe05ad634568379d2da937a5a824ece686588",
            "45f7b4ea14489b068943d9e488eb70f91d8044fef99ded1e64fdaaf385300072",
        ),
    ] {
        let r = result(empty);
        let id = document(&tx, &r);
        assert!(
            pending(&tx, &identity(), 0, 100).unwrap()["documentIds"]
                .as_array()
                .unwrap()
                .contains(&json!(id))
        );
        let before = tx.total_changes();
        let p = prepare(&tx, id, &r.input_sha256, &identity()).unwrap();
        assert!(p.cached().is_none());
        assert_eq!(before, tx.total_changes());
        let run = record_result(&tx, &p, &r, "user:synthetic", "synthetic-parser", AT).unwrap();
        assert_eq!(run.result_hash, expected);
        assert_eq!(hash(&r.pages[0]).unwrap(), page_hash);
        assert!(!run.cached);
        let cached = record_cached(&tx, &p, "user:synthetic", "synthetic-parser", AT).unwrap();
        assert!(cached.cached);
        assert_eq!(cached.result_hash, run.result_hash);
        let public = text(&tx, id, 0, 1).unwrap();
        assert_eq!(public["parse"]["resultHash"], expected);
        assert!(public["pages"][0].get("layout").is_none());
        assert_eq!(public["nextOffset"], Value::Null);
        assert_eq!(text(&tx, id, 1, 1).unwrap()["pages"], json!([]));
    }
    assert_eq!(
        pending(&tx, &identity(), 0, 100).unwrap()["documentIds"],
        json!([])
    );
    assert_eq!(
        tx.query_row("SELECT count(*) FROM document_pdf_parse_results", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        tx.query_row(
            "SELECT count(*) FROM document_pdf_parse_attempts",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        4
    );
    validate_history(&tx).unwrap();
    tx.commit().unwrap();
    crate::portable_backup::company(&db).unwrap();
}
#[test]
fn rollback_failure_only_and_native_identity_have_no_false_results() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let mut r = result(false);
    let id = document(&tx, &r);
    let native = ParserIdentity {
        parser_id: "pdfium".into(),
        parser_version: "synthetic-engine.adapter1.sha256".into(),
        contract_version: pdf::NATIVE_CONTRACT.into(),
    };
    r.contract_version = native.contract_version.clone();
    let p = prepare(&tx, id, &r.input_sha256, &native).unwrap();
    record_result(&tx, &p, &r, "user:synthetic", "synthetic-parser", AT).unwrap();
    tx.rollback().unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM document_pdf_parse_results", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let id = document(&tx, &r);
    record_failure(
        &tx,
        id,
        Some(&r.input_sha256),
        &native,
        OperationalError::MemoryLimit,
        "user:synthetic",
        "synthetic-parser",
        AT,
    )
    .unwrap();
    assert!(status(&tx, id).unwrap().is_none());
    assert_eq!(
        pending(&tx, &native, 0, 100).unwrap()["documentIds"],
        json!([id])
    );
    validate_history(&tx).unwrap();
    tx.commit().unwrap();
}
#[test]
fn history_corruption_refuses_cache_reads_and_backup_without_repair() {
    for sql in [
        "DROP TRIGGER document_pdf_parse_pages_no_update;UPDATE document_pdf_parse_pages SET text='changed'",
        "DROP TRIGGER document_pdf_parse_results_no_update;UPDATE document_pdf_parse_results SET parser_version='different'",
        "DROP TRIGGER document_pdf_parse_attempts_no_update;UPDATE document_pdf_parse_attempts SET source_sha256_hash='bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb'",
        "DROP TRIGGER document_pdf_parse_pages_no_delete;DELETE FROM document_pdf_parse_pages",
    ] {
        let db = parsed();
        db.execute_batch(sql).unwrap();
        let before = db.total_changes();
        assert!(status(&db, 1).is_err());
        assert!(text(&db, 1, 0, 10).is_err());
        assert!(prepare(&db, 1, &result(false).input_sha256, &identity()).is_err());
        assert!(crate::portable_backup::company(&db).is_err());
        assert_eq!(before, db.total_changes());
    }
}
#[test]
fn canonical_js_edges_keep_negative_zero_exponents_and_utf16_key_order() {
    let value:Value=serde_json::from_str(r#"{"z":-0.0,"a":1e-7,"b":1e-6,"c":1e20,"d":1e21,"e":1.2345678901234567,"\ue000":"private","😀":"astral","text":"é é \u2028\u2029\n\u0000"}"#).unwrap();
    assert_eq!(
        hash(&value).unwrap(),
        "d22931d4fa318d819c65f46c0d6845ffc3a9ab050c6c2a5a69b66b32469a6076"
    );
}
