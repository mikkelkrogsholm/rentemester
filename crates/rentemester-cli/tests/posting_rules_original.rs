use std::process::Command;
fn cli(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_rentemester-rust"))
        .args(args)
        .env_remove("RENTEMESTER_COMPANY")
        .output()
        .unwrap()
}
#[test]
fn six_help_and_usage_before_credentials() {
    for op in [
        "propose",
        "approve",
        "disable",
        "supersede",
        "explain",
        "test",
    ] {
        let o = cli(&[
            "--company",
            "synthetic",
            "posting-rules",
            op,
            "--cookie-file",
            "/does/not/exist",
            "--help",
        ]);
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    }
    for args in [
        vec!["propose", "--input", "{}"],
        vec!["explain", "--context", r#"{"amount":1,"amount":2}"#],
        vec!["test", "--context", "{}", "--actor", "user:forged"],
        vec!["explain", "--context", "{}", "--input", "{}"],
        vec!["test", "--context", "@file.json"],
    ] {
        let mut full = vec!["posting-rules"];
        full.extend(args);
        full.extend(["--cookie-file", "/does/not/exist"]);
        let o = cli(&full);
        assert_eq!(
            o.status.code(),
            Some(2),
            "{}",
            String::from_utf8_lossy(&o.stderr)
        );
        assert!(!String::from_utf8_lossy(&o.stderr).contains("credential"));
    }
}
