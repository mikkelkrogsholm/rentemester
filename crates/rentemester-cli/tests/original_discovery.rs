use serde_json::Value;
use std::process::{Command, Output, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
fn run(args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .env("RENTEMESTER_COMPANY", "../../poisoned-company")
        .env("RENTEMESTER_WORKSPACE", "/missing/discovery-workspace")
        .env("RENTEMESTER_LOCAL_PROFILE", "/missing/discovery-profile")
        .stdin(Stdio::null())
        .output()
        .unwrap()
}
fn success(args: &[&str]) -> Value {
    let output = run(args);
    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    serde_json::from_slice(&output.stdout).unwrap()
}
#[test]
fn root_discovery_aliases_preserve_json_catalogue() {
    let expected = success(&["--help"]);
    assert_eq!(expected["ok"], true);
    assert!(expected["commands"].is_array());
    assert_eq!(success(&[]), expected);
    assert_eq!(success(&["help"]), expected);
}
#[test]
fn native_version_is_the_actual_package_version() {
    let output = run(&["--version"]);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!("rentemester-rust {}\n", env!("CARGO_PKG_VERSION"))
    );
}
#[test]
fn help_does_not_load_explicit_profile_credentials_or_workspace() {
    let expected = success(&["--help"]);
    for args in [
        vec![
            "help",
            "--local-profile=/missing/discovery-profile",
            "--cookie-file=-",
        ],
        vec![
            "--local-profile",
            "/missing/discovery-profile",
            "--help",
            "--service-key-file",
            "/missing/discovery-key",
        ],
        vec![
            "--help",
            "--server=https://example.invalid",
            "--cookie-file=/missing/discovery-cookie",
            "--workspace=/missing/discovery-workspace",
        ],
    ] {
        assert_eq!(success(&args), expected);
    }
}
#[test]
fn root_usage_errors_are_stderr_only_before_loading() {
    for args in [
        vec!["unknown-command"],
        vec!["--help", "--local-profile"],
        vec!["--help", "--local-profile="],
        vec!["--help", "--local-profile=relative"],
        vec![
            "--help",
            "--local-profile=/missing/profile",
            "--server=https://example.invalid",
        ],
        vec![
            "help",
            "--local-profile=/missing/profile",
            "--allow-loopback-http",
        ],
        vec![
            "--help",
            "--cookie-file=/missing/cookie",
            "--service-key-file=/missing/key",
        ],
        vec!["--help", "--server"],
        vec!["--help=false"],
        vec!["--version", "--help"],
    ] {
        let output = run(&args);
        assert_eq!(output.status.code(), Some(2), "{args:?}: {output:?}");
        assert!(output.stdout.is_empty(), "{output:?}");
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(error.contains("--help"), "{error}");
        assert!(!error.contains("expected trial-balance"), "{error}");
        assert!(!error.contains("Could not open"), "{error}");
    }
}
#[test]
fn catalogue_includes_implemented_families_and_retains_scope_boundaries() {
    let help = success(&["--help"]);
    let commands = help["commands"].as_array().unwrap();
    let names = commands
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect::<Vec<_>>()
        .join("|");
    for name in [
        "invoice apply-payment",
        "invoice repair-posting",
        "recurring-invoice create|generate|list|run-workspace",
        "remote",
        "validate-journal",
        "trial-balance",
    ] {
        assert!(names.contains(name), "{names}");
    }
    let binding = help["localBinding"].as_str().unwrap();
    assert!(binding.contains("RENTEMESTER_COMPANY"));
    assert!(binding.contains("default launcher is Rust"));
    assert!(binding.contains("RENTEMESTER_LEGACY_CLI=1"));
    let registry = &help["currentCli"]["commands"];
    assert_eq!(registry.as_array().unwrap().len(), 265);
    let keys: std::collections::BTreeSet<_> = registry.as_array().unwrap().iter()
        .map(|c| c["key"].as_str().unwrap()).collect();
    assert_eq!(keys.len(), 265);
    assert!(registry.as_array().unwrap().iter().all(|c| c["help"].is_string()));
    assert!(binding.contains("Whole-product parity is unfinished"));
    assert!(!binding.contains("environment-default or launcher parity"));
}
#[test]
fn existing_command_help_passes_through() {
    let root = success(&["--help"]);
    for args in [
        vec!["invoice", "apply-payment", "--help"],
        vec!["invoice", "repair-posting", "--help"],
        vec!["recurring-invoice", "list", "--help"],
        vec!["recurring-invoice", "run-workspace", "--help"],
        vec!["remote", "--help"],
        vec!["report", "--help"],
        vec!["trial-balance", "--help"],
        vec!["documents", "--help"],
    ] {
        let help = success(&args);
        assert_ne!(help, root, "{args:?}");
    }
}
