use super::*;
#[test]
fn original_diagnostics118_http_exact_date_grammar_and_selector_refusals() {
    assert_eq!(date_query(None, false), Ok(None));
    for (query, date) in [
        ("asOf=", ""),
        ("asOf=2026-02-30", "2026-02-30"),
        ("asOf=not+a+date", "not a date"),
        ("asOf=2035-01-01", "2035-01-01"),
    ] {
        assert_eq!(date_query(Some(query), false), Ok(Some(date.into())));
    }
    for query in [
        "asOf=a&asOf=b",
        "company=b",
        "actor=user%3Au",
        "asOf=x&unknown=y",
    ] {
        assert!(date_query(Some(query), false).is_err());
    }
    assert_eq!(date_query(None, true), Ok(None));
    for query in ["", "asOf=2026-01-01", "company=b"] {
        assert!(date_query(Some(query), true).is_err());
    }
}
#[test]
fn original_diagnostics118_http_failed_health_data_is_not_transport_failure() {
    let core = json!({"ok":false,"checks":[{"name":"ledger","ok":false}],"missing":["ledger"],"schema_outdated":false,"errors":["missing: ledger"]});
    let value = auth_outcome(rentemester_host::Attempt {
        outcome: Ok(core.clone()),
        retired: false,
    })
    .unwrap();
    assert_eq!(value, core);
    assert!(
        auth_outcome(rentemester_host::Attempt {
            outcome: Ok(core),
            retired: true
        })
        .is_err()
    );
}
