//! Fresh synthetic CLI parser/stream cases. The runner wires the delivered module.
use std::process::Command;
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
fn call(args: &[&str]) -> std::process::Output {
    Command::new(BIN)
        .args(args)
        .env_remove("RENTEMESTER_COMPANY")
        .env_remove("RENTEMESTER_WORKSPACE")
        .output()
        .unwrap()
}
#[test]
fn original716_schema_usage_before_any_database_access() {
    for args in [
        vec![
            "system",
            "migrate",
            "--apply",
            "YES",
            "--company",
            "/synthetic/not-present",
        ],
        vec![
            "system",
            "repair-schema-views",
            "--apply",
            "yes",
            "--reason",
            " ",
            "--company",
            "/synthetic/not-present",
        ],
        vec!["system", "migrate", "--sql", "DROP TABLE companies"],
    ] {
        let output = call(&args);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
    }
}
#[test]
fn original716_schema_help_is_positive_without_credentials() {
    for command in ["migrate", "repair-schema-views"] {
        let output = call(&["system", command, "--help"]);
        assert_eq!(output.status.code(), Some(0));
        assert!(output.stderr.is_empty());
        assert!(String::from_utf8_lossy(&output.stdout).contains("--apply yes"));
    }
}
#[test]
fn original716_schema_unknown_source_is_not_reported_current() {
    let output = call(&[
        "system",
        "migrate",
        "--company",
        "/synthetic/missing-schema-original716",
        "--json",
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["ok"], false);
    assert!(value["errors"].as_array().is_some_and(|a| !a.is_empty()));
}
