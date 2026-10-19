use serde_json::{Value, json};
use std::process::Command;
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
#[test]
fn correction_discovery_and_confirmation_contract() {
    let out = Command::new(BIN)
        .args(["remote", "--help"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    for name in [
        "direct-bank-payable-correction-plan",
        "direct-bank-payable-correction",
    ] {
        assert!(v["commands"].as_array().unwrap().contains(&json!(name)));
    }
    assert!(
        v["directBankPayableCorrection"]["applySchema"]["required"]
            .as_array()
            .unwrap()
            .contains(&json!("idempotencyKey"))
    );
    for args in [
        vec!["direct-bank-payable-correction"],
        vec!["direct-bank-payable-correction-plan", "--confirm"],
        vec!["direct-bank-payable-correction", "--confirm", "--id", "1"],
    ] {
        let out = Command::new(BIN).arg("remote").args(args).output().unwrap();
        assert_eq!(out.status.code(), Some(2));
    }
}
