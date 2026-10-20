use super::*;
use rusqlite::{Connection, TransactionBehavior};
const NOW: &str = "2026-01-01T00:00:00.000Z";
fn database() -> Connection {
    let mut control = Connection::open_in_memory().unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic Local Name").unwrap();
    db
}
fn entity() -> Value {
    json!({"cvrNummer":87654321,"virksomhedMetadata":{"nyesteNavn":{"navn":"Synthetic Registry Company"},"nyesteBeliggenhedsadresse":{"vejnavn":"Synthetic Road","husnummerFra":7,"postnummer":1234,"postdistrikt":"Synthetic City"},"nyesteVirksomhedsform":{"kortBeskrivelse":"ApS"},"nyesteHovedbranche":{"branchekode":"123456","branchetekst":"Synthetic industry"},"sammensatStatus":"NORMAL"},"attributter":[{"type":"REGNSKABSÅR_START","vaerdier":[{"vaerdi":"--07-01","periode":{"gyldigTil":null}}]},{"type":"REVISION_FRAVALGT","vaerdier":[{"vaerdi":"true","periode":{"gyldigTil":null}}]}]})
}
#[test]
fn normalization_fresh_and_stale_failure_fallback_preserve_raw_and_original_time() {
    assert_eq!(normalize(" dk 87 65 43 21 ").as_deref(), Some("87654321"));
    for bad in ["", "123", "DK123456789", "12-345678", "1234abcd"] {
        assert!(normalize(bad).is_none());
    }
    let mut db = database();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let prepared = prepare(&tx, Some("DK87654321"), false, NOW).unwrap();
    assert!(prepared.needs_lookup);
    let result = finish(
        &tx,
        &prepared,
        Some(&LookupOutcome::Found(entity())),
        "synthetic",
        NOW,
    )
    .unwrap();
    assert_eq!(result["cached"], false);
    assert_eq!(result["fetchedAt"], NOW);
    let saved: (String, String, String) = tx
        .query_row(
            "SELECT raw_response,fetched_at,expires_at FROM cvr_lookups",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(serde_json::from_str::<Value>(&saved.0).unwrap(), entity());
    assert_eq!(saved.2, "2026-01-31T00:00:00.000Z");
    let fresh = prepare(&tx, Some("87654321"), false, "2026-01-02T00:00:00.000Z").unwrap();
    assert!(!fresh.needs_lookup);
    let changes = tx.total_changes();
    let fresh_result = finish(&tx, &fresh, None, "synthetic", "2026-01-02T00:00:00.000Z").unwrap();
    assert_eq!(fresh_result["cached"], true);
    assert_eq!(tx.total_changes(), changes);
    let stale = prepare(&tx, Some("87654321"), false, "2026-03-01T00:00:00.000Z").unwrap();
    assert!(stale.needs_lookup);
    let stale_result = finish(
        &tx,
        &stale,
        Some(&LookupOutcome::Failed("Synthetic unavailable".into())),
        "synthetic",
        "2026-03-01T00:00:00.000Z",
    )
    .unwrap();
    assert_eq!(stale_result, fresh_result);
    assert_eq!(tx.total_changes(), changes);
    let wrong = finish(
        &tx,
        &stale,
        Some(&LookupOutcome::Found(json!({"cvrNummer":12345678}))),
        "synthetic",
        "2026-03-01T00:00:00.000Z",
    )
    .unwrap();
    assert_eq!(wrong["ok"], false);
    assert_eq!(tx.total_changes(), changes);
}
#[test]
fn uncached_failure_and_invalid_input_never_publish_cache() {
    let mut db = database();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let prepared = prepare(&tx, Some("87654321"), false, NOW).unwrap();
    let changes = tx.total_changes();
    let result = finish(
        &tx,
        &prepared,
        Some(&LookupOutcome::Failed(
            "Synthetic credentials missing".into(),
        )),
        "synthetic",
        NOW,
    )
    .unwrap();
    assert_eq!(result["ok"], false);
    assert_eq!(result["cached"], false);
    assert_eq!(tx.total_changes(), changes);
    let invalid = prepare(&tx, Some("not a CVR"), false, NOW).unwrap();
    assert!(!invalid.needs_lookup);
    assert_eq!(
        finish(&tx, &invalid, None, "synthetic", NOW).unwrap()["ok"],
        false
    );
    assert_eq!(tx.total_changes(), changes);
}
#[test]
fn sync_updates_only_registry_fields_audits_repeats_clears_nulls_and_preserves_fiscal_policy() {
    let mut db = database();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("UPDATE companies SET cvr='87654321',address='Old street',postal_code='0000',city='Old city',company_form='Old form',industry_code='000000',industry_text='Old industry',cvr_status='Old status',audit_waived=0,vat_period_type='quarter',payment_terms_days=14 WHERE id=1").unwrap();
    let protected = || {
        tx.query_row::<String,_,_>("SELECT json_array(cvr,country,currency,fiscal_year_start_month,fiscal_year_label_strategy,vat_period_type,payment_terms_days) FROM companies WHERE id=1",[],|r|r.get(0)).unwrap()
    };
    let before = protected();
    let prepared = prepare(&tx, None, true, NOW).unwrap();
    let result = finish(
        &tx,
        &prepared,
        Some(&LookupOutcome::Found(entity())),
        "synthetic",
        NOW,
    )
    .unwrap();
    assert_eq!(
        result["updatedFields"],
        json!([
            "name",
            "address",
            "postalCode",
            "city",
            "companyForm",
            "industryCode",
            "industryText",
            "cvrStatus",
            "auditWaived"
        ])
    );
    assert_eq!(
        result["fiscalYearStartMonth"],
        json!({"current":1,"cvr":7,"matches":false})
    );
    assert_eq!(protected(), before);
    let prepared = prepare(&tx, None, true, NOW).unwrap();
    let repeated = finish(&tx, &prepared, None, "synthetic", NOW).unwrap();
    assert_eq!(repeated["updatedFields"], json!([]));
    assert_eq!(repeated["cached"], true);
    assert_eq!(tx.query_row::<i64,_,_>("SELECT count(*) FROM audit_log WHERE event_type='company_cvr_sync' AND actor='user:synthetic'",[],|r|r.get(0)).unwrap(),2);
    let prepared = prepare(&tx, None, true, "2026-03-01T00:00:00.000Z").unwrap();
    finish(&tx,&prepared,Some(&LookupOutcome::Found(json!({"cvrNummer":87654321,"virksomhedMetadata":{"nyesteNavn":{"navn":"Synthetic Cleared"}}}))),"synthetic","2026-03-01T00:00:00.000Z").unwrap();
    let profile = crate::company_setup::profile(&tx).unwrap();
    for key in [
        "address",
        "postalCode",
        "city",
        "companyForm",
        "industryCode",
        "industryText",
        "cvrStatus",
        "auditWaived",
    ] {
        assert!(profile[key].is_null(), "{key}");
    }
    assert_eq!(protected(), before);
    assert_eq!(
        tx.query_row::<i64, _, _>("SELECT count(*) FROM journal_entries", [], |r| r.get(0))
            .unwrap(),
        0
    );
    let prepared = prepare(&tx, None, true, NOW).unwrap();
    tx.execute("UPDATE companies SET cvr='12345678' WHERE id=1", [])
        .unwrap();
    let changes = tx.total_changes();
    assert!(finish(&tx, &prepared, None, "synthetic", NOW).is_err());
    assert_eq!(tx.total_changes(), changes);
}
