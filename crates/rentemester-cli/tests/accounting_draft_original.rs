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
fn seven_offline_help_and_preflight_before_credentials() {
    for command in [
        "create",
        "revise",
        "submit",
        "reject",
        "approve-and-post",
        "show",
        "list",
    ] {
        let o = cli(&[
            "--cookie-file=/missing/credential",
            "--local-profile=/missing/profile",
            "accounting-draft",
            command,
            "--help",
        ]);
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        assert!(String::from_utf8_lossy(&o.stdout).contains("accounting-draft create"));
    }
    for flags in [
        vec!["create", "--draft-id=draft", "--input=/missing/payload"],
        vec!["revise", "--draft-id=draft", "--confirm=yes"],
        vec![
            "submit",
            "--draft-id=draft",
            "--expected-event-hash=bad",
            "--confirm=yes",
        ],
        vec!["reject", "--draft-id=draft", "--confirm=yes", "--reason= "],
        vec!["approve-and-post", "--draft-id=draft"],
        vec!["show", "--draft-id=draft", "--actor=user:forged"],
        vec!["list", "--input={}"],
        vec!["show", "--draft-id=Bad"],
        vec!["show", "--draft-id=draft", "--draft-id=other"],
        vec!["list", "--expected-event-hash=bad"],
    ] {
        let mut a = vec!["accounting-draft"];
        a.extend(flags);
        a.extend([
            "--company=synthetic",
            "--server=http://127.0.0.1:9",
            "--cookie-file=/missing/credential",
        ]);
        let o = cli(&a);
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
#[test]
fn input_is_local_unique_bounded_json_file() {
    let path = std::env::temp_dir().join(format!(
        "rentemester-draft-input-{}.json",
        std::process::id()
    ));
    for bytes in [
        b"[]".to_vec(),
        br#"{"nested":{"a":1,"a":2}}"#.to_vec(),
        b"not JSON".to_vec(),
        vec![b' '; 262145],
    ] {
        std::fs::write(&path, bytes).unwrap();
        let o = cli(&[
            "accounting-draft",
            "create",
            "--draft-id=draft",
            "--confirm=yes",
            "--input",
            path.to_str().unwrap(),
            "--company=synthetic",
            "--server=http://127.0.0.1:9",
            "--cookie-file=/missing/credential",
        ]);
        assert_eq!(o.status.code(), Some(2));
        assert!(o.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&o.stderr).contains("credential"));
    }
    std::fs::remove_file(path).unwrap();
    let o = cli(&[
        "accounting-draft",
        "create",
        "--draft-id=draft",
        "--confirm=yes",
        "--input={}",
        "--company=synthetic",
        "--server=http://127.0.0.1:9",
        "--cookie-file=/missing/credential",
    ]);
    assert_eq!(o.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&o.stderr).contains("local draft JSON file"));
}

#[test]
fn duplicate_global_flags_are_usage_before_credentials() {
    for flags in [["--json", "--json"], ["--format=json", "--format=human"]] {
        let mut args = vec!["accounting-draft", "list"];
        args.extend(flags);
        args.push("--cookie-file=/missing/credential");
        let o = cli(&args);
        assert_eq!(o.status.code(), Some(2));
        assert!(o.stdout.is_empty());
        assert!(String::from_utf8_lossy(&o.stderr).contains("Duplicate"));
    }
}
