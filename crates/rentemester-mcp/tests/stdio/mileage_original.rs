use super::*;
#[test]
fn mileage_discovery_nested_input_optional_confirm_and_local_output() {
    let rows = parsed(&run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
            call(
                3,
                "mileage_log",
                json!({"company":"synthetic","input":{"tripDate":"2026-01-02","purpose":"Trip","fromLocation":"A","toLocation":"B","kilometers":1,"vehicle":"Car","driver":"Driver","ratePerKm":2,"rateBasis":"Explicit"}}),
            ),
            call(
                4,
                "mileage_export",
                json!({"company":"synthetic","from":"2026-01-01","to":"2026-01-31","outputDir":"/not-opened","confirm":false}),
            ),
        ],
    ));
    let tools = rows[1]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 333);
    for name in [
        "mileage_log",
        "mileage_list",
        "mileage_report",
        "mileage_export",
        "register_mileage",
        "mileage",
    ] {
        assert!(tools.iter().any(|t| t["name"] == name));
    }
    let log = tools.iter().find(|t| t["name"] == "mileage_log").unwrap();
    assert!(log["inputSchema"]["properties"].get("input").is_some());
    assert!(
        !log["inputSchema"]["required"]
            .as_array()
            .unwrap()
            .contains(&json!("confirm"))
    );
    for row in &rows[2..] {
        assert_eq!(
            row["result"]["structuredContent"]["code"],
            "CONFIRM_REQUIRED"
        );
        assert_eq!(
            row["result"]["content"][0]["text"],
            row["result"]["structuredContent"].to_string()
        );
    }
}
