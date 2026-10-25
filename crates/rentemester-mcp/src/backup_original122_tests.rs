use crate::backup_original122::{call, tools, wrap};
use rentemester_client::ApiClient;
use serde_json::{Value, json};

fn api() -> ApiClient {
    ApiClient::new("http://127.0.0.1:9", true, Some("session=synthetic".into())).unwrap()
}
fn envelope(result: &Value) -> &Value {
    let structured = &result["structuredContent"];
    assert_eq!(
        serde_json::from_str::<Value>(result["content"][0]["text"].as_str().unwrap()).unwrap(),
        *structured
    );
    assert_eq!(result["isError"], structured["ok"] != true);
    structured
}
#[test]
fn backup_original122_original_catalogue_flat_schema_and_annotations() {
    let tools = tools();
    assert_eq!(tools.len(), 2);
    for (t, lock) in tools.iter().zip([false, true]) {
        assert_eq!(t["inputSchema"]["required"], json!(["company"]));
        assert_eq!(t["inputSchema"]["additionalProperties"], false);
        assert_eq!(
            t["annotations"],
            json!({"readOnlyHint":!lock,"destructiveHint":false,"idempotentHint":!lock,"openWorldHint":false})
        );
        assert!(t["inputSchema"]["properties"].get("body").is_none());
        assert!(t["inputSchema"]["properties"].get("actor").is_none());
        assert_eq!(
            t["outputSchema"],
            serde_json::from_str::<Value>(crate::diagnostics_original118::OUTPUT_SCHEMA).unwrap()
        );
    }
    assert_eq!(tools[0]["name"], "system_backup_status");
    assert_eq!(tools[1]["name"], "system_backup_lock");
    assert_eq!(
        tools[1]["inputSchema"]["properties"]["graceDays"]["type"],
        "number"
    );
    assert_eq!(
        tools[1]["inputSchema"]["properties"]["confirm"]["type"],
        "boolean"
    );
    let listed = crate::tools::list();
    for name in [
        "backup_status",
        "backup_lock",
        "system_backup_status",
        "system_backup_lock",
    ] {
        assert_eq!(
            listed["tools"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|t| t["name"] == name)
                .count(),
            1
        );
    }
}
#[tokio::test]
async fn backup_original122_missing_confirm_is_source_envelope_without_request() {
    for args in [
        json!({"company":"synthetic"}),
        json!({"company":"synthetic","confirm":false,"at":"bad","graceDays":-1.5}),
    ] {
        let result = call(&api(), "system_backup_lock", args).await.unwrap();
        assert_eq!(
            envelope(&result),
            &json!({"ok":false,"errors":["confirm: true required for write tool system_backup_lock"],"code":"CONFIRM_REQUIRED"})
        );
    }
}
#[tokio::test]
async fn backup_original122_structural_args_are_rpc_invalid_params() {
    for (name, args) in [
        (
            "system_backup_status",
            json!({"company":"synthetic","asOf":1}),
        ),
        (
            "system_backup_status",
            json!({"company":"synthetic","confirm":true}),
        ),
        (
            "system_backup_lock",
            json!({"company":"synthetic","graceDays":"3"}),
        ),
        (
            "system_backup_lock",
            json!({"company":"synthetic","confirm":"true"}),
        ),
        (
            "system_backup_lock",
            json!({"company":"synthetic","actor":"forged"}),
        ),
        ("system_backup_lock", json!({"company":""})),
        (
            "system_backup_lock",
            json!({"company":"synthetic","body":"{}"}),
        ),
    ] {
        assert!(call(&api(), name, args.clone()).await.is_err());
        let mut session = crate::Session {
            initialized: true,
            ready: true,
            ..Default::default()
        };
        let response = session.handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":args}}),&api()).await.unwrap();
        assert_eq!(response["error"]["code"], -32602);
    }
}
#[test]
fn backup_original122_wrap_removes_core_fields_and_omits_false_data() {
    for ok in [true, false] {
        let core = json!({"ok":ok,"errors":if ok {json!([])} else {json!(["invalid date"])},"appliedRules":["DK-BOOKKEEPING-BACKUP-001"],"checkedAt":"2026-01-01T00:00:00.000Z","backupDue":false});
        let result = wrap(core);
        let e = envelope(&result);
        assert_eq!(e["appliedRules"], json!(["DK-BOOKKEEPING-BACKUP-001"]));
        if ok {
            assert_eq!(
                e["data"],
                json!({"checkedAt":"2026-01-01T00:00:00.000Z","backupDue":false})
            );
        } else {
            assert!(e.get("data").is_none());
        }
    }
}
