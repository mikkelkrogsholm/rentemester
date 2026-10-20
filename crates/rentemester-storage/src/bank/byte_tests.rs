use super::*;
use rentemester_engine::bank::{decode_bytes, parse_with_profile};
use rusqlite::{Connection, TransactionBehavior};
fn database() -> Connection {
    let mut control = Connection::open_in_memory().unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic Byte Bank").unwrap();
    db.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('B','Synthetic bank','asset','debit');INSERT INTO bank_accounts(slug,name,ledger_account_no)VALUES('synthetic-bytes','Synthetic bank','B')").unwrap();
    db
}
fn imported(tx: &Transaction<'_>, bytes: &[u8], profile: Option<&str>) -> Value {
    let content = decode_bytes(bytes, profile).unwrap();
    import_bytes(
        tx,
        bytes,
        &parse_with_profile(&content, profile).unwrap(),
        Some("synthetic-bytes"),
        "synthetic",
        "2026-10-20T12:00:00Z",
        "bank/processed/synthetic-original.csv",
    )
    .unwrap()
}
#[test]
fn original_bom_and_newline_bytes_are_hashed_and_source_verified_without_reencoding() {
    let mut db = database();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let original = "\u{feff}date;text;amount\r\n2026-04-01;Synthetic Æble €;-125\r\n";
    let result = imported(&tx, original.as_bytes(), None);
    let expected = format!("{:x}", Sha256::digest(original.as_bytes()));
    assert_eq!(result["sourceFileHash"], expected);
    assert_ne!(
        expected,
        format!(
            "{:x}",
            Sha256::digest(decode_bytes(original.as_bytes(), None).unwrap().as_bytes())
        )
    );
    crate::bank_reconciliation::verify_source(&tx, 1, original.as_bytes()).unwrap();
    assert!(
        crate::bank_reconciliation::verify_source(
            &tx,
            1,
            original.trim_start_matches('\u{feff}').as_bytes()
        )
        .is_err()
    );
    assert!(
        crate::bank_reconciliation::verify_source(
            &tx,
            1,
            original.replace("\r\n", "\n").as_bytes()
        )
        .is_err()
    );
    assert_eq!(source_profile(&tx, 1).unwrap(), None);
}
#[test]
fn encoding_variants_keep_raw_hashes_logical_occurrences_and_exact_source_proof() {
    let mut db = database();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let line = b"01.04.2026;Synthetic \xe6\xf8\xe5 \x80;-125\r\n";
    let mut old = b"Dato;Tekst;Bel\xf8b\r\n".to_vec();
    old.extend_from_slice(line);
    old.extend_from_slice(line);
    let first = imported(&tx, &old, Some("danske-bank"));
    assert_eq!(first["imported"], 2);
    assert_eq!(
        first["sourceFileHash"],
        format!("{:x}", Sha256::digest(&old))
    );
    let utf8 = format!(
        "\u{feff}{}",
        decode_bytes(&old, Some("danske-bank")).unwrap()
    );
    let repeat = imported(&tx, utf8.as_bytes(), Some("danske-bank"));
    assert_eq!(repeat["imported"], 0);
    assert_eq!(repeat["skippedDuplicates"], 2);
    assert_ne!(repeat["sourceFileHash"], first["sourceFileHash"]);
    let more = format!("{utf8}01.04.2026;Synthetic æøå €;-125\r\n");
    let overlap = imported(&tx, more.as_bytes(), Some("danske-bank"));
    assert_eq!(overlap["imported"], 1);
    assert_eq!(overlap["skippedDuplicates"], 2);
    for id in [1, 2] {
        assert_eq!(
            source_profile(&tx, id).unwrap().as_deref(),
            Some("danske-bank")
        );
        crate::bank_reconciliation::verify_source(&tx, id, &old).unwrap();
    }
    crate::bank_reconciliation::verify_source(&tx, 3, more.as_bytes()).unwrap();
    assert!(crate::bank_reconciliation::verify_source(&tx, 1, utf8.as_bytes()).is_err());
    assert!(
        decode_bytes(&old, None).is_err(),
        "generic profile cannot reinterpret legacy bytes"
    );
    let text: String = tx
        .query_row("SELECT text FROM bank_transactions WHERE id=1", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(text, "Synthetic æøå €");
    assert_eq!(
        tx.query_row::<i64, _, _>("SELECT count(*) FROM bank_transactions", [], |r| r.get(0))
            .unwrap(),
        3
    );
    assert_eq!(
        tx.query_row::<i64, _, _>("SELECT count(*) FROM journal_entries", [], |r| r.get(0))
            .unwrap(),
        0
    );
}
