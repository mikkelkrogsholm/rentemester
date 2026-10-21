//! Operator-owned Google Drive transport and exact approved destination-folder bindings.
use rentemester_runtime::backup_remote::{GatewayConfig, GoogleDriveVerifier};
use serde::Deserialize;
use std::{io::Read, path::PathBuf};
use zeroize::Zeroize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Folder {
    destination_id: String,
    folder_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct Config {
    provider: String,
    url: Option<String>,
    bearer_token: String,
    timeout_ms: Option<u64>,
    trusted_ca_file: Option<PathBuf>,
    destinations: Vec<Folder>,
}
impl Drop for Config {
    fn drop(&mut self) {
        self.bearer_token.zeroize();
    }
}
pub(crate) struct Provider {
    pub verifier: GoogleDriveVerifier,
    pub bindings: Vec<rentemester_application::backup_remote::Binding>,
}
pub(crate) fn build(
    config: Option<&Config>,
    destinations: &[rentemester_runtime::backup_destination::Config],
) -> Result<Option<Provider>, Box<dyn std::error::Error>> {
    let Some(config) = config else {
        return Ok(None);
    };
    if config.provider != "google-drive"
        || config.destinations.is_empty()
        || config.destinations.len() > 16
    {
        return Err("invalid backupRemoteVerification provider or bindings".into());
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut bindings = vec![];
    for binding in &config.destinations {
        if !seen.insert(&binding.destination_id)
            || !rentemester_runtime::backup_remote::valid_object_id(&binding.folder_id)
            || !destinations
                .iter()
                .any(|d| d.id == binding.destination_id && d.kind == "google-drive")
        {
            return Err("backupRemoteVerification requires unique configured Google Drive destinations and exact folder IDs".into());
        }
        bindings.push(rentemester_application::backup_remote::Binding {
            destination_id: binding.destination_id.clone(),
            folder_id: binding.folder_id.clone(),
        });
    }
    let gateway = GatewayConfig {
        endpoint: config
            .url
            .as_deref()
            .unwrap_or("https://www.googleapis.com/drive/v3"),
        bearer_token: &config.bearer_token,
        timeout_ms: config.timeout_ms.unwrap_or(15_000),
    };
    let verifier = if let Some(path) = &config.trusted_ca_file {
        if !path.is_absolute() {
            return Err("backupRemoteVerification trustedCaFile requires absolute path".into());
        }
        let mut pem = Vec::new();
        std::fs::File::open(path)
            .map_err(|_| "backupRemoteVerification trust unavailable")?
            .take(65_537)
            .read_to_end(&mut pem)
            .map_err(|_| "backupRemoteVerification trust unavailable")?;
        if pem.is_empty() || pem.len() > 65_536 {
            return Err("backupRemoteVerification invalid trust size".into());
        }
        GoogleDriveVerifier::with_trusted_ca(gateway, &pem)
    } else {
        GoogleDriveVerifier::new(gateway)
    }
    .map_err(|_| "invalid backupRemoteVerification transport configuration")?;
    Ok(Some(Provider { verifier, bindings }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    fn destination(kind: &str) -> rentemester_runtime::backup_destination::Config {
        serde_json::from_value(json!({"id":"synthetic-drive","label":"Synthetic Drive","kind":kind,"directory":"/synthetic-approved-backup","nonRelatedParty":false,"inEeaOrEu":false,"country":"ZZ","meetsRecognisedStandards":false,"attestedBy":"Synthetic operator","attestedAt":"2026-01-01T00:00:00Z"})).unwrap()
    }
    fn configured() -> Value {
        json!({"provider":"google-drive","bearerToken":"synthetic-secret","destinations":[{"destinationId":"synthetic-drive","folderId":"synthetic_folder"}]})
    }
    #[test]
    fn optional_provider_and_exact_operator_bindings_are_explicit() {
        assert!(build(None, &[]).unwrap().is_none());
        let value: Config = serde_json::from_value(configured()).unwrap();
        let provider = build(Some(&value), &[destination("google-drive")])
            .unwrap()
            .unwrap();
        assert_eq!(provider.bindings.len(), 1);
        assert_eq!(provider.bindings[0].folder_id, "synthetic_folder");
        for destinations in [vec![], vec![destination("local-folder")]] {
            assert!(build(Some(&value), &destinations).is_err());
        }
        for invalid in [
            json!([]),
            json!([{"destinationId":"unconfigured-drive","folderId":"synthetic_folder"}]),
            json!([{"destinationId":"synthetic-drive","folderId":"../unsafe"}]),
            json!([{"destinationId":"synthetic-drive","folderId":"synthetic_folder"},{"destinationId":"synthetic-drive","folderId":"synthetic_folder"}]),
        ] {
            let mut v = configured();
            v["destinations"] = invalid;
            let config: Config = serde_json::from_value(v).unwrap();
            assert!(build(Some(&config), &[destination("google-drive")]).is_err());
        }
    }
    #[test]
    fn wrong_transport_missing_secret_and_untrusted_operator_fields_fail_closed() {
        for (key, value) in [
            ("provider", json!("unknown")),
            ("url", json!("http://127.0.0.1/drive/v3")),
            ("url", json!("https://other.invalid/drive/v3")),
            (
                "url",
                json!("https://www.googleapis.com/drive/v3?token=secret"),
            ),
            ("bearerToken", json!("")),
            ("bearerToken", json!("secret\r\nHeader: injected")),
            ("trustedCaFile", json!("relative.pem")),
        ] {
            let mut v = configured();
            v[key] = value;
            let config: Config = serde_json::from_value(v).unwrap();
            let error = build(Some(&config), &[destination("google-drive")])
                .err()
                .unwrap()
                .to_string();
            assert!(!error.contains("secret"));
        }
        let mut value = configured();
        value["actor"] = json!("user:caller");
        assert!(serde_json::from_value::<Config>(value).is_err());
        let mut value = configured();
        value.as_object_mut().unwrap().remove("bearerToken");
        assert!(serde_json::from_value::<Config>(value).is_err());
    }
}
