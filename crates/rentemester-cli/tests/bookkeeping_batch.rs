use serde_json::{Value, json};
use std::process::Command;
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
#[test]
fn batch_discovery_confirmation_and_command_scope() {
    let out = Command::new(BIN)
        .args(["remote", "--help"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    for name in [
        "bookkeeping-batch-plan",
        "bookkeeping-batch-status",
        "bookkeeping-batch-persist",
        "bookkeeping-batch-approve",
        "bookkeeping-batch-apply",
    ] {
        assert!(v["commands"].as_array().unwrap().contains(&json!(name)));
    }
    assert!(
        v["bookkeepingBatch"]["bodySchemas"]["bookkeeping-batch-persist"]["required"]
            .as_array()
            .unwrap()
            .contains(&json!("expectedPlanHash"))
    );
    for args in [
        vec!["bookkeeping-batch-apply"],
        vec!["bookkeeping-batch-plan", "--confirm"],
        vec!["bookkeeping-batch-status", "--confirm"],
        vec!["bookkeeping-batch-apply", "--confirm", "--id", "7"],
        vec![
            "bookkeeping-batch-persist",
            "--confirm",
            "--bank-from",
            "2026-01-01",
        ],
    ] {
        let out = Command::new(BIN).arg("remote").args(args).output().unwrap();
        assert_eq!(out.status.code(), Some(2));
    }
}
