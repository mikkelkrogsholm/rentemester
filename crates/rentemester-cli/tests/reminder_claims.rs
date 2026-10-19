use std::process::Command;
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
#[test]
fn reminder_journey_help_and_invalid_usage_are_explicit() {
    let out = Command::new(BIN)
        .args(["remote", "--help"])
        .output()
        .unwrap();
    let help = String::from_utf8_lossy(&out.stdout);
    for name in [
        "invoice-reminder-claims",
        "register-invoice-reminder",
        "post-invoice-reminder",
        "settle-invoice-claims",
    ] {
        assert!(help.contains(name));
    }
    for args in [
        vec!["invoice-reminder-claims", "--id", "0", "--confirm"],
        vec!["register-invoice-reminder"],
        vec![
            "settle-invoice-claims",
            "--bank-reference",
            "guessed",
            "--confirm",
        ],
    ] {
        let out = Command::new(BIN)
            .args([
                "remote",
                "--server",
                "http://127.0.0.1:9",
                "--allow-loopback-http",
                "--company",
                "synthetic",
            ])
            .args(args)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2));
    }
}
