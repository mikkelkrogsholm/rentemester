//! Four source onboarding command boundary checks. Fresh synthetic inputs only.
use std::process::Command;
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
fn call(args: &[&str]) -> std::process::Output {
    Command::new(BIN)
        .args(args)
        .env_remove("RENTEMESTER_WORKSPACE")
        .env_remove("RENTEMESTER_COMPANY")
        .env("RENTEMESTER_SERVER_BIN", "/synthetic/no-server-binary")
        .output()
        .unwrap()
}
#[test]
fn original716_four_help_contracts_are_available_without_runtime() {
    for command in [
        vec!["init"],
        vec!["serve"],
        vec!["local", "start"],
        vec!["workspace-access", "bootstrap-first"],
    ] {
        let mut a = command.clone();
        a.push("--help");
        let o = call(&a);
        assert_eq!(
            o.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&o.stderr)
        );
        assert!(String::from_utf8_lossy(&o.stdout).contains("--workspace"));
        assert!(o.stderr.is_empty());
    }
}
#[test]
fn original716_invalid_inputs_exit_two_before_native_launch() {
    for a in [
        vec!["serve", "--port=0"],
        vec!["init", "--no-vat", "--vat-period=quarter"],
        vec!["local", "start"],
        vec!["serve", "--format=xml"],
    ] {
        let o = call(&a);
        assert_eq!(o.status.code(), Some(2));
        assert!(o.stdout.is_empty());
        assert!(!o.stderr.is_empty());
        assert!(!String::from_utf8_lossy(&o.stderr).contains("could not be launched"));
    }
}
#[test]
fn original716_bootstrap_cancel_is_business_refusal_without_password_read() {
    let o = call(&[
        "workspace-access",
        "bootstrap-first",
        "--workspace=/synthetic/missing",
        "--company=synthetic",
        "--name=Synthetic",
        "--email=synthetic@example.invalid",
        "--password-file=/synthetic/no-password",
    ]);
    assert_eq!(o.status.code(), Some(1));
    assert!(o.stderr.is_empty());
    let result: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(result["ok"], false);
    assert!(!String::from_utf8_lossy(&o.stdout).contains("no-password"));
}
#[test]
fn original716_missing_packaged_server_has_no_toolchain_fallback() {
    let o = call(&[
        "init",
        "--workspace=/synthetic/fresh",
        "--company=synthetic",
        "--actor=user:synthetic",
    ]);
    assert_eq!(o.status.code(), Some(2));
    assert!(o.stdout.is_empty());
    assert!(String::from_utf8_lossy(&o.stderr).contains("no fallback"));
}
