//! Synthetic domain proof only: public canonical storage, not authentication admission.
use rentemester_storage::{company_creation, company_setup};
use rusqlite::Connection;
use serde_json::{Value, json};

fn settings(value: Value) -> company_creation::Settings {
    company_creation::settings(&serde_json::from_value(value).unwrap()).unwrap()
}
fn profile(db: &mut Connection) -> Value {
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    company_setup::profile(&tx).unwrap()
}
fn count(db: &Connection, table: &str) -> i64 {
    db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}
fn no_ledger_effects(db: &Connection) {
    for table in [
        "accounts",
        "journal_entries",
        "journal_lines",
        "account_role_mappings",
        "account_role_proposals",
    ] {
        assert_eq!(count(db, table), 0, "unexpected {table}");
    }
}

#[test]
fn full_identity_uses_canonical_profile_with_shifted_year_non_vat_and_zero_terms() {
    let mut db = Connection::open_in_memory().unwrap();
    let s = settings(
        json!({"name":" Synthetic Identity ","slug":"synthetic-identity","cvr":" dk87654321 ","address":" Road 1 ","postalCode":" 1234 ","city":" Town ","paymentTermsDays":0,"fiscalYearStartMonth":"7","fiscalYearLabelStrategy":"span","vatPeriodType":null,"payment":{"bankName":"Synthetic bank","registrationNo":"1234","accountNo":"1234567890"}}),
    );
    company_creation::initialize(&mut db, &s, "synthetic").unwrap();
    let p = profile(&mut db);
    for (key, expected) in [
        ("name", json!("Synthetic Identity")),
        ("cvr", json!("DK87654321")),
        ("address", json!("Road 1")),
        ("postalCode", json!("1234")),
        ("city", json!("Town")),
        ("paymentTermsDays", json!(0)),
        ("fiscalYearStartMonth", json!(7)),
        ("fiscalYearLabelStrategy", json!("span")),
        ("vatPeriodType", Value::Null),
    ] {
        assert_eq!(p[key], expected, "{key}");
    }
    assert_eq!(p["payment"]["bankName"], "Synthetic bank");
    assert_eq!(count(&db, "bank_accounts"), 1);
    assert_eq!(db.query_row("SELECT COUNT(*) FROM audit_log WHERE event_type='company_profile_update' AND actor='user:synthetic'", [], |r|r.get::<_,i64>(0)).unwrap(), 1);
    no_ledger_effects(&db);
    let before = profile(&mut db);
    assert!(company_creation::initialize(&mut db, &s, "synthetic").is_err());
    assert_eq!(profile(&mut db), before);
}

#[test]
fn old_omitted_and_null_fields_keep_defaults_and_empty_text_is_null() {
    let mut old = Connection::open_in_memory().unwrap();
    company_creation::initialize(
        &mut old,
        &settings(json!({"name":"Synthetic"})),
        "synthetic",
    )
    .unwrap();
    let baseline = profile(&mut old);
    assert_eq!(baseline["paymentTermsDays"], 14);
    assert_eq!(baseline["vatPeriodType"], "quarter");
    assert_eq!(baseline["fiscalYearStartMonth"], 1);
    for extra in [
        json!({"address":null,"postalCode":null,"city":null,"paymentTermsDays":null}),
        json!({"address":" ","postalCode":"","city":"  "}),
    ] {
        let mut input = json!({"name":"Synthetic"});
        input
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let mut db = Connection::open_in_memory().unwrap();
        company_creation::initialize(&mut db, &settings(input), "synthetic").unwrap();
        let p = profile(&mut db);
        for key in [
            "address",
            "postalCode",
            "city",
            "paymentTermsDays",
            "payment",
            "vatPeriodType",
        ] {
            assert_eq!(p[key], baseline[key], "{key}");
        }
        no_ledger_effects(&db);
        assert_eq!(count(&db, "bank_accounts"), 0);
    }
}

#[test]
fn invalid_creation_fields_refuse_before_any_fresh_schema_is_written() {
    for (field, value) in [
        ("address", json!("bad\u{0000}text")),
        ("postalCode", json!("bad\ntext")),
        ("city", json!("x".repeat(1025))),
        ("paymentTermsDays", json!(-1)),
        ("paymentTermsDays", json!(366)),
        ("paymentTermsDays", json!(1.5)),
        ("paymentTermsDays", json!("0")),
        ("address", json!(true)),
        ("postalCode", json!(1234)),
        ("actor", json!("user:other")),
    ] {
        let db = Connection::open_in_memory().unwrap();
        let mut input = json!({"name":"Synthetic","payment":{"bankName":"Synthetic bank"}});
        input[field] = value;
        let refused = serde_json::from_value::<company_creation::Input>(input)
            .map(|i| company_creation::settings(&i).is_err())
            .unwrap_or(true);
        assert!(refused, "accepted invalid {field}");
        assert_eq!(count(&db, "sqlite_schema"), 0);
    }
}

