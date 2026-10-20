//! Offline discovery uses the same registry as tools/list, without a session.
use serde_json::Value;
use std::collections::BTreeSet;
use std::process::{Command, Output, Stdio};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rentemester-mcp"))
        .args(args)
        .env_clear()
        .env("HOME", "/rentemester-test-no-home")
        .current_dir(std::env::temp_dir())
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

#[test]
fn catalogue_is_deterministic_without_credentials_workspace_or_server() {
    let output = run(&["--catalogue"]);
    assert!(output.status.success(), "{:?}", output);
    assert!(output.stderr.is_empty());
    assert_eq!(output.stdout, run(&["--catalogue"]).stdout);
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["format"], "rentemester-native-mcp-catalogue/1");
    assert_eq!(value["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(value["protocolVersion"], "2025-11-25");
    assert_eq!(value.as_object().unwrap().len(), 4);
    let tools = value["tools"].as_array().unwrap();
    let mut names = BTreeSet::new();
    for tool in tools {
        assert!(names.insert(tool["name"].as_str().unwrap()));
        assert_eq!(tool["inputSchema"]["type"], "object");
        assert!(tool["annotations"]["readOnlyHint"].is_boolean());
    }
    for required in [
        "me",
        "companies",
        "post_journal",
        "workspace_transfer_status",
        "invoice_email_send",
    ] {
        assert!(names.contains(required), "missing actual tool {required}");
    }
}

#[test]
fn mixed_catalogue_refuses_before_credentials_even_after_help() {
    for args in [
        vec!["--help", "--catalogue"],
        vec!["--catalogue", "--catalogue"],
        vec!["--server", "https://invalid.example", "--catalogue"],
        vec!["--cookie-file", "/missing/private-cookie", "--catalogue"],
        vec!["--catalogue", "--service-key-file", "/missing/private-key"],
    ] {
        let output = run(&args);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert_eq!(
            String::from_utf8_lossy(&output.stderr).trim(),
            "--catalogue must be the only argument"
        );
    }
}
