use serde_json::{Value, json};
use std::process::Command;
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
#[test]
fn disposition_discovery_and_global_read_vs_confirmed_action_scope() {
    let out = Command::new(BIN)
        .args(["remote", "--help"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    for name in [
        "group-disposition-status",
        "plan-group-disposition",
        "propose-group-disposition",
        "approve-group-disposition",
        "link-group-disposition",
        "settle-group-disposition",
        "supersede-group-disposition",
        "reopen-group-disposition",
    ] {
        assert!(v["commands"].as_array().unwrap().contains(&json!(name)));
    }
    assert!(v["intercompanyDispositions"]["bodySchemas"]["link-group-disposition"]["properties"]["ledgerHeadHash"].is_object());
    for args in [
        vec!["group-disposition-status", "--company", "synthetic"],
        vec!["group-disposition-status", "--id", "synthetic", "--confirm"],
        vec!["plan-group-disposition", "--confirm"],
        vec!["approve-group-disposition"],
        vec!["link-group-disposition", "--confirm", "--id", "3"],
    ] {
        let out = Command::new(BIN).arg("remote").args(args).output().unwrap();
        assert_eq!(out.status.code(), Some(2));
    }
}
