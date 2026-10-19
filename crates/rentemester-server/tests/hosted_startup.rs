//! Operator protocol failures must occur before stdin or workspace admission.
use std::{
    io::Write,
    process::{Command, Stdio},
};
#[test]
fn hosted_startup_is_paired_explicit_and_never_echoes_stdin() {
    let binary = env!("CARGO_BIN_EXE_rentemester-server");
    let secret = "synthetic-operator-input-never-echo";
    for args in [
        vec!["--scheduled-worker"],
        vec!["--scheduled-service-key-stdin"],
        vec!["--scheduled-worker", "--scheduled-service-key-stdin"],
        vec![
            "--scheduled-worker",
            "--scheduled-service-key-stdin",
            "--config",
            "missing-private-config.json",
            "--create",
        ],
        vec![
            "--scheduled-worker",
            "--scheduled-service-key-stdin",
            "--config",
            "missing-private-config.json",
            "--verify-backup",
            "missing-archive",
        ],
        vec![
            "--scheduled-worker",
            "--scheduled-service-key-stdin",
            "--config",
            "missing-private-config.json",
            "--restore-backup",
            "missing-archive",
        ],
    ] {
        let mut child = Command::new(binary)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        if let Some(mut input) = child.stdin.take() {
            let _ = input.write_all(secret.as_bytes());
        }
        let output = child.wait_with_output().unwrap();
        assert!(!output.status.success());
        let log = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(log.contains("scheduled worker requires paired"));
        assert!(!log.contains(secret));
    }
    let help = Command::new(binary).arg("--help").output().unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("--scheduled-service-key-stdin"));
}
