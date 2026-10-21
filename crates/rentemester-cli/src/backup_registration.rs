//! Inert owner destination metadata and manual declarations; no upload or shell.
use rentemester_client::{
    ApiClient, BackupRegistrationMutation, MAX_BACKUP_REGISTRATION_BODY,
    backup_registration_body_schema,
};
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
type Failure = (u8, Error);
fn invalid() -> Failure {
    (
        2,
        Error::new(
            "invalid_input",
            "backup registrations require current company/server/private human cookie, exact selectors, and write-only confirmation; no actor/time/service/path authority",
        ),
    )
}
pub fn accepts(command: &str) -> bool {
    matches!(
        command,
        "backup-registrations"
            | "backup-registration-operation"
            | "backup-add-destination"
            | "backup-remove-destination"
            | "backup-confirm-placement"
    )
}
pub fn help() -> Value {
    json!({"usage":"remote backup-registrations --company SLUG; remote backup-registration-operation --company SLUG --operation ID; remote backup-add-destination|backup-remove-destination|backup-confirm-placement --company SLUG --confirm < variant.json. Explicit --server and private --cookie-file required.","policy":"Current human CompanyAdmin. JSON at most16KiB includes operationId and confirm:true, excludes action (SDK supplies it). Locations are inert descriptions. Confirmation records an unverified declaration; it does not transfer files or qualify an external copy. Preserve the original operation ID and reconcile unknown outcomes explicitly using backup-registration-operation; no automatic retries.","bodySchemas":{"add":backup_registration_body_schema(BackupRegistrationMutation::Add),"remove":backup_registration_body_schema(BackupRegistrationMutation::Remove),"declare":backup_registration_body_schema(BackupRegistrationMutation::Declare)}})
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow: bool,
    confirm: bool,
    preview: bool,
) -> Result<Value, Failure> {
    let inspect = command == "backup-registration-operation";
    let write = !matches!(
        command,
        "backup-registrations" | "backup-registration-operation"
    );
    if !accepts(command)
        || confirm != write
        || preview
        || flags.keys().any(|k| {
            !(["--company", "--server", "--cookie-file"].contains(k)
                || inspect && *k == "--operation")
        })
    {
        return Err(invalid());
    }
    let required = |key| flags.get(key).copied().ok_or_else(invalid);
    let company = required("--company")?;
    if company.is_empty()
        || !company
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(invalid());
    }
    let operation = if inspect {
        required("--operation")?
    } else {
        ""
    };
    if inspect
        && (!(8..=64).contains(&operation.len())
            || !operation
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'))
    {
        return Err(invalid());
    }
    let server = required("--server")?;
    let cookie = required("--cookie-file")?;
    let mut bytes = Vec::new();
    if write {
        std::io::stdin()
            .take(MAX_BACKUP_REGISTRATION_BODY as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid())?;
        if bytes.len() > MAX_BACKUP_REGISTRATION_BODY {
            return Err(invalid());
        }
    }
    let cookie = rentemester_client::read_cookie_file(std::path::Path::new(cookie))
        .map_err(crate::remote::client_error)?;
    let api = ApiClient::new(server, allow, Some(cookie)).map_err(crate::remote::client_error)?;
    let runtime = tokio::runtime::Runtime::new().map_err(|_| invalid())?;
    let data = runtime
        .block_on(async {
            match command {
                "backup-registrations" => api.backup_registrations(company).await,
                "backup-registration-operation" => {
                    api.backup_registration_operation(company, operation).await
                }
                "backup-add-destination" => {
                    api.add_backup_destination(company, bytes, confirm).await
                }
                "backup-remove-destination" => {
                    api.remove_backup_destination(company, bytes, confirm).await
                }
                _ => api.confirm_backup_placement(company, bytes, confirm).await,
            }
        })
        .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":true,"data":data}))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_scope_confirmation_and_discovery() {
        let valid = BTreeMap::from([
            ("--server", "https://example.invalid"),
            ("--cookie-file", "/missing-cookie"),
            ("--company", "synthetic"),
        ]);
        for field in [
            "--actor",
            "--as-of",
            "--service-key-file",
            "--operation",
            "--path",
        ] {
            let mut flags = valid.clone();
            flags.insert(field, "forged");
            assert_eq!(
                run("backup-registrations", &flags, false, false, false)
                    .unwrap_err()
                    .0,
                2
            )
        }
        assert_eq!(
            run("backup-add-destination", &valid, false, false, false)
                .unwrap_err()
                .0,
            2
        );
        assert_eq!(
            run("backup-registrations", &valid, false, true, false)
                .unwrap_err()
                .0,
            2
        );
        let help = crate::remote::run(&["--help".into()]).unwrap();
        for name in [
            "backup-registrations",
            "backup-registration-operation",
            "backup-add-destination",
            "backup-remove-destination",
            "backup-confirm-placement",
        ] {
            assert!(
                help["commands"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|v| v == name)
            )
        }
    }
}
