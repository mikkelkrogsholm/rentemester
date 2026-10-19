use serde_json::{Value, json};
use std::process::Command;
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
#[test]
fn contact_import_discovery_requires_explicit_company_confirmation_and_exact_body() {
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
            .contains(&json!("import-contacts"))
    );
    assert_eq!(
        v["contactImport"]["bodySchema"]["additionalProperties"],
        false
    );
    for args in [
        vec!["import-contacts"],
        vec!["import-contacts", "--confirm", "--id", "1"],
        vec!["import-contacts", "--confirm", "--enrich-cvr", "true"],
    ] {
        let out = Command::new(BIN).arg("remote").args(args).output().unwrap();
        assert_eq!(out.status.code(), Some(2));
    }
}
