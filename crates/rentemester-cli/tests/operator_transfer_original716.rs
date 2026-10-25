//! Source syntax and one-shot refusal contracts. Synthetic inputs only.
use std::process::Command;
fn call(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_rentemester-rust"))
        .args(args)
        .output()
        .unwrap()
}
#[test]
fn source_transfer_help_does_not_open_runtime() {
    for verb in ["snapshot", "restore"] {
        let o = call(&["workspace", verb, "--help"]);
        assert_eq!(
            o.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&o.stderr)
        );
        assert!(String::from_utf8_lossy(&o.stdout).contains("--config"));
        assert!(String::from_utf8_lossy(&o.stdout).contains("--server-bin"));
    }
}
#[test]
fn source_transfer_confirmation_is_business_refusal() {
    let o = call(&[
        "workspace",
        "snapshot",
        "--workspace=/synthetic",
        "--out=/synthetic.tar",
    ]);
    assert_eq!(o.status.code(), Some(1));
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["ok"], false);
    assert_eq!(v["errors"][0], "--confirm yes required");
}
#[test]
fn source_transfer_unknown_duplicate_missing_and_no_implicit_engine() {
    for a in [
        vec!["workspace", "snapshot", "--unknown"],
        vec!["workspace", "snapshot", "--json", "--json"],
        vec!["workspace", "snapshot", "--confirm=yes"],
        vec![
            "workspace",
            "restore",
            "--confirm=yes",
            "--snapshot=/synthetic.tar",
            "--target=/synthetic",
        ],
    ] {
        let o = call(&a);
        assert_eq!(
            o.status.code(),
            Some(2),
            "{}",
            String::from_utf8_lossy(&o.stderr)
        );
        assert!(o.stdout.is_empty());
    }
}
