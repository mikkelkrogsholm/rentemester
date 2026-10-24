use super::*;
use rusqlite::Connection;
const NOW: &str = "2026-10-20T12:00:00Z";
fn database() -> Connection {
    let mut control = Connection::open_in_memory().unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic importer").unwrap();
    db
}
fn input(rows: &str, enrich: bool) -> Input {
    Input {
        file_name: "synthetic.csv".into(),
        content: format!(
            "Kontaktnavn;CVR-nummer;Kontakttype;Landekode;Total salg;Total køb;EAN-nummer;Betalingsfrist i dage;Adresse\n{rows}"
        ),
        enrich_cvr: enrich,
        confirm: true,
        default_role: None,
    }
}
fn lookup(cvr: &str) -> Lookup {
    Lookup {
        cvr: cvr.into(),
        outcome: LookupOutcome::Found(
            json!({"cvrNummer":cvr,"navne":[{"navn":"Registry snapshot"}],"beliggenhedsadresse":[{"vejnavn":"Registry street","husnummerFra":2,"postnummer":8000,"postdistrikt":"Registry city"}],"elektroniskPost":[{"kontaktoplysning":"public@example.invalid"}]}),
        ),
    }
}
fn count(tx: &Transaction<'_>, table: &str) -> i64 {
    tx.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}
