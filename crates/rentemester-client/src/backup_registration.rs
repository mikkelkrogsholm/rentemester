//! Inert owner registrations and explicitly unverified transfer declarations.
use crate::{
    ApiClient, Error, company_path,
    domain::{body, object},
};
use reqwest::Method;
use serde_json::{Value, json};
pub const MAX_BACKUP_REGISTRATION_BODY: usize = 16 * 1024;
#[derive(Clone, Copy)]
pub enum BackupRegistrationMutation {
    Add,
    Remove,
    Declare,
}
impl BackupRegistrationMutation {
    fn action(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Remove => "remove",
            Self::Declare => "declare",
        }
    }
}
pub fn backup_registration_body_schema(action: BackupRegistrationMutation) -> Value {
    let id = json!({"type":"string","minLength":8,"maxLength":64});
    let mut properties = json!({"operationId":id,"destinationId":id,"confirm":{"const":true}});
    let mut required = vec!["operationId", "destinationId", "confirm"];
    match action {
        BackupRegistrationMutation::Add => {
            for (name, schema) in [
                (
                    "label",
                    json!({"type":"string","minLength":1,"maxLength":128}),
                ),
                (
                    "kind",
                    json!({"type":"string","enum":["local-folder","dropbox","google-drive","ssh","other"]}),
                ),
                (
                    "location",
                    json!({"type":"string","minLength":1,"maxLength":1024}),
                ),
                (
                    "attestedBy",
                    json!({"type":"string","minLength":1,"maxLength":128}),
                ),
                ("inEeaOrEu", json!({"type":"boolean"})),
                ("nonRelatedParty", json!({"type":"boolean"})),
                (
                    "regionCountry",
                    json!({"type":["string","null"],"minLength":2,"maxLength":2}),
                ),
                (
                    "regionNote",
                    json!({"type":["string","null"],"maxLength":2048}),
                ),
                (
                    "itSecurityMeetsStandards",
                    json!({"type":["boolean","null"]}),
                ),
                (
                    "itSecurityNote",
                    json!({"type":["string","null"],"maxLength":2048}),
                ),
                (
                    "operatorTargetId",
                    json!({"type":["string","null"],"minLength":8,"maxLength":64}),
                ),
            ] {
                properties[name] = schema
            }
            required.extend([
                "label",
                "kind",
                "location",
                "attestedBy",
                "inEeaOrEu",
                "nonRelatedParty",
            ]);
        }
        BackupRegistrationMutation::Declare => {
            properties["backupId"] = json!({"type":"string","minLength":64,"maxLength":64});
            properties["note"] = json!({"type":["string","null"],"maxLength":4096});
            required.push("backupId");
        }
        BackupRegistrationMutation::Remove => {}
    }
    object(properties, &required)
}
fn id(s: &str) -> bool {
    (8..=64).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
fn hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn input(
    bytes: &[u8],
    action: BackupRegistrationMutation,
    confirmed: bool,
) -> Result<Value, Error> {
    if !confirmed || bytes.len() > MAX_BACKUP_REGISTRATION_BODY {
        return Err(Error::InvalidInput(
            "explicit backup registration confirmation and body at most16KiB required",
        ));
    }
    body(bytes, &backup_registration_body_schema(action))?;
    let mut value: Value = serde_json::from_slice(bytes)
        .map_err(|_| Error::InvalidInput("invalid backup registration JSON"))?;
    let invalid = || {
        Error::InvalidInput(
            "invalid backup registration fields; location is inert metadata, never path or network authority",
        )
    };
    for field in ["operationId", "destinationId"] {
        if !value[field].as_str().is_some_and(id) {
            return Err(invalid());
        }
    }
    for field in [
        "label",
        "location",
        "attestedBy",
        "regionCountry",
        "regionNote",
        "itSecurityNote",
        "operatorTargetId",
        "note",
    ] {
        if let Some(text) = value[field].as_str()
            && (text.chars().any(char::is_control)
                || (["label", "location", "attestedBy"].contains(&field) && text.trim().is_empty()))
        {
            return Err(invalid());
        }
    }
    if value["operatorTargetId"].as_str().is_some_and(|s| !id(s))
        || value["regionCountry"]
            .as_str()
            .is_some_and(|s| s.len() != 2 || !s.bytes().all(|b| b.is_ascii_uppercase()))
        || value["backupId"].as_str().is_some_and(|s| !hash(s))
    {
        return Err(invalid());
    }
    if ["ssh", "other"].iter().any(|kind| value["kind"] == *kind)
        && !value["operatorTargetId"].is_null()
    {
        return Err(invalid());
    }
    value["action"] = json!(action.action());
    if serde_json::to_vec(&value).map_err(|_| invalid())?.len() > MAX_BACKUP_REGISTRATION_BODY {
        return Err(invalid());
    }
    Ok(value)
}
fn operation(value: &Value) -> bool {
    let result = &value["result"];
    if value["format"] != "rentemester-native-backup-registration-operation/1"
        || !value["operationId"].as_str().is_some_and(id)
        || !value["destinationId"].as_str().is_some_and(id)
        || !value["actor"]
            .as_str()
            .is_some_and(|s| s.starts_with("user:") && s.len() > 5)
        || !value["createdAt"].is_string()
        || result["destinationId"] != value["destinationId"]
    {
        return false;
    }
    match value["action"].as_str() {
        Some("add") => result["active"] == true,
        Some("remove") => result["active"] == false,
        Some("declare") => {
            result["verified"] == false
                && result["verifyMethod"] == "declared"
                && result["remoteSyncVerified"] == false
                && result["attestationIndependentlyVerified"] == false
                && result["backupId"] == result["archiveSha256"]
                && result["archiveSha256"].as_str().is_some_and(hash)
                && result["archiveSizeBytes"].as_u64().is_some_and(|n| n > 0)
        }
        _ => false,
    }
}
fn response(
    value: Value,
    company: &str,
    list: bool,
    expected: Option<&str>,
    write: bool,
) -> Result<Value, Error> {
    let valid = value["ok"] == true
        && value["slug"] == company
        && if list {
            value["destinations"].as_array().is_some_and(|a| {
                a.iter()
                    .all(|d| d["active"] == true && d["destinationId"].as_str().is_some_and(id))
            }) && value["history"]
                .as_array()
                .is_some_and(|a| a.iter().all(operation))
                && value["backups"].as_array().is_some_and(|a| {
                    a.iter().all(|b| {
                        b["backupId"] == b["archiveSha256"]
                            && b["backupId"].as_str().is_some_and(hash)
                            && b["archiveSizeBytes"].as_u64().is_some_and(|n| n > 0)
                    })
                })
                && value["operatorTargets"].is_array()
                && value["errors"]
                    .as_array()
                    .is_some_and(|a| a.iter().all(Value::is_string))
        } else {
            value.get("operation").is_some_and(|o| {
                (!write && o.is_null())
                    || operation(o) && expected.is_some_and(|id| o["operationId"] == id)
            })
        };
    if valid {
        Ok(value)
    } else if write {
        Err(Error::OutcomeUnknown)
    } else {
        Err(Error::Protocol)
    }
}
impl ApiClient {
    fn backup_registration_human(&self) -> Result<(), Error> {
        if self.cookie.is_none() || self.service_key.is_some() {
            Err(Error::InvalidInput(
                "current human MFA session cookie required",
            ))
        } else {
            Ok(())
        }
    }
    pub async fn backup_registrations(&self, company: &str) -> Result<Value, Error> {
        let path = company_path(company, "backup-registrations")?;
        self.backup_registration_human()?;
        response(self.get(&path, &[]).await?, company, true, None, false)
    }
    pub async fn backup_registration_operation(
        &self,
        company: &str,
        operation: &str,
    ) -> Result<Value, Error> {
        let path = company_path(company, "backup-registrations")?;
        if !id(operation) {
            return Err(Error::InvalidInput(
                "valid backup registration operation required",
            ));
        }
        self.backup_registration_human()?;
        response(
            self.get(&format!("{path}/operations/{operation}"), &[])
                .await?,
            company,
            false,
            Some(operation),
            false,
        )
    }
    async fn backup_registration_apply(
        &self,
        company: &str,
        bytes: Vec<u8>,
        confirmed: bool,
        action: BackupRegistrationMutation,
    ) -> Result<Value, Error> {
        let path = company_path(company, "backup-registrations")?;
        let value = input(&bytes, action, confirmed)?;
        self.backup_registration_human()?;
        let wire = serde_json::to_vec(&value)
            .map_err(|_| Error::InvalidInput("invalid backup registration JSON"))?;
        response(
            self.request(Method::POST, &path, &[], Some(wire)).await?,
            company,
            false,
            value["operationId"].as_str(),
            true,
        )
        .and_then(|receipt| {
            if receipt["operation"]["action"] != value["action"]
                || receipt["operation"]["destinationId"] != value["destinationId"]
            {
                Err(Error::OutcomeUnknown)
            } else {
                Ok(receipt)
            }
        })
    }
    /// One explicit POST. Reconcile an uncertain result with the original operation ID; never auto-retry.
    pub async fn add_backup_destination(
        &self,
        company: &str,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        self.backup_registration_apply(company, bytes, confirmed, BackupRegistrationMutation::Add)
            .await
    }
    pub async fn remove_backup_destination(
        &self,
        company: &str,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        self.backup_registration_apply(
            company,
            bytes,
            confirmed,
            BackupRegistrationMutation::Remove,
        )
        .await
    }
    pub async fn confirm_backup_placement(
        &self,
        company: &str,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        self.backup_registration_apply(
            company,
            bytes,
            confirmed,
            BackupRegistrationMutation::Declare,
        )
        .await
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_variant_fields_fixed_action_and_unverified_semantics() {
        let remove = br#"{"operationId":"remove-0001","destinationId":"dest-0001","confirm":true}"#;
        assert_eq!(
            input(remove, BackupRegistrationMutation::Remove, true).unwrap()["action"],
            "remove"
        );
        for extra in [
            "action",
            "actor",
            "createdAt",
            "path",
            "archiveSha256",
            "size",
            "providerUrl",
        ] {
            let mut v: Value = serde_json::from_slice(remove).unwrap();
            v[extra] = json!("forged");
            assert!(
                input(
                    &serde_json::to_vec(&v).unwrap(),
                    BackupRegistrationMutation::Remove,
                    true
                )
                .is_err()
            )
        }
        assert!(input(remove, BackupRegistrationMutation::Remove, false).is_err());
        let declare = json!({"operationId":"declare-0001","destinationId":"dest-0001","backupId":"a".repeat(64),"confirm":true,"note":"Owner declaration only"});
        assert_eq!(
            input(
                &serde_json::to_vec(&declare).unwrap(),
                BackupRegistrationMutation::Declare,
                true
            )
            .unwrap()["action"],
            "declare"
        );
    }
    #[tokio::test]
    async fn missing_human_service_and_invalid_identifiers_fail_before_network() {
        let apis = [
            ApiClient::new("http://127.0.0.1:9", true, None).unwrap(),
            ApiClient::with_service_key("http://127.0.0.1:9", true, "synthetic-service".into())
                .unwrap(),
        ];
        for api in apis {
            assert!(matches!(
                api.backup_registrations("synthetic").await,
                Err(Error::InvalidInput(_))
            ));
            assert!(matches!(
                api.backup_registration_operation("synthetic", "operation-0001")
                    .await,
                Err(Error::InvalidInput(_))
            ));
            assert!(matches!(
                api.remove_backup_destination(
                    "synthetic",
                    br#"{"operationId":"remove-0001","destinationId":"dest-0001","confirm":true}"#
                        .to_vec(),
                    true
                )
                .await,
                Err(Error::InvalidInput(_))
            ))
        }
    }
    #[test]
    fn all_registration_kinds_are_inert_and_explicit_booleans_and_country_are_checked() {
        for kind in ["local-folder", "dropbox", "google-drive", "ssh", "other"] {
            let value = json!({"operationId":"register-0001","destinationId":"dest-0001","confirm":true,"label":"Synthetic","kind":kind,"location":"ssh://synthetic.invalid/not-executed","attestedBy":"Synthetic owner","inEeaOrEu":false,"nonRelatedParty":false,"regionCountry":"DK","regionNote":null,"itSecurityMeetsStandards":null,"operatorTargetId":null});
            assert_eq!(
                input(
                    &serde_json::to_vec(&value).unwrap(),
                    BackupRegistrationMutation::Add,
                    true
                )
                .unwrap()["action"],
                "add"
            );
            for (field, bad) in [
                ("inEeaOrEu", json!(null)),
                ("label", json!(" ")),
                ("location", json!("bad\nlocation")),
                ("regionCountry", json!("dk")),
                ("operatorTargetId", json!("../path")),
            ] {
                let mut v = value.clone();
                v[field] = bad;
                assert!(
                    input(
                        &serde_json::to_vec(&v).unwrap(),
                        BackupRegistrationMutation::Add,
                        true
                    )
                    .is_err()
                )
            }
        }
    }
    #[test]
    fn declaration_receipts_never_claim_verification() {
        let value = json!({"ok":true,"slug":"synthetic","operation":{"format":"rentemester-native-backup-registration-operation/1","operationId":"declare-0001","action":"declare","destinationId":"dest-0001","actor":"user:synthetic","createdAt":"2026-10-21T12:00:00.000Z","result":{"destinationId":"dest-0001","backupId":"a".repeat(64),"archiveSha256":"a".repeat(64),"archiveSizeBytes":12,"verified":false,"verifyMethod":"declared","remoteSyncVerified":false,"attestationIndependentlyVerified":false}}});
        assert_eq!(
            response(
                value.clone(),
                "synthetic",
                false,
                Some("declare-0001"),
                true
            )
            .unwrap(),
            value
        );
        for field in [
            "verified",
            "remoteSyncVerified",
            "attestationIndependentlyVerified",
        ] {
            let mut bad = value.clone();
            bad["operation"]["result"][field] = json!(true);
            assert!(matches!(
                response(bad, "synthetic", false, Some("declare-0001"), true),
                Err(Error::OutcomeUnknown)
            ))
        }
        assert!(
            response(
                json!({"ok":true,"slug":"synthetic","operation":null}),
                "synthetic",
                false,
                Some("declare-0001"),
                false
            )
            .is_ok()
        );
        assert!(matches!(
            response(
                json!({"ok":true,"slug":"synthetic","operation":null}),
                "synthetic",
                false,
                Some("declare-0001"),
                true
            ),
            Err(Error::OutcomeUnknown)
        ));
    }
    #[tokio::test]
    async fn mutation_sends_fixed_action_once_and_inspection_is_bodyless() {
        use std::{
            io::{Read, Write},

            thread,
        };
        for inspect in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let origin = format!("http://{}", listener.local_addr().unwrap());
            let reply = if inspect {
                json!({"ok":true,"slug":"synthetic","operation":null})
            } else {
                json!({"ok":true,"slug":"synthetic","operation":{"format":"rentemester-native-backup-registration-operation/1","operationId":"remove-0001","action":"remove","destinationId":"dest-0001","actor":"user:synthetic","createdAt":"2026-10-21T12:00:00.000Z","result":{"destinationId":"dest-0001","active":false}}})
            };
            let expected = reply.clone();
            let reply = reply.to_string();
            let worker = thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                    .unwrap();
                let mut request = Vec::new();
                loop {
                    let mut buf = [0; 4096];
                    let n = stream.read(&mut buf).unwrap();
                    assert!(n > 0);
                    request.extend_from_slice(&buf[..n]);
                    if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&request[..end]).to_ascii_lowercase();
                        let size = headers
                            .lines()
                            .find_map(|l| l.strip_prefix("content-length: "))
                            .map(|n| n.parse::<usize>().unwrap())
                            .unwrap_or(0);
                        if request.len() >= end + 4 + size {
                            break;
                        }
                    }
                }
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Type:application/json\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{reply}",reply.len()).unwrap();
                request
            });
            let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
            let reply = if inspect {
                api.backup_registration_operation("synthetic", "remove-0001")
                    .await
            } else {
                api.remove_backup_destination(
                    "synthetic",
                    br#"{"operationId":"remove-0001","destinationId":"dest-0001","confirm":true}"#
                        .to_vec(),
                    true,
                )
                .await
            };
            assert_eq!(reply.unwrap(), expected);
            let wire = String::from_utf8(worker.join().unwrap()).unwrap();
            assert!(
                wire.to_ascii_lowercase()
                    .contains("cookie: session=synthetic")
            );
            if inspect {
                assert!(wire.starts_with("GET /api/companies/synthetic/backup-registrations/operations/remove-0001 HTTP/1.1"));
                assert_eq!(wire.split("\r\n\r\n").nth(1), Some(""))
            } else {
                assert!(
                    wire.starts_with("POST /api/companies/synthetic/backup-registrations HTTP/1.1")
                );
                let body: Value =
                    serde_json::from_str(wire.split("\r\n\r\n").nth(1).unwrap()).unwrap();
                assert_eq!(
                    body,
                    json!({"action":"remove","operationId":"remove-0001","destinationId":"dest-0001","confirm":true})
                );
            }
        }
    }
}

#[cfg(test)]
use crate::test_transport::TcpListener;
