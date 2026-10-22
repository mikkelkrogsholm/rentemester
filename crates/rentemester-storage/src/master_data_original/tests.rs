use super::*;
#[test]
fn master_data_original_pure_lists_null_terms_mixed_case_archived_all_and_typed_audit() {
    let control_file = tempfile::NamedTempFile::new().unwrap();
    let ledger_file = tempfile::NamedTempFile::new().unwrap();
    let mut control = rusqlite::Connection::open(control_file.path()).unwrap();
    let mut db = rusqlite::Connection::open(ledger_file.path()).unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic").unwrap();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    for (name, principal, subject) in [
        (
            "zulu",
            PrincipalKind::User,
            "service_account:reserved-human",
        ),
        ("Alpha", PrincipalKind::ServiceAccount, "trusted-service"),
        ("alpha", PrincipalKind::User, "ordinary"),
    ] {
        create(
            &tx,
            Kind::Customer,
            &json!({"name":name,"email":"  ","defaultCurrency":" eur "}),
            subject,
            principal,
        )
        .unwrap();
    }
    let rows = list(&tx, Kind::Customer, false).unwrap();
    assert_eq!(rows["count"], 3);
    assert_eq!(rows["rows"][0]["name"], "Alpha");
    assert_eq!(rows["rows"][1]["name"], "alpha");
    assert_eq!(rows["rows"][0]["paymentTermsDays"], Value::Null);
    assert_eq!(rows["rows"][0]["email"], Value::Null);
    assert_eq!(rows["rows"][0]["defaultCurrency"], "EUR");
    assert_eq!(rows["rows"][0].as_object().unwrap().len(), 13);
    assert_eq!(rows["rows"][0]["archived"], false);
    // Explicit synthetic boundary fixture, not an operational archive workflow.
    tx.execute("UPDATE customers SET archived=1 WHERE name='Alpha'", [])
        .unwrap();
    assert_eq!(list(&tx, Kind::Customer, false).unwrap()["count"], 2);
    assert_eq!(list(&tx, Kind::Customer, true).unwrap()["count"], 3);
    create(
        &tx,
        Kind::Vendor,
        &json!({"name":"Synthetic vendor"}),
        "trusted-service",
        PrincipalKind::ServiceAccount,
    )
    .unwrap();
    let vendors = list(&tx, Kind::Vendor, false).unwrap();
    assert_eq!(vendors["rows"][0].as_object().unwrap().len(), 15);
    assert_eq!(
        vendors["rows"][0]["identityStatus"],
        "human_resolution_required"
    );
    assert_eq!(vendors["rows"][0]["defaultExpenseAccount"], Value::Null);
    for (event, actor) in [
        ("customer_add", "user:service_account:reserved-human"),
        ("vendor_create", "service_account:trusted-service"),
    ] {
        assert_eq!(
            tx.query_row::<i64, _, _>(
                "SELECT count(*) FROM audit_log WHERE event_type=?1 AND actor=?2",
                [event, actor],
                |r| r.get(0)
            )
            .unwrap(),
            1
        );
    }
    for table in ["documents", "journal_entries", "bank_transactions"] {
        assert_eq!(
            tx.query_row::<i64, _, _>(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
                .unwrap(),
            0
        );
    }
    for input in [
        json!({"name":"Bad account","defaultExpenseAccount":"missing"}),
        json!({"name":"Bad VAT","defaultVatTreatment":"invented"}),
    ] {
        assert!(create(&tx, Kind::Vendor, &input, "ordinary", PrincipalKind::User).is_err());
    }
}
#[test]
fn master_data_original_cvr_fill_overrides_only_source_fields() {
    let company = json!({"name":"Synthetic provider","address":"Synthetic Street 1","postalCode":"1000","city":"Synthetic city","cvr":"12345678","email":"provider@example.invalid","phone":"123","website":"https://example.invalid","countryCode":"DK"});
    let input=compose(&json!({"name":" Manual ","address":" ","notes":" note ","paymentTermsDays":7,"defaultCurrency":"eur","countryCode":"DE"}),Some(&company)).unwrap();
    assert_eq!(input["name"], "Manual");
    assert_eq!(input["address"], "Synthetic Street 1, 1000 Synthetic city");
    assert_eq!(input["vatOrCvr"], "DK12345678");
    assert_eq!(input["countryCode"], "DE");
    assert_eq!(input["paymentTermsDays"], 7);
    assert_eq!(input["notes"], "note");
}