#[test]
fn import_reuses_writers_complete_cache_and_exact_dedup_without_accounting_effects() {
    let mut db = database();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let i = input(
        "Buyer;12345678;Company;DK;1;0;5790000000000;730;\nBoth;87654321;Company;DK;1;1;;;CSV street\nNo identity;;Other;;0;0;;;\nFallback;11223344;Company;DK;0;1;;;CSV fallback\n",
        true,
    );
    let p = prepare(&tx, &i, NOW).unwrap();
    assert_eq!(p.lookup_needs.len(), 3);
    assert_eq!(count(&tx, "customers"), 0);
    let result = execute(
        &tx,
        &p,
        &[
            lookup("12345678"),
            lookup("87654321"),
            Lookup {
                cvr: "11223344".into(),
                outcome: LookupOutcome::Failed("provider unavailable".into()),
            },
        ],
        "synthetic",
        NOW,
    )
    .unwrap();
    assert_eq!(
        result["import"]["summary"],
        json!({"parsed":4,"customersCreated":2,"vendorsCreated":3,"skipped":0,"enriched":2,"enrichmentFailures":1})
    );
    let row: (String, i64, String) = tx
        .query_row(
            "SELECT ean_number,payment_terms_days,address FROM customers WHERE name='Buyer'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        row,
        (
            "5790000000000".into(),
            730,
            "Registry street 2, 8000 Registry city".into()
        )
    );
    assert_eq!(
        tx.query_row::<String, _, _>("SELECT address FROM customers WHERE name='Both'", [], |r| r
            .get(0))
            .unwrap(),
        "CSV street"
    );
    assert_eq!(count(&tx, "cvr_lookups"), 2);
    assert_eq!(count(&tx, "journal_entries"), 0);
    let changes = tx.total_changes();
    let p = prepare(&tx, &i, NOW).unwrap();
    assert!(p.lookup_needs.is_empty());
    let repeated = execute(&tx, &p, &[], "synthetic", NOW).unwrap();
    assert_eq!(repeated["import"]["summary"]["skipped"], 5);
    assert_eq!(changes, tx.total_changes());
    crate::sales::update_customer(
        &tx,
        1,
        &json!({"eanNumber":"579 0000000000","paymentTermsDays":9007199254740991i64}),
        "synthetic",
    )
    .unwrap();
    assert!(
        crate::sales::update_customer(
            &tx,
            1,
            &json!({"paymentTermsDays":9007199254740992i64}),
            "synthetic"
        )
        .is_err()
    );
}
#[test]
fn fresh_cache_stale_fallback_and_per_role_failures_follow_source() {
    let mut db = database();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let i = input("First;12345678;Company;DK;1;0;;;\n", true);
    let p = prepare(&tx, &i, NOW).unwrap();
    execute(&tx, &p, &[lookup("12345678")], "synthetic", NOW).unwrap();
    let i = input("Second;12345678;Company;DK;1;1;;;\n", true);
    let p = prepare(&tx, &i, NOW).unwrap();
    assert!(p.lookup_needs.is_empty());
    let r = execute(&tx, &p, &[], "synthetic", NOW).unwrap();
    assert_eq!(r["import"]["summary"]["enriched"], 1);
    let later = "2027-01-01T12:00:00Z";
    let i = input(
        "Third;12345678;Company;DK;1;1;;;\nMissing;87654321;Company;DK;1;1;;;\n",
        true,
    );
    let p = prepare(&tx, &i, later).unwrap();
    assert_eq!(p.lookup_needs.len(), 2);
    let failures = ["12345678", "87654321"].map(|cvr| Lookup {
        cvr: cvr.into(),
        outcome: LookupOutcome::Failed("not configured".into()),
    });
    let r = execute(&tx, &p, &failures, "synthetic", later).unwrap();
    assert_eq!(r["import"]["summary"]["enriched"], 1);
    assert_eq!(r["import"]["summary"]["enrichmentFailures"], 2);
}
#[test]
fn expected_row_refusal_is_partial_but_storage_failure_rolls_back_every_effect() {
    let mut db = database();
    {
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let mut i = input(
            "Invalid;12345678;Company;DK;1;1;bad;0;\nGood;;Other;;0;0;;;\n",
            false,
        );
        i.default_role = Some(Role::Vendor);
        let p = prepare(&tx, &i, NOW).unwrap();
        let r = execute(&tx, &p, &[], "synthetic", NOW).unwrap();
        assert_eq!(r["import"]["summary"]["customersCreated"], 0);
        assert_eq!(r["import"]["summary"]["vendorsCreated"], 2);
        tx.rollback().unwrap();
    }
    db.execute_batch("CREATE TEMP TRIGGER synthetic_failure BEFORE INSERT ON vendors WHEN NEW.name='Fail' BEGIN SELECT RAISE(ABORT,'synthetic unavailable'); END;").unwrap();
    {
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let mut i = input(
            "First;12345678;Company;DK;1;0;;;\nFail;;Other;;0;0;;;\n",
            true,
        );
        i.default_role = Some(Role::Vendor);
        let p = prepare(&tx, &i, NOW).unwrap();
        let e = execute(&tx, &p, &[lookup("12345678")], "synthetic", NOW).unwrap_err();
        assert_eq!(e.code, "contact_storage_unavailable");
        tx.rollback().unwrap();
    }
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    for table in ["customers", "vendors", "cvr_lookups", "audit_log"] {
        assert_eq!(count(&tx, table), 0, "{table}");
    }
}
#[test]
fn malformed_import_is_prewrite_and_warning_only_empty_import_is_successful() {
    let mut db = database();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let changes = tx.total_changes();
    let mut i = input("", false);
    i.content = "unknown;format\na;b".into();
    assert_eq!(
        prepare(&tx, &i, NOW).unwrap_err().code,
        "contact_import_invalid"
    );
    let i = input("Invalid;;Other;;\"1,25\";0;;;\n", false);
    let p = prepare(&tx, &i, NOW).unwrap();
    let r = execute(&tx, &p, &[], "synthetic", NOW).unwrap();
    assert_eq!(r["import"]["summary"]["parsed"], 0);
    assert_eq!(r["import"]["errors"].as_array().unwrap().len(), 1);
    assert_eq!(changes, tx.total_changes());
}
#[test]
fn archived_null_identity_is_not_reactivated_and_case_sensitive_names_remain_distinct() {
    let mut db = database();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let i = input("Retained;;Other;;0;0;;;\n", false);
    let p = prepare(&tx, &i, NOW).unwrap();
    execute(&tx, &p, &[], "synthetic", NOW).unwrap();
    tx.execute("UPDATE vendors SET archived=1 WHERE name='Retained'", [])
        .unwrap();
    let p = prepare(&tx, &i, NOW).unwrap();
    let r = execute(&tx, &p, &[], "synthetic", NOW).unwrap();
    assert_eq!(r["import"]["summary"]["skipped"], 1);
    assert_eq!(
        tx.query_row::<i64, _, _>(
            "SELECT archived FROM vendors WHERE name='Retained'",
            [],
            |r| r.get(0)
        )
        .unwrap(),
        1
    );
    let i = input("retained;;Other;;0;0;;;\n", false);
    let p = prepare(&tx, &i, NOW).unwrap();
    assert_eq!(
        execute(&tx, &p, &[], "synthetic", NOW).unwrap()["import"]["summary"]["vendorsCreated"],
        1
    );
}
#[test]
fn current_gdpr_erasure_prevents_reintroduction_and_provider_disclosure() {
    let mut db = database();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let i = input("Erased;12345678;Company;DK;1;0;;;\n", false);
    let p = prepare(&tx, &i, NOW).unwrap();
    execute(&tx, &p, &[], "synthetic", NOW).unwrap();
    crate::gdpr::erase(
        &tx,
        &crate::gdpr::Input {
            cvr: Some("DK12345678".into()),
            name: None,
            as_of: None,
            confirm: true,
        },
        "2099-01-01",
        "synthetic",
    )
    .unwrap();
    let i = input("Erased;12345678;Company;DK;1;1;;;\n", true);
    let p = prepare(&tx, &i, NOW).unwrap();
    assert!(p.lookup_needs.is_empty());
    let r = execute(&tx, &p, &[], "synthetic", NOW).unwrap();
    assert_eq!(r["import"]["summary"]["customersCreated"], 0);
    assert_eq!(r["import"]["summary"]["vendorsCreated"], 0);
    assert_eq!(count(&tx, "cvr_lookups"), 0);
    assert!(!r["import"]["errors"].as_array().unwrap().is_empty());
}

