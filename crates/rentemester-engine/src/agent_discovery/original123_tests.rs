use super::*;

#[test]
fn original123_frozen_identity_and_full_definitions() {
    assert_eq!(about()["catalogue"]["workflowCount"], 31);
    assert_eq!(about()["catalogue"]["capabilityCount"], 26);
    assert_eq!(about()["catalogue"]["hash"], source()["hash"]);
    assert_eq!(about()["fullCoverage"], false);
    assert!(about()["ruleBundleVersion"].is_null());
    for original in source()["workflows"].as_array().unwrap() {
        let v = describe(original["id"].as_str().unwrap(), &[], false).unwrap();
        assert_eq!(v["workflow"]["live"], false);
        for (key, val) in original.as_object().unwrap() {
            if key == "steps" {
                continue;
            }
            assert_eq!(&v["workflow"][key], val, "{} {key}", original["id"]);
        }
        for (before, after) in original["steps"]
            .as_array()
            .unwrap()
            .iter()
            .zip(v["workflow"]["steps"].as_array().unwrap())
        {
            for (key, val) in before.as_object().unwrap() {
                if ["operation", "uncertainOutcomeReadBack"].contains(&key.as_str()) {
                    for (k, x) in val.as_object().unwrap() {
                        assert_eq!(&after[key][k], x);
                    }
                } else {
                    assert_eq!(&after[key], val);
                }
            }
        }
    }
}
#[test]
fn original123_search_bounds_order_and_exhaustion() {
    let all = search("", 0, 50, &[], true).unwrap();
    assert_eq!(all["total"], 26);
    assert_eq!(all["count"], 26);
    for (i, c) in source()["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        assert_eq!(all["items"][i]["id"], c["id"]);
        assert!(all["items"][i].get("keywords").is_none());
        assert!(all["items"][i].get("canonicalState").is_none());
    }
    let exhausted = search("", u64::MAX, 50, &[], true).unwrap();
    assert_eq!(exhausted["count"], 0);
    assert!(exhausted["nextCursor"].is_null());
    assert!(search(&"😀".repeat(100), 0, 1, &[], false).is_ok());
    assert!(search(&"😀".repeat(101), 0, 1, &[], false).is_err());
    assert!(search("", 0, 0, &[], false).is_err());
    assert!(search("", 0, 51, &[], false).is_err());
    assert_eq!(
        search("COMPANY workspace", 0, 50, &[], false).unwrap()["items"][0]["id"],
        "company-workspace"
    );
    assert!(describe("unknown/../😀", &[], true).is_none());
}
#[test]
fn original123_actual_routes_never_alias_parameter_meaning() {
    let reference =
        json!({"surface":"http","method":"GET","pattern":"/api/companies/:slug/workspace-inbox"});
    let wrong = [
        json!({"method":"GET","path":"/api/workspace/inbox"}),
        json!({"method":"GET","path":"/api/companies/{company}/workspace-inbox"}),
    ];
    assert_eq!(fact(&reference, &wrong, None, true)["resolved"], false);
    let actual = [json!({"method":"GET","path":"/api/companies/{slug}/workspace-inbox"})];
    assert_eq!(fact(&reference, &actual, None, true)["resolved"], true);
    assert_eq!(fact(&reference, &actual, None, false)["resolved"], false);
    assert_eq!(
        fact(&json!({"surface":"cli","key":"init"}), &[], None, true)["resolved"],
        false
    );
    assert_eq!(
        fact(
            &json!({"surface":"cli","key":"company list"}),
            &[],
            None,
            true
        )["resolved"],
        true
    );
}
#[test]
fn original123_mcp_actual_annotations_retry_and_single_merge() {
    let tools = [
        json!({"name":"invoice_send_email","annotations":{"readOnlyHint":false,"idempotentHint":true}}),
        json!({"name":"company_profile_get","annotations":{"readOnlyHint":true,"idempotentHint":true}}),
        json!({"name":"company_add"}),
    ];
    let email = fact(
        &json!({"surface":"mcp","name":"invoice_send_email"}),
        &[],
        Some(&tools),
        true,
    );
    assert_eq!(email["retryClass"], "unsafe-read-back");
    assert_eq!(
        fact(
            &json!({"surface":"mcp","name":"company_add"}),
            &[],
            Some(&tools),
            true
        )["resolved"],
        false
    );
    assert!(
        fact(
            &json!({"surface":"mcp","name":"company_profile_get"}),
            &[],
            None,
            true
        )["resolved"]
            .is_null()
    );
    let value = decorate_mcp(search("", 0, 50, &[], true).unwrap(), &tools);
    assert_eq!(decorate_mcp(value.clone(), &tools), value);
    let mut changed = json!({"workflow":{"steps":[{"id":"write","operation":{"surface":"mcp","name":"invoice_send_email"},"expectedSafety":"write","expectedIdempotent":false}]},"workspaceConnected":true});
    resolve_workflow(&mut changed, &[], Some(&tools), true);
    assert_eq!(changed["workflow"]["live"], false);
    assert!(
        !changed["workflow"]["unresolvedOperations"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
#[test]
fn original123_destructive_and_unknown_future_classification() {
    let op =
        json!({"surface":"http","method":"DELETE","pattern":"/api/companies/:slug/documents/:id"});
    let r = [
        json!({"method":"DELETE","path":"/api/companies/{slug}/documents/{id}","effect":"destructive"}),
    ];
    assert_eq!(fact(&op, &r, None, true)["safety"], "destructive");
    let tools =
        [json!({"name":"future_company_accounting_magic","annotations":{"readOnlyHint":true}})];
    assert!(
        !bindings(&[], Some(&tools), true)
            .iter()
            .any(|b| b["id"] == "mcp:future_company_accounting_magic")
    );
}
