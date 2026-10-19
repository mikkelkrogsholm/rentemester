use serde_json::{Value, json};
use std::process::Command;
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
#[test]
fn cfo_discovery_and_mutating_or_mixed_selector_refusals() {
    let out = Command::new(BIN)
        .args(["remote", "--help"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        v["commands"]
            .as_array()
            .unwrap()
            .contains(&json!("cfo-analytics"))
    );
    assert_eq!(
        v["cfoAnalytics"]["querySchema"]["additionalProperties"],
        false
    );
    for args in [
        vec!["--confirm"],
        vec!["--company", "left", "--company-slugs-json", "[\"right\"]"],
        vec!["--actor", "agent:test"],
        vec!["--body", "{}"],
    ] {
        let out = Command::new(BIN)
            .args(["remote", "cfo-analytics"])
            .args(args)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2));
    }
}