#[test]
fn explicit_original_roles_minimal_headers_partial_rows_and_repeat_are_masterdata_only() {
    for fallback in [Role::Customer, Role::Vendor] {
        let mut db = database();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let mut i = input("", false);
        i.default_role = Some(fallback);
        i.content = "Kontaktnavn;Total salg;Total køb\nNone;0;0\nSales;1;0\nPurchases;0;1\nBoth;1;1\nInvalid;NaN;0\n;0;1\n".into();
        let accounts = count(&tx, "accounts");
        let p = prepare(&tx, &i, NOW).unwrap();
        let r = execute(&tx, &p, &[], "synthetic", NOW).unwrap();
        assert_eq!(r["ok"], true);
        assert_eq!(r["import"]["summary"]["parsed"], 4);
        assert_eq!(r["import"]["errors"].as_array().unwrap().len(), 2);
        assert_eq!(
            count(&tx, "customers"),
            if fallback == Role::Customer { 3 } else { 2 }
        );
        assert_eq!(
            count(&tx, "vendors"),
            if fallback == Role::Vendor { 3 } else { 2 }
        );
        let changes = tx.total_changes();
        let p = prepare(&tx, &i, NOW).unwrap();
        let r = execute(&tx, &p, &[], "synthetic", NOW).unwrap();
        assert_eq!(r["import"]["summary"]["skipped"], 5);
        assert_eq!(changes, tx.total_changes());
        i.content = "Kontaktnavn\nMinimal\n".into();
        let p = prepare(&tx, &i, NOW).unwrap();
        let r = execute(&tx, &p, &[], "synthetic", NOW).unwrap();
        assert_eq!(
            r["import"]["summary"][if fallback == Role::Customer {
                "customersCreated"
            } else {
                "vendorsCreated"
            }],
            1
        );
        i.default_role = None;
        assert!(prepare(&tx, &i, NOW).is_err());
        i.default_role = Some(fallback);
        i.content = "Other\nMissing name header\n".into();
        assert!(prepare(&tx, &i, NOW).is_err());
        i.content = "Kontaktnavn\nUnconfirmed\n".into();
        i.confirm = false;
        assert!(prepare(&tx, &i, NOW).is_err());
        assert_eq!(count(&tx, "accounts"), accounts);
        assert_eq!(count(&tx, "journal_entries"), 0);
        assert_eq!(count(&tx, "cvr_lookups"), 0);
    }
}

#[test]
fn original_fallback_is_retained_for_lookup_dedup_cache_and_erasure() {
    let mut db = database();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let mut i = input("", true);
    i.default_role = Some(Role::Customer);
    i.content = "Kontaktnavn;CVR-nummer;Landekode\nFallback;12345678;DK\n".into();
    let p = prepare(&tx, &i, NOW).unwrap();
    assert_eq!(p.lookup_needs.len(), 1);
    // Mutating caller input after preparation must not change the prepared role.
    i.default_role = Some(Role::Vendor);
    let r = execute(&tx, &p, &[lookup("12345678")], "synthetic", NOW).unwrap();
    assert_eq!(r["import"]["summary"]["customersCreated"], 1);
    assert_eq!(count(&tx, "vendors"), 0);
    // Existing customer is not an existing vendor; fresh cache still prevents disclosure.
    let p = prepare(&tx, &i, NOW).unwrap();
    assert!(p.lookup_needs.is_empty());
    let r = execute(&tx, &p, &[], "synthetic", NOW).unwrap();
    assert_eq!(r["import"]["summary"]["vendorsCreated"], 1);
    assert_eq!(r["import"]["summary"]["enriched"], 1);
    crate::gdpr::erase(
        &tx,
        &crate::gdpr::Input {
            cvr: Some("DK12345678".into()),
            name: None,
            as_of: None,
            confirm: true,
        },
        "2099-01-01",
        "synthetic",
    )
    .unwrap();
    // A new name avoids the retained-record dedup path and exercises the CVR tombstone.
    i.content = "Kontaktnavn;CVR-nummer;Landekode\nRenamed erased;12345678;DK\n".into();
    for role in [Role::Customer, Role::Vendor] {
        i.default_role = Some(role);
        let p = prepare(&tx, &i, NOW).unwrap();
        assert!(p.lookup_needs.is_empty());
        let r = execute(&tx, &p, &[], "synthetic", NOW).unwrap();
        assert_eq!(r["import"]["summary"]["customersCreated"], 0);
        assert_eq!(r["import"]["summary"]["vendorsCreated"], 0);
        assert!(!r["import"]["errors"].as_array().unwrap().is_empty());
    }
    assert_eq!(count(&tx, "journal_entries"), 0);
}
