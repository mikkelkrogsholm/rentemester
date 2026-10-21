//! Read-only, current-instance exact-archive joins. No filesystem/provider trust or policy inference.
use crate::backup_remote::{self, Receipt};
use rentemester_engine::Error;
use rusqlite::Transaction;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug)]
pub struct Evidence {
    pub placements: Vec<Value>,
    pub remote_verifications: Vec<Receipt>,
}
fn invalid() -> Error {
    Error::new(
        "backup_governance_evidence_unavailable",
        "Matching backup evidence is malformed or inconsistent",
    )
}
fn timestamp(value: &str) -> Result<i128, Error> {
    time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
        .map(|v| v.unix_timestamp_nanos() / 1_000_000)
        .map_err(|_| invalid())
}
fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, Error> {
    value[key].as_str().ok_or_else(invalid)
}
fn operation(value: &str) -> bool {
    (8..=64).contains(&value.len())
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
/// Unknown destinations and different archives are not this company's evidence.
/// Relevant corruption fails closed; future observations are not exposed as completed facts.
pub fn matching(
    tx: &Transaction<'_>,
    destinations: &[String],
    hash: &str,
    size: u64,
    created_at: &str,
    checked_at: &str,
) -> Result<Evidence, Error> {
    let now_ms = timestamp(checked_at)?;
    if now_ms < 0
        || size == 0
        || hash.len() != 64
        || !hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid());
    }
    timestamp(created_at)?;
    let destinations: BTreeSet<_> = destinations.iter().map(String::as_str).collect();
    let mut rows = tx.prepare("SELECT event_type,entity_type,entity_id FROM workspace_audit WHERE id>?1 AND event_type IN ('native_backup_placed','native_backup_remote_verified') ORDER BY id").map_err(|_| invalid())?;
    let mut placements = Vec::new();
    let mut remote = Vec::new();
    let mut placement_ids = BTreeMap::new();
    let mut remote_ids = BTreeMap::new();
    for row in rows
        .query_map([backup_remote::boundary(tx)?], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|_| invalid())?
    {
        let (event, kind, wire) = row.map_err(|_| invalid())?;
        if wire.len() > 16384 {
            return Err(invalid());
        }
        let value: Value = serde_json::from_str(&wire).map_err(|_| invalid())?;
        if !value
            .get("destinationId")
            .and_then(Value::as_str)
            .is_some_and(|id| destinations.contains(id))
            || value["archiveSha256"] != hash
        {
            continue;
        }
        if value["archiveSizeBytes"] != size || value["backupCreatedAt"] != created_at {
            return Err(invalid());
        }
        if event == "native_backup_placed" {
            if kind != "native_backup_placement"
                || value["format"] != "rentemester-native-backup-placement/1"
            {
                return Err(invalid());
            }
            if timestamp(text(&value, "placedAt")?)? > now_ms {
                continue;
            }
            let object = value.as_object().ok_or_else(invalid)?;
            if object.keys().any(|key| {
                ![
                    "format",
                    "destinationId",
                    "operationId",
                    "archiveSha256",
                    "archiveSizeBytes",
                    "backupCreatedAt",
                    "verified",
                    "verifyMethod",
                    "signatureVerified",
                    "remoteSyncVerified",
                    "destination",
                    "placedBy",
                    "placedAt",
                    "auditRecorded",
                ]
                .contains(&key.as_str())
            }) || !operation(text(&value, "operationId")?)
                || !text(&value, "placedBy")?
                    .strip_prefix("user:")
                    .is_some_and(|subject| {
                        !subject.is_empty()
                            && subject.len() <= 251
                            && !subject.chars().any(char::is_control)
                    })
                || value["verified"] != true
                || value["signatureVerified"] != true
                || value["verifyMethod"] != "sha256-reread"
                || value["remoteSyncVerified"] != false
                || !value["destination"].is_object()
                || value["destination"]["id"] != value["destinationId"]
                || value.get("auditRecorded").is_some_and(|v| v != true)
            {
                return Err(invalid());
            }
            let mut normalized = value.clone();
            // The application supplies only the CURRENT operator Config::view; never echo nested audit metadata.
            normalized.as_object_mut().unwrap().remove("destination");
            normalized["auditRecorded"] = json!(true);
            let key = (
                text(&value, "destinationId")?.to_owned(),
                text(&value, "operationId")?.to_owned(),
            );
            if let Some(prior) = placement_ids.insert(key, normalized.clone()) {
                if prior != normalized {
                    return Err(invalid());
                }
            } else {
                placements.push(normalized);
            }
        } else {
            if kind != "native_backup_remote_verification" {
                return Err(invalid());
            }
            let receipt: Receipt = serde_json::from_value(value).map_err(|_| invalid())?;
            if timestamp(&receipt.observed_at)? > now_ms
                || timestamp(&receipt.verified_at)? > now_ms
            {
                continue;
            }
            receipt.validate()?;
            let key = (receipt.destination_id.clone(), receipt.operation_id.clone());
            if let Some(prior) = remote_ids.insert(key, receipt.clone()) {
                if prior != receipt {
                    return Err(invalid());
                }
            } else {
                remote.push(receipt);
            }
        }
    }
    for receipt in &remote {
        if !placements.iter().any(|placement| {
            placement["destinationId"] == receipt.destination_id
                && placement["operationId"] == receipt.placement_operation_id
                && placement["archiveSha256"] == receipt.archive_sha256
                && placement["archiveSizeBytes"] == receipt.archive_size_bytes
                && placement["backupCreatedAt"] == receipt.backup_created_at
        }) {
            return Err(invalid());
        }
    }
    Ok(Evidence {
        placements,
        remote_verifications: remote,
    })
}
#[cfg(test)]
mod tests;
