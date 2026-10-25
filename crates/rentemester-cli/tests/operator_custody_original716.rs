//! Focused source parse refusals; no live workspace or privileged configuration.
use std::process::Command;
fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_rentemester-rust"))
        .args(args)
        .output()
        .unwrap()
}
#[test]
fn original716_custody_verify_requires_backup_and_rotation_requires_reason() {
    let output = run(&["system", "verify-backup-signature"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Missing required --backup-dir <dir>")
    );
    let output = run(&[
        "system",
        "rotate-backup-keypair",
        "--company",
        "synthetic",
        "--reason",
        " ",
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Missing required --reason"));
}
#[test]
fn original716_custody_does_not_turn_remote_paths_into_authority() {
    let output = run(&[
        "system",
        "verify-backup-signature",
        "--backup-dir",
        "/synthetic/not-live",
        "--server",
        "https://example.invalid",
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("local-profile"));
}