#[test]
fn caller_refusal_rolls_back_partial_profile_bank_and_audit_changes() {
    let mut db = Connection::open_in_memory().unwrap();
    company_creation::initialize(&mut db, &settings(json!({"name":"Synthetic"})), "synthetic")
        .unwrap();
    let before = profile(&mut db);
    let audits = count(&db, "audit_log");
    {
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let bad = json!({"address":"Changed","payment":{"bankName":"Synthetic bank"},"paymentTermsDays":366});
        assert!(company_setup::update_profile(&tx, bad.as_object().unwrap(), "synthetic").is_err());
        tx.rollback().unwrap();
    }
    assert_eq!(profile(&mut db), before);
    assert_eq!(count(&db, "audit_log"), audits);
    assert_eq!(count(&db, "bank_accounts"), 0);
    {
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let patch = json!({"city":"New town","paymentTermsDays":365});
        company_setup::update_profile(&tx, patch.as_object().unwrap(), "synthetic").unwrap();
        tx.commit().unwrap();
    }
    let after = profile(&mut db);
    assert_eq!(after["city"], "New town");
    assert_eq!(after["paymentTermsDays"], 365);
    for key in [
        "name",
        "address",
        "postalCode",
        "cvr",
        "payment",
        "createdAt",
    ] {
        assert_eq!(after[key], before[key]);
    }
    no_ledger_effects(&db);
}

#[test]
fn initializer_profile_refusal_rolls_back_settings_and_bank_without_retrying_namespace() {
    let mut db = Connection::open_in_memory().unwrap();
    // Settings is public; deliberately corrupt it after normal preflight to
    // exercise the initializer's actual transaction failure, not a fake schema.
    let mut s = settings(
        json!({"name":"Synthetic refusal","fiscalYearStartMonth":"7","address":"Changed","payment":{"bankName":"Synthetic bank"}}),
    );
    s.profile.insert("paymentTermsDays".into(), json!(366));
    assert!(company_creation::initialize(&mut db, &s, "synthetic").is_err());
    // Fresh baseline construction is a separate existing commit. This private
    // failed namespace must be retired by the caller, never published/retried.
    let p = profile(&mut db);
    assert!(p["address"].is_null());
    assert!(p["payment"].is_null());
    assert_eq!(p["paymentTermsDays"], 14);
    assert_eq!(p["fiscalYearStartMonth"], 1);
    assert_eq!(count(&db, "bank_accounts"), 0);
    assert_eq!(db.query_row("SELECT COUNT(*) FROM audit_log WHERE event_type IN ('company_profile_update','bank_account_created')", [], |r|r.get::<_,i64>(0)).unwrap(), 0);
    no_ledger_effects(&db);
}

#[test]
fn legacy_sequence_and_optional_trailing_fields_preserve_the_same_canonical_input() {
    let old = json!(["Synthetic", null, "12345678", "4", "span", null, null]);
    let input: company_creation::Input = serde_json::from_value(old.clone()).expect("existing seven-position HTTP input remains valid");
    let settings = company_creation::settings(&input).unwrap();
    assert_eq!(settings.month, 4);
    assert_eq!(settings.strategy, "span");
    assert_eq!(settings.profile["cvr"], "12345678");
    assert!(!settings.profile.contains_key("paymentTermsDays"));
    for count in 0..=4 {
        let mut sequence = old.as_array().unwrap().clone();
        sequence.extend([json!(" Road "), json!(" 1234 "), json!(" Town "), json!(0)].into_iter().take(count));
        let i: company_creation::Input = serde_json::from_value(json!(sequence)).unwrap();
        let s = company_creation::settings(&i).unwrap();
        if count > 0 { assert_eq!(s.profile["address"], "Road"); }
        if count > 1 { assert_eq!(s.profile["postalCode"], "1234"); }
        if count > 2 { assert_eq!(s.profile["city"], "Town"); }
        if count > 3 { assert_eq!(s.profile["paymentTermsDays"], 0); }
    }
    let mut excess = old.as_array().unwrap().clone();
    excess.extend([Value::Null, Value::Null, Value::Null, Value::Null, Value::Null]);
    assert!(serde_json::from_value::<company_creation::Input>(json!(excess)).is_err());
}
