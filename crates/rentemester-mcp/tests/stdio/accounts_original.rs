use super::*;
#[test]
fn accounts_original_four_unique_tools_and_offline_confirmation() {
    let out = run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
            call(
                3,
                "accounts_add",
                json!({"company":"synthetic","input":{"accountNo":"1790","name":"Synthetic bank","type":"asset"}}),
            ),
            call(
                4,
                "accounts_role_confirm",
                json!({"company":"synthetic","role":"bank","accountNo":"1790","confirm":false}),
            ),
        ],
    );
    let rows = parsed(&out);
    let tools = rows[1]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 370);
    let names: std::collections::BTreeSet<_> =
        tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(names.len(), 370);
    for (name, read) in [
        ("accounts_list", true),
        ("accounts_add", false),
        ("accounts_roles_status", true),
        ("accounts_role_confirm", false),
    ] {
        let t = tools.iter().find(|t| t["name"] == name).unwrap();
        assert_eq!(t["annotations"]["readOnlyHint"], read);
        assert_eq!(t["annotations"]["idempotentHint"], read);
        assert_eq!(t["annotations"]["destructiveHint"], false);
        assert!(
            t["inputSchema"]["required"]
                .as_array()
                .unwrap()
                .contains(&json!("company"))
        );
        assert!(t["inputSchema"]["properties"].get("actor").is_none());
    }
    for row in &rows[2..] {
        assert_eq!(row["result"]["isError"], true);
        assert_eq!(
            row["result"]["structuredContent"]["code"],
            "CONFIRM_REQUIRED"
        );
    }
}
