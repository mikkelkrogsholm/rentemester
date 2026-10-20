use super::*;
use rentemester_engine::bank::{parse, parse_with_profile};
use rusqlite::Connection;
const NOW: &str = "2026-10-20T12:00:00Z";
const CSV: &str = "\u{feff}Dato;Rentedato;Tekst;Beløb;Valuta;Saldo;Afsender;Oprindelig afsender;Modtagerkonto;Besked;Arkivreference;Kundereference;Ukendt\n04.04.2026;05.04.2026;\"Synthetic; outgoing\";-20;DKK;1130;;Synthetic fallback;1234;Message;AR1;CR1;retained\n04.04.2026;05.04.2026;Synthetic incoming;50;DKK;1150;Synthetic direct;Ignored;4567;Second;AR2;CR2;other\n03.04.2026;04.04.2026;Synthetic opening movement;100;DKK;1100;;;;;;;\n";
fn fixture() -> Connection {
    let mut control = Connection::open_in_memory().unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic Profile Bank").unwrap();
    db
}
fn account(tx: &Transaction<'_>) {
    tx.execute("INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('1872','Synthetic bank asset','asset','debit')", []).unwrap();
    create_account(
        tx,
        &serde_json::from_value(
            json!({"name":"Synthetic Bank","slug":"synthetic-bank","ledgerAccountNo":"1872"}),
        )
        .unwrap(),
        "synthetic",
    )
    .unwrap();
}
fn imported(tx: &Transaction<'_>, csv: &str, now: &str) -> Value {
    import(
        tx,
        csv,
        &parse_with_profile(csv, Some("danske-bank")).unwrap(),
        Some("synthetic-bank"),
        "synthetic",
        now,
        "bank/processed/synthetic-original.csv",
    )
    .unwrap()
}
#[test]
fn named_profile_retains_all_evidence_reconstructs_balances_and_preserves_repeat_identity() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    account(&tx);
    let result = imported(&tx, CSV, NOW);
    assert_eq!(result["imported"], 3);
    assert_eq!(result["profile"], "danske-bank");
    assert_eq!(result["balanceWarnings"], json!([]));
    assert_eq!(result["exceptionsCreated"], 3);
    assert_eq!(result["firstBalance"], json!(1100.0));
    assert_eq!(result["lastBalance"], json!(1130.0));
    let view = read(&tx, "synthetic", "2026-04-04", Some("2026")).unwrap();
    let rows = view["transactions"].as_array().unwrap();
    let row = rows
        .iter()
        .find(|r| r["text"] == "Synthetic; outgoing")
        .unwrap();
    assert_eq!(row["bookingDate"], "2026-04-05");
    assert_eq!(row["counterpartyName"], "Synthetic fallback");
    assert_eq!(row["counterpartyAccount"], "1234");
    assert_eq!(row["archiveReference"], "AR1");
    assert_eq!(row["customerReference"], "CR1");
    assert_eq!(row["raw"]["Ukendt"], "retained");
    assert_eq!(row["balanceAfter"], json!(1130.0));
    assert_eq!(view["actualBalance"], json!(1130.0));
    let id = row["id"].as_i64().unwrap();
    assert_eq!(
        source_profile(&tx, id).unwrap().as_deref(),
        Some("danske-bank")
    );
    // The root integrates the narrowly coordinated profile-aware physical verifier.
    crate::bank_reconciliation::verify_source(&tx, id, CSV.as_bytes()).unwrap();
    let repeated = imported(&tx, CSV, "2026-10-20T12:01:00Z");
    assert_eq!(repeated["imported"], 0);
    assert_eq!(repeated["skippedDuplicates"], 3);
    assert_eq!(repeated["exceptionsCreated"], 0);
    let changed_extra = CSV.replace("retained", "changed source annotation");
    let skipped = imported(&tx, &changed_extra, "2026-10-20T12:02:00Z");
    assert_eq!(skipped["skippedDuplicates"], 3);
    let retained: String = tx
        .query_row(
            "SELECT raw_json FROM bank_transactions WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&retained).unwrap()["Ukendt"],
        "retained"
    );
    assert_eq!(
        tx.query_row("SELECT count(*) FROM journal_entries", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    tx.commit().unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM bank_transactions", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        3
    );
}
#[test]
fn only_fresh_batch_adjacency_creates_a_gap_exception_and_repeat_creates_none() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    account(&tx);
    imported(&tx, CSV, NOW);
    let gap = "Dato;Tekst;Beløb;Saldo\n07.04.2026;Synthetic gap;-10;900\n06.04.2026;Synthetic previous;-10;1120\n";
    let result = imported(&tx, gap, "2026-10-20T12:01:00Z");
    assert_eq!(result["imported"], 2);
    assert_eq!(result["balanceWarnings"].as_array().unwrap().len(), 1);
    assert_eq!(result["exceptionsCreated"], 3);
    let evidence: String = tx
        .query_row(
            "SELECT source_evidence FROM exceptions WHERE type='BANK_BALANCE_GAP'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let evidence: Value = serde_json::from_str(&evidence).unwrap();
    assert_eq!(evidence["expectedBalance"], json!(1110.0));
    assert_eq!(evidence["statementBalance"], json!(900.0));
    assert_eq!(evidence["drift"], json!(-210.0));
    let repeated = imported(&tx, gap, "2026-10-20T12:02:00Z");
    assert_eq!(repeated["imported"], 0);
    assert_eq!(repeated["skippedDuplicates"], 2);
    assert_eq!(repeated["balanceWarnings"], json!([]));
    assert_eq!(repeated["exceptionsCreated"], 0);
    let single = imported(
        &tx,
        "Dato;Tekst;Beløb;Saldo\n08.04.2026;Synthetic isolated statement;-10;1\n",
        "2026-10-20T12:03:00Z",
    );
    assert_eq!(single["exceptionsCreated"], 1);
    assert_eq!(
        tx.query_row(
            "SELECT count(*) FROM exceptions WHERE type='BANK_BALANCE_GAP'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
}
#[test]
fn generic_raw_history_does_not_imply_a_native_profile_and_unknown_account_is_readonly() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    account(&tx);
    let before = tx.total_changes();
    assert!(validate_import_account(&tx, Some("missing")).is_err());
    assert_eq!(tx.total_changes(), before);
    let csv = "date;text;amount\n2026-04-03;Synthetic legacy generic;1\n";
    let mut rows = parse(csv).unwrap();
    rows[0].raw = Some(std::collections::BTreeMap::from([(
        "Legacy".into(),
        "raw".into(),
    )]));
    import(
        &tx,
        csv,
        &rows,
        Some("synthetic-bank"),
        "synthetic",
        NOW,
        "synthetic.csv",
    )
    .unwrap();
    let id = tx
        .query_row("SELECT id FROM bank_transactions", [], |r| {
            r.get::<_, i64>(0)
        })
        .unwrap();
    assert_eq!(source_profile(&tx, id).unwrap(), None);
    crate::bank_reconciliation::verify_source(&tx, id, csv.as_bytes()).unwrap();
}
#[test]
fn missing_unknown_or_conflicting_native_profile_proofs_refuse() {
    for kind in ["missing", "unknown", "conflicting"] {
        let mut db = fixture();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        account(&tx);
        imported(&tx, CSV, NOW);
        let id = tx
            .query_row(
                "SELECT id FROM bank_transactions ORDER BY id LIMIT 1",
                [],
                |r| r.get::<_, i64>(0),
            )
            .unwrap();
        tx.execute_batch("DROP TRIGGER audit_log_no_update; DROP TRIGGER audit_log_no_delete;")
            .unwrap();
        match kind {
            "missing" => {
                tx.execute(
                    "DELETE FROM audit_log WHERE event_type='bank_import_profile'",
                    [],
                )
                .unwrap();
            }
            "unknown" => {
                let raw: String = tx
                    .query_row(
                        "SELECT message FROM audit_log WHERE event_type='bank_import_profile'",
                        [],
                        |r| r.get(0),
                    )
                    .unwrap();
                let mut value: Value = serde_json::from_str(&raw).unwrap();
                value["profile"] = json!("unknown");
                tx.execute(
                    "UPDATE audit_log SET message=?1 WHERE event_type='bank_import_profile'",
                    [crate::intercompany_dispositions::canonical(&value)],
                )
                .unwrap();
            }
            _ => {
                tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) SELECT event_type,entity_type,entity_id,message,actor FROM audit_log WHERE event_type='bank_import_profile'",[]).unwrap();
            }
        }
        assert!(source_profile(&tx, id).is_err());
        assert!(crate::bank_reconciliation::verify_source(&tx, id, CSV.as_bytes()).is_err());
    }
}
