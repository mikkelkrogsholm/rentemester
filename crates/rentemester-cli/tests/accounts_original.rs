use std::process::Command;
fn cli(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_rentemester-rust"))
        .args(args)
        .env_remove("RENTEMESTER_COMPANY")
        .env_remove("RENTEMESTER_WORKSPACE")
        .output()
        .unwrap()
}
#[test]
fn accounts_original_help_is_offline_and_usage_precedes_credentials() {
    for command in ["list", "add", "roles-status", "role-confirm"] {
        let o = cli(&[
            "--company=synthetic",
            "accounts",
            command,
            "--cookie-file=/does/not/exist",
            "--help",
        ]);
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        assert!(String::from_utf8_lossy(&o.stdout).contains("accounts list"));
    }
    for flags in [
        vec!["add", "--account-no=1790", "--name=Bank", "--type=asset"],
        vec![
            "add",
            "--account-no=1790",
            "--name=Bank",
            "--type=asset",
            "--confirm=yes",
            "--allow-direct-posting=0",
        ],
        vec![
            "role-confirm",
            "--role=fx_gain",
            "--account=1790",
            "--confirm=yes",
        ],
        vec!["list", "--actor=user:forged"],
        vec!["list", "--input={}"],
        vec!["roles-status", "--role=bank"],
    ] {
        let mut args = vec!["accounts"];
        args.extend(flags);
        args.extend([
            "--company=synthetic",
            "--server=http://127.0.0.1:9",
            "--cookie-file=/does/not/exist",
        ]);
        let o = cli(&args);
        assert_eq!(
            o.status.code(),
            Some(2),
            "{}",
            String::from_utf8_lossy(&o.stderr)
        );
        assert!(o.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&o.stderr).contains("credential"));
    }
}
