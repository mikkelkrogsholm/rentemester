use super::*;
use rentemester_engine::{document::Metadata, fx::FxRate, money::Money};
use rusqlite::{Connection, TransactionBehavior};

fn register(tx: &Transaction<'_>, country: &str, gross: i64, vat: i64) -> i64 {
    let sender = match country {
        "DK" => {
            json!({"name":"Synthetic supplier","address":"Synthetic 1","vatOrCvr":"DK12345678"})
        }
        "DE" => {
            json!({"name":"Synthetic supplier","address":"Synthetic 1","vatOrCvr":"DE123456789","countryCode":"DE","identifierKind":"eu_vat"})
        }
        _ => {
            json!({"name":"Synthetic supplier","address":"Synthetic 1","vatOrCvr":"REG12345","countryCode":"US","identifierKind":"non_eu"})
        }
    };
    let meta:Metadata=serde_json::from_value(json!({"source":"synthetic","documentType":"purchase_sale","issueDate":"2026-10-18","deliveryDescription":"Synthetic supplies","invoiceNo":"SYN-1","amountIncVat":gross,"vatAmount":vat,"sender":sender,"recipient":{"name":"Synthetic buyer","address":"Synthetic 2","vatOrCvr":"DK87654321"},"reverseChargeWordingConfirmed":true})).unwrap();
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
        "2026-10-18",
    )
    .unwrap();
    tx.query_row("SELECT MAX(id) FROM documents", [], |r| r.get(0))
        .unwrap()
}
fn setup(tx: &Transaction<'_>) {
    tx.execute_batch("UPDATE companies SET vat_period_type='quarter',cvr='DK87654321' WHERE id=1; INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('A','Synthetic cash','asset','debit'),('X','Synthetic expense','expense','debit'),('V','Synthetic input VAT','vat','debit'),('O','Synthetic reverse VAT','vat','credit'); INSERT INTO account_role_mappings(role,account_no,status,version,confirmed_by) VALUES('input_vat','V','confirmed',1,'synthetic'),('reverse_charge_vat','O','confirmed',1,'synthetic'); INSERT INTO bank_accounts(slug,name,ledger_account_no) VALUES('synthetic','Synthetic bank','A');").unwrap();
}
fn cache(tx: &Transaction<'_>) {
    let now: String = tx
        .query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ','now')", [], |r| {
            r.get(0)
        })
        .unwrap();
    let record=rentemester_engine::vies::record(&json!({"countryCode":"DE","vatNumber":&"DE123456789"[2..],"normalized":"DE123456789","valid":true,"name":null,"address":null,"rawResponse":null}),&now).unwrap();
    crate::vies::store(tx, &record, "user:synthetic").unwrap();
}
#[test]
fn six_source_treatments_allocate_exact_lines_and_registration_refusals() {
    // Source expense-booking.ts/vat.ts: representation reports full invoice VAT
    // but deducts only 25%; foreign self-assessment reports document VAT zero.
    for (treatment, country, gross, vat, expected) in [
        (
            "standard",
            "DK",
            125,
            25,
            vec![
                ("X", "100.00", "0.00", Some("DK_PURCHASE_25")),
                ("V", "25.00", "0.00", None),
            ],
        ),
        (
            "representation",
            "DK",
            125,
            25,
            vec![
                ("X", "100.00", "0.00", Some("REPRESENTATION_SPECIAL")),
                (
                    "X",
                    "18.75",
                    "0.00",
                    Some("REPRESENTATION_NON_DEDUCTIBLE_VAT"),
                ),
                ("V", "6.25", "0.00", None),
            ],
        ),
        (
            "reverse_charge",
            "DE",
            100,
            0,
            vec![
                ("X", "100.00", "0.00", Some("EU_SERVICE_REVERSE_CHARGE")),
                ("V", "25.00", "0.00", None),
                ("O", "0.00", "25.00", None),
            ],
        ),
        (
            "eu_goods_acquisition",
            "DE",
            100,
            0,
            vec![
                ("X", "100.00", "0.00", Some("EU_GOODS_ACQUISITION")),
                ("V", "25.00", "0.00", None),
                ("O", "0.00", "25.00", None),
            ],
        ),
        ("exempt", "DK", 100, 0, vec![("X", "100.00", "0.00", None)]),
        (
            "non_deductible",
            "DK",
            125,
            25,
            vec![("X", "125.00", "0.00", None)],
        ),
        (
            "reverse_charge",
            "US",
            100,
            0,
            vec![
                ("X", "100.00", "0.00", Some("NON_EU_SERVICE_REVERSE_CHARGE")),
                ("V", "25.00", "0.00", None),
                ("O", "0.00", "25.00", None),
            ],
        ),
    ] {
        let mut control = Connection::open_in_memory().unwrap();
        let mut db = Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic treatments").unwrap();
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        setup(&tx);
        let id = register(&tx, country, gross, vat);
        if country == "DE" {
            cache(&tx);
        }
        let meta = expense::metadata(&tx, id).unwrap();
        let allocate = |requested| {
            expense::purchase(
                &tx,
                id,
                &meta,
                "X",
                requested,
                &Money::parse(&gross.to_string()).unwrap(),
                &FxRate::parse("1").unwrap(),
            )
        };
        let allocation = allocate(Some(treatment)).unwrap();
        let actual: Vec<_> = allocation
            .lines
            .iter()
            .map(|l| {
                (
                    l.account_no.as_str().to_owned(),
                    l.debit_amount.decimal(),
                    l.credit_amount.decimal(),
                    l.vat_code.clone(),
                )
            })
            .collect();
        let expected: Vec<_> = expected
            .into_iter()
            .map(|(a, d, c, v)| {
                (
                    a.to_owned(),
                    d.to_owned(),
                    c.to_owned(),
                    v.map(str::to_owned),
                )
            })
            .collect();
        assert_eq!(actual, expected, "{treatment}/{country}");
        tx.execute("UPDATE companies SET vat_period_type=NULL WHERE id=1", [])
            .unwrap();
        assert_eq!(
            allocate(Some(treatment)).is_err(),
            matches!(
                treatment,
                "standard" | "representation" | "reverse_charge" | "eu_goods_acquisition"
            )
        );
        if country == "DK" && vat > 0 {
            tx.execute(
                "UPDATE accounts SET default_vat_code='DK_PURCHASE_25' WHERE account_no='X'",
                [],
            )
            .unwrap();
            assert_eq!(allocate(None).unwrap().treatment, "non_deductible");
        }
    }
}
#[test]
fn optional_key_replays_typed_atomic_outcome_but_conflicts_and_new_keys_cannot_repost() {
    let mut control = Connection::open_in_memory().unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic receipt").unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    setup(&tx);
    register(&tx, "DK", 125, 25);
    let csv = "date;text;amount\n2026-10-18;Synthetic payment;-125\n";
    crate::bank::import(
        &tx,
        csv,
        &rentemester_engine::bank::parse(csv).unwrap(),
        Some("synthetic"),
        "synthetic",
        "2026-10-18T12:00:00Z",
        "synthetic-original",
    )
    .unwrap();
    let mut input:Input=serde_json::from_value(json!({"documentId":1,"bankTransactionId":1,"expenseAccount":"X","vatTreatment":"standard","idempotencyKey":"synthetic-key"})).unwrap();
    let latest = Date::parse("2026-12-01").unwrap();
    let first = book(
        &tx,
        &input,
        "synthetic",
        PrincipalKind::ServiceAccount,
        latest,
        1_792_320_000_000,
    )
    .unwrap();
    let second = book(
        &tx,
        &input,
        "synthetic",
        PrincipalKind::ServiceAccount,
        latest,
        1_792_320_000_001,
    )
    .unwrap();
    assert_eq!(first["idempotency"]["replayed"], false);
    assert_eq!(second["idempotency"]["replayed"], true);
    assert_eq!(first["entryId"], second["entryId"]);
    input.text = Some("Different intent".into());
    assert_eq!(
        book(
            &tx,
            &input,
            "synthetic",
            PrincipalKind::ServiceAccount,
            latest,
            1_792_320_000_002
        )
        .unwrap_err()
        .error
        .code,
        "IDEMPOTENCY_CONFLICT"
    );
    input.idempotency_key = None;
    assert!(
        book(
            &tx,
            &input,
            "synthetic",
            PrincipalKind::ServiceAccount,
            latest,
            1_792_320_000_003
        )
        .is_err()
    );
    assert_eq!(
        tx.query_row::<i64, _, _>("SELECT count(*) FROM journal_entries", [], |r| r.get(0))
            .unwrap(),
        1
    );
    let (actor, program): (String, String) = tx
        .query_row(
            "SELECT created_by,created_by_program FROM journal_entries",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        (actor.as_str(), program.as_str()),
        ("service_account:synthetic", "service-principal")
    );
}
#[test]
fn foreign_vies_and_invoice_evidence_are_not_optional_preflight_events() {
    for country in ["DE", "US"] {
        let mut control = Connection::open_in_memory().unwrap();
        let mut db = Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic foreign refusal")
            .unwrap();
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        setup(&tx);
        let id = register(&tx, country, 100, 0);
        let mut meta = expense::metadata(&tx, id).unwrap();
        let purchase = |m: &Metadata| {
            expense::purchase(
                &tx,
                id,
                m,
                "X",
                Some("reverse_charge"),
                &Money::parse("100").unwrap(),
                &FxRate::parse("1").unwrap(),
            )
        };
        if country == "DE" {
            assert!(purchase(&meta).is_err());
            cache(&tx);
            assert!(purchase(&meta).is_ok());
        } else {
            assert!(purchase(&meta).is_ok());
            meta.reverse_charge_wording_confirmed = None;
            assert!(purchase(&meta).is_err());
        }
        assert_eq!(
            tx.query_row::<i64, _, _>("SELECT count(*) FROM vat_validation_events", [], |r| r
                .get(0))
                .unwrap(),
            0
        );
    }
}
