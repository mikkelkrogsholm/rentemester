use super::*;
#[test]
fn privacy_wire_refuses_forged_attribution_dates_missing_confirmation_and_unknown_fields() {
    for (operation, body) in [
        (
            "discover",
            json!({"cvr":"SYN","asOf":"2026-01-01","confirm":true}),
        ),
        (
            "forget",
            json!({"cvr":"SYN","afterRetentionExpiry":true,"confirm":true}),
        ),
        (
            "legal-hold",
            json!({"name":"Synthetic","holdId":"case-1","operation":"place","reason":"Synthetic review","confirm":true}),
        ),
        ("audit-log", json!({"since":"2026-01-01","confirm":true})),
    ] {
        assert!(valid_input(operation, &body));
        for key in ["actor", "companySlug", "principalId", "unknown"] {
            let mut bad = body.clone();
            bad[key] = json!("forged");
            assert!(!valid_input(operation, &bad));
        }
        let mut missing = body.clone();
        missing.as_object_mut().unwrap().remove("confirm");
        assert!(!valid_input(operation, &missing));
        let mut no = body.clone();
        no["confirm"] = json!(false);
        assert!(!valid_input(operation, &no));
    }
    for operation in ["erase", "forget"] {
        let mut body = json!({"cvr":"SYN","confirm":true});
        if operation == "forget" {
            body["afterRetentionExpiry"] = json!(true)
        }
        for date in [json!(null), json!("2099-01-01")] {
            body["asOf"] = date;
            assert!(!valid_input(operation, &body));
        }
    }
}
