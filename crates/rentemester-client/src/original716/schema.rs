//! Private stopped-server operator subprocess, never an HTTP fallback or retry.
use serde_json::Value;
use std::path::Path;
/// The explicit executable is invoked once; a non-JSON/failed result is observable.
/// Neither company paths nor actor assertions are forwarded as server authority.
pub fn execute_private_schema(
    server_binary: &Path,
    config: &Path,
    company: &str,
    repair: bool,
    apply: bool,
    reason: Option<&str>,
) -> Result<Value, String> {
    if !server_binary.is_absolute() || !config.is_absolute() {
        return Err("schema operator requires absolute --server-bin and --config paths".into());
    }
    let mut command = std::process::Command::new(server_binary);
    command
        .arg("--original716-schema")
        .arg(if repair {
            "repair-schema-views"
        } else {
            "migrate"
        })
        .arg("--config")
        .arg(config)
        .arg("--company")
        .arg(company);
    if apply {
        command.args(["--apply", "yes"]);
    }
    if let Some(reason) = reason {
        command.arg("--reason").arg(reason);
    }
    let output = command
        .output()
        .map_err(|e| format!("schema operator could not execute: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "schema operator failed ({}); effects not retried: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|_| "schema operator returned invalid JSON; effects not retried".into())
}
