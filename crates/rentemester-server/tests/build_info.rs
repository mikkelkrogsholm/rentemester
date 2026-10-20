//! Artifact identity discovery must precede workspace admission and listener binding.
use serde_json::{Value, json};
use std::process::{Command, Output, Stdio};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rentemester-server"))
        .args(args)
        .env_clear()
        .env("HOME", "/rentemester-test-no-home")
        .current_dir(std::env::temp_dir())
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

#[test]
fn build_info_needs_no_environment_configuration_or_workspace() {
    let output = run(&["--build-info"]);
    assert!(output.status.success(), "{:?}", output);
    assert!(output.stderr.is_empty());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    let policy = rentemester_runtime::package_update::current_policy();
    assert_eq!(
        value,
        json!({
            "format": "rentemester-native-server/1",
            "version": env!("CARGO_PKG_VERSION"),
            "platform": format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
            "apiEpoch": policy.api_epoch,
            "controlSchema": policy.control_schema,
            "companySchema": policy.company_schema,
        })
    );
    assert_eq!(output.stdout, run(&["--build-info"]).stdout);
    let mut control = rusqlite::Connection::open_in_memory().unwrap();
    let mut company = rusqlite::Connection::open_in_memory().unwrap();
    let initialized = rentemester_storage::fresh::initialize_empty_pair(
        &mut control,
        &mut company,
        "Synthetic build identity",
    )
    .unwrap();
    assert_eq!(value["controlSchema"], initialized.control_version);
    assert_eq!(value["companySchema"], initialized.company_version);
}

#[test]
fn mixed_build_info_refuses_before_config_or_listen_even_after_help() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let occupied = listener.local_addr().unwrap().to_string();
    for args in [
        vec!["--help", "--build-info"],
        vec!["--build-info", "--build-info"],
        vec![
            "--config",
            "/missing/private.json",
            "--create",
            "--build-info",
        ],
        vec!["--listen", occupied.as_str(), "--build-info"],
    ] {
        let output = run(&args);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("--build-info must be the only argument")
        );
    }
}
