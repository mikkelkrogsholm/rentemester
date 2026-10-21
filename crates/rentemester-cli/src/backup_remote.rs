//! Exact whole-workspace remote verification and lost-reply reconciliation.
use rentemester_client::ApiClient;
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::collections::BTreeMap;
type Failure = (u8, Error);
fn invalid(message: &str) -> Failure {
    (2, Error::new("invalid_input", message))
}
pub fn accepts(command: &str) -> bool {
    matches!(
        command,
        "verify-backup-remote" | "backup-remote-verification"
    )
}
pub fn help() -> Value {
    json!({"usage":"remote verify-backup-remote --destination ID --operation ID --placement-operation ID --remote-object ID --confirm; remote backup-remote-verification --destination ID --operation ID. Explicit --server and private --cookie-file required; whole workspace, no --company or stdin body.","policy":"Current human MFA workspace/all-company administration. Verify an existing placement through operator-bound Google Drive metadata and full-byte SHA256 reread. Caller supplies selectors only, never provider URLs, credentials, folder IDs, paths, keys or archive claims. Never automatically repeat POST after an unknown outcome: use backup-remote-verification with the original destination and operation ID. GET returns the retained original observation or null; historical observation is not current remote availability or independent provider attestation."})
}
fn id(value: &str) -> bool {
    (8..=64).contains(&value.len())
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
fn object(value: &str) -> bool {
    (1..=160).contains(&value.len())
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow: bool,
    confirm: bool,
    preview: bool,
) -> Result<Value, Failure> {
    let write = command == "verify-backup-remote";
    if !accepts(command)
        || confirm != write
        || preview
        || flags.keys().any(|key| {
            !(["--server", "--cookie-file", "--destination", "--operation"].contains(key)
                || write && ["--placement-operation", "--remote-object"].contains(key))
        })
    {
        return Err(invalid(
            "remote verification requires exact selectors, human cookie and POST-only confirmation; no company/body/provider/authority flags",
        ));
    }
    let required = |key| {
        flags
            .get(key)
            .copied()
            .ok_or_else(|| invalid("explicit remote verification selectors/server/cookie required"))
    };
    let destination = required("--destination")?;
    let operation = required("--operation")?;
    let placement = if write {
        required("--placement-operation")?
    } else {
        ""
    };
    let remote = if write {
        required("--remote-object")?
    } else {
        ""
    };
    if !id(destination) || !id(operation) || write && (!id(placement) || !object(remote)) {
        return Err(invalid("invalid remote verification selector"));
    }
    let server = required("--server")?;
    let cookie_path = required("--cookie-file")?;
    let cookie = rentemester_client::read_cookie_file(std::path::Path::new(cookie_path))
        .map_err(crate::remote::client_error)?;
    let api = ApiClient::new(server, allow, Some(cookie)).map_err(crate::remote::client_error)?;
    let runtime =
        tokio::runtime::Runtime::new().map_err(|_| invalid("HTTP runtime unavailable"))?;
    let data = runtime
        .block_on(async {
            if write {
                api.verify_backup_remote(destination, operation, placement, remote, confirm)
                    .await
            } else {
                api.backup_remote_verification(destination, operation).await
            }
        })
        .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":true,"data":data}))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_flags_confirmation_and_identifiers_precede_cookie_access() {
        let valid = BTreeMap::from([
            ("--server", "https://example.invalid"),
            ("--cookie-file", "/missing-private-cookie"),
            ("--destination", "drive-0001"),
            ("--operation", "verify-0001"),
            ("--placement-operation", "placed-0001"),
            ("--remote-object", "Drive_Object-1"),
        ]);
        assert_eq!(
            run("verify-backup-remote", &valid, false, false, false)
                .unwrap_err()
                .0,
            2
        );
        for field in [
            "--company",
            "--body",
            "--provider-url",
            "--key",
            "--actor",
            "--archive-sha256",
            "--service-key-file",
        ] {
            let mut flags = valid.clone();
            flags.insert(field, "forged");
            let error = run("verify-backup-remote", &flags, false, true, false).unwrap_err();
            assert_eq!(error.0, 2);
            assert!(error.1.message.contains("exact selectors"));
        }
        for (key, value) in [
            ("--destination", "../drive"),
            ("--operation", "VERIFY-0001"),
            ("--placement-operation", "bad"),
            ("--remote-object", "Object?alt=media"),
        ] {
            let mut flags = valid.clone();
            flags.insert(key, value);
            assert!(
                run("verify-backup-remote", &flags, false, true, false)
                    .unwrap_err()
                    .1
                    .message
                    .contains("invalid remote verification selector")
            );
        }
        assert_eq!(
            run("backup-remote-verification", &valid, false, false, false)
                .unwrap_err()
                .0,
            2
        );
        let read = BTreeMap::from([
            ("--destination", "drive-0001"),
            ("--operation", "verify-0001"),
        ]);
        assert_eq!(
            run("backup-remote-verification", &read, false, true, false)
                .unwrap_err()
                .0,
            2
        );
    }
    #[test]
    fn commands_are_discoverable_and_new_selectors_are_not_generic_authority() {
        let help = crate::remote::run(&["--help".into()]).unwrap();
        for name in ["verify-backup-remote", "backup-remote-verification"] {
            assert!(
                help["commands"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|v| v == name)
            );
        }
        for flag in [
            "--destination",
            "--operation",
            "--placement-operation",
            "--remote-object",
        ] {
            let args = vec!["health".into(), flag.into(), "synthetic-id".into()];
            assert!(crate::remote::run(&args).is_err());
        }
    }
}
