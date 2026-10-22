use std::process::Command;
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
#[test]
fn help_is_pure_and_discovers_all_six_treatments() {
    for args in [
        vec!["expense", "--help"],
        vec![
            "expense",
            "book",
            "--help",
            "--cookie-file",
            "/not/opened",
            "--local-profile",
            "/not/opened",
        ],
        vec!["expense", "vat-preflight", "--help"],
    ] {
        let output = Command::new(BIN).args(args).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains("eu_goods_acquisition"));
        assert!(text.contains("--apply yes"));
        assert!(text.contains("current credential identity"));
    }
}
#[test]
fn invalid_usage_is_before_credentials_and_no_cli_caller_key() {
    for tail in [
        vec!["vat-preflight", "--document-id", "3", "--apply", "true"],
        vec![
            "book",
            "--document-id",
            "3",
            "--bank-transaction-id",
            "7",
            "--expense-account",
            "synthetic",
            "--idempotency-key",
            "forbidden",
        ],
        vec![
            "book",
            "--document-id",
            "9007199254740992",
            "--bank-transaction-id",
            "7",
            "--expense-account",
            "synthetic",
        ],
        vec!["vat-preflight", "--document-id", "3", "--actor", "forged"],
    ] {
        let mut args = vec!["expense"];
        args.extend(tail);
        args.extend([
            "--server",
            "http://127.0.0.1:9",
            "--allow-loopback-http",
            "--cookie-file",
            "/must-not-be-opened",
            "--company",
            "synthetic",
        ]);
        let output = Command::new(BIN).args(args).output().unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(!String::from_utf8_lossy(&output.stderr).contains("Could not read"));
    }
}
