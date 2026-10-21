use super::*;
use rusqlite::{Connection, TransactionBehavior};
const PRINCIPAL: crate::idempotency::PrincipalKind = crate::idempotency::PrincipalKind::User;
fn fixture() -> Connection {
    let mut c = Connection::open_in_memory().unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut c, &mut db, "Synthetic account lifecycle").unwrap();
    db.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('B','Synthetic bank','asset','debit'),('C','Other bank','asset','debit'),('E','Equity','equity','credit');").unwrap();
    db
}
fn input() -> bank::AccountInput {
    serde_json::from_value(json!({"name":" Åé ÆØ ","slug":" ","currency":" ","bankName":" Synthetic bank ","accountNo":" 123 ","iban":" SYN ","ledgerAccountNo":" B "})).unwrap()
}
#[test]
fn account_lifecycle_keeps_identity_clears_only_present_fields_audits_noop_and_blocks_history_changes()
 {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let first = create(&tx, &input(), "synthetic", PRINCIPAL)
        .unwrap()
        .original()["data"]["account"]
        .clone();
    assert_eq!(first["slug"], "ae");
    assert_eq!(first["currency"], "DKK");
    assert_eq!(first.as_object().unwrap().len(), 14);
    let patch = Patch {
        account_no: Some(" ".into()),
        bic: Some(" SYNBIC ".into()),
        active: Some(false),
        ..Patch::default()
    };
    let edited = update(&tx, " ae ", &patch, "synthetic", PRINCIPAL)
        .unwrap()
        .browser()
        .unwrap();
    for key in ["id", "slug", "createdAt"] {
        assert_eq!(first[key], edited[key]);
    }
    assert!(edited["accountNo"].is_null());
    assert_eq!(edited["iban"], "SYN");
    assert_eq!(edited["bic"], "SYNBIC");
    assert_eq!(edited["active"], false);
    assert_eq!(
        bank::validate_original_import_account(&tx, Some("ae"))
            .unwrap_err()
            .code,
        "bank_import_account_inactive"
    );
    update(
        &tx,
        "1",
        &Patch {
            active: Some(true),
            ..Patch::default()
        },
        "synthetic",
        PRINCIPAL,
    )
    .unwrap()
    .browser()
    .unwrap();
    let before = tx
        .query_row::<i64, _, _>(
            "SELECT count(*) FROM audit_log WHERE event_type='bank_account_update'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let unchanged = update(&tx, "ae", &Patch::default(), "synthetic", PRINCIPAL)
        .unwrap()
        .browser()
        .unwrap();
    assert_eq!(unchanged["id"], first["id"]);
    assert_eq!(
        tx.query_row::<i64, _, _>(
            "SELECT count(*) FROM audit_log WHERE event_type='bank_account_update'",
            [],
            |r| r.get(0)
        )
        .unwrap(),
        before + 1
    );
    let csv = "date,text,amount,currency\n2026-04-01,Synthetic preserved,1.23,DKK\n";
    bank::import(
        &tx,
        csv,
        &rentemester_engine::bank::parse(csv).unwrap(),
        Some("ae"),
        "synthetic",
        "2026-04-01T00:00:00Z",
        "bank/processed/synthetic.txt",
    )
    .unwrap();
    for patch in [
        Patch {
            ledger_account_no: Some("C".into()),
            ..Patch::default()
        },
        Patch {
            ledger_account_no: Some("".into()),
            ..Patch::default()
        },
        Patch {
            currency: Some("eur".into()),
            ..Patch::default()
        },
    ] {
        let refused = update(&tx, "ae", &patch, "synthetic", PRINCIPAL)
            .unwrap()
            .original();
        assert_eq!(refused["ok"], false);
        assert!(refused.get("data").is_none());
    }
    assert_eq!(account(&tx, 1).unwrap(), unchanged);
    assert_eq!(
        tx.query_row::<i64, _, _>("SELECT count(*) FROM journal_entries", [], |r| r.get(0))
            .unwrap(),
        0
    );
}
#[test]
fn account_validation_duplicate_mapping_and_audit_failure_are_explicit() {
    assert_eq!(original_account_slug("Straße ÅÉ æø"), "stra-e-ae");
    let mut db = fixture();
    {
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let invalid: bank::AccountInput =
            serde_json::from_value(json!({"name":"","currency":"EU"})).unwrap();
        assert_eq!(
            create(&tx, &invalid, "synthetic", PRINCIPAL)
                .unwrap()
                .original()["errors"],
            json!([
                "name is required",
                "slug could not be derived; pass an explicit --slug",
                "currency must be a 3-letter ISO currency code"
            ])
        );
        let mut data = input();
        data.ledger_account_no = Some("missing".into());
        let missing = create(&tx, &data, "synthetic", PRINCIPAL)
            .unwrap()
            .original();
        assert_eq!(missing["code"], "NOT_FOUND");
        data.ledger_account_no = Some("E".into());
        assert_eq!(
            create(&tx, &data, "synthetic", PRINCIPAL)
                .unwrap()
                .original()["errors"][0],
            "ledger account E must be an asset account"
        );
        create(&tx, &input(), "synthetic", PRINCIPAL).unwrap();
        assert_eq!(
            create(&tx, &input(), "synthetic", PRINCIPAL)
                .unwrap()
                .original()["errors"][0],
            "a bank account with slug 'ae' already exists"
        );
        assert_eq!(
            update(&tx, "unknown", &Patch::default(), "synthetic", PRINCIPAL)
                .unwrap()
                .original()["code"],
            "NOT_FOUND"
        );
        tx.commit().unwrap();
    }
    db.execute_batch("CREATE TRIGGER synthetic_account_audit_failure BEFORE INSERT ON audit_log WHEN NEW.event_type='bank_account_update' BEGIN SELECT RAISE(ABORT,'synthetic audit failure');END;").unwrap();
    {
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        assert!(
            update(
                &tx,
                "ae",
                &Patch {
                    name: Some("Changed".into()),
                    ..Patch::default()
                },
                "synthetic",
                PRINCIPAL
            )
            .is_err()
        );
    }
    assert_eq!(
        db.query_row::<String, _, _>("SELECT name FROM bank_accounts WHERE id=1", [], |r| r
            .get(0))
            .unwrap(),
        "Åé ÆØ"
    );
}
