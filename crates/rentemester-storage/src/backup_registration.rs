//! Inert owner metadata and unverified declarations. No location is ever opened or executed.
use rentemester_engine::Error;
use rusqlite::{Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "action",
    rename_all = "lowercase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Request {
    Add {
        operation_id: String,
        destination_id: String,
        label: String,
        kind: String,
        location: String,
        attested_by: String,
        in_eea_or_eu: bool,
        non_related_party: bool,
        region_country: Option<String>,
        region_note: Option<String>,
        it_security_meets_standards: Option<bool>,
        it_security_note: Option<String>,
        operator_target_id: Option<String>,
        confirm: bool,
    },
    Remove {
        operation_id: String,
        destination_id: String,
        confirm: bool,
    },
    Declare {
        operation_id: String,
        destination_id: String,
        backup_id: String,
        note: Option<String>,
        confirm: bool,
    },
}
fn invalid() -> Error {
    Error::new(
        "backup_registration_refused",
        "Backup registration or declaration refused",
    )
}
pub fn valid_id(s: &str) -> bool {
    (8..=64).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
pub fn valid_hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn text(s: &str, max: usize) -> Result<String, Error> {
    if s.chars().any(char::is_control) {
        return Err(invalid());
    }
    let s = s.trim();
    if s.is_empty() || s.len() > max {
        return Err(invalid());
    }
    Ok(s.into())
}
fn optional(s: &Option<String>, max: usize) -> Result<Option<String>, Error> {
    s.as_ref()
        .map(|v| {
            if v.chars().any(char::is_control) || v.trim().len() > max {
                return Err(invalid());
            }
            Ok(v.trim().to_owned())
        })
        .transpose()
}
fn time(s: &str) -> Result<i128, Error> {
    time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339)
        .map(|v| v.unix_timestamp_nanos())
        .map_err(|_| invalid())
}
impl Request {
    pub fn operation_id(&self) -> &str {
        match self {
            Self::Add { operation_id, .. }
            | Self::Remove { operation_id, .. }
            | Self::Declare { operation_id, .. } => operation_id,
        }
    }
    pub fn destination_id(&self) -> &str {
        match self {
            Self::Add { destination_id, .. }
            | Self::Remove { destination_id, .. }
            | Self::Declare { destination_id, .. } => destination_id,
        }
    }
    pub fn action(&self) -> &'static str {
        match self {
            Self::Add { .. } => "add",
            Self::Remove { .. } => "remove",
            Self::Declare { .. } => "declare",
        }
    }
    pub fn canonical(&self) -> Result<Self, Error> {
        if !valid_id(self.operation_id()) || !valid_id(self.destination_id()) {
            return Err(invalid());
        }
        Ok(match self {
            Self::Add {
                operation_id,
                destination_id,
                label,
                kind,
                location,
                attested_by,
                in_eea_or_eu,
                non_related_party,
                region_country,
                region_note,
                it_security_meets_standards,
                it_security_note,
                operator_target_id,
                confirm,
            } => {
                if !confirm
                    || !["local-folder", "dropbox", "google-drive", "ssh", "other"]
                        .contains(&kind.as_str())
                    || region_country
                        .as_ref()
                        .is_some_and(|s| s.len() != 2 || !s.bytes().all(|b| b.is_ascii_uppercase()))
                    || operator_target_id.as_ref().is_some_and(|s| !valid_id(s))
                    || (["ssh", "other"].contains(&kind.as_str()) && operator_target_id.is_some())
                {
                    return Err(invalid());
                }
                Self::Add {
                    operation_id: operation_id.clone(),
                    destination_id: destination_id.clone(),
                    label: text(label, 128)?,
                    kind: kind.clone(),
                    location: text(location, 1024)?,
                    attested_by: text(attested_by, 128)?,
                    in_eea_or_eu: *in_eea_or_eu,
                    non_related_party: *non_related_party,
                    region_country: region_country.clone(),
                    region_note: optional(region_note, 2048)?,
                    it_security_meets_standards: *it_security_meets_standards,
                    it_security_note: optional(it_security_note, 2048)?,
                    operator_target_id: operator_target_id.clone(),
                    confirm: true,
                }
            }
            Self::Remove {
                operation_id,
                destination_id,
                confirm,
            } => {
                if !confirm {
                    return Err(invalid());
                }
                Self::Remove {
                    operation_id: operation_id.clone(),
                    destination_id: destination_id.clone(),
                    confirm: true,
                }
            }
            Self::Declare {
                operation_id,
                destination_id,
                backup_id,
                note,
                confirm,
            } => {
                if !confirm || !valid_hash(backup_id) {
                    return Err(invalid());
                }
                Self::Declare {
                    operation_id: operation_id.clone(),
                    destination_id: destination_id.clone(),
                    backup_id: backup_id.clone(),
                    note: optional(note, 4096)?,
                    confirm: true,
                }
            }
        })
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Receipt {
    pub format: String,
    pub operation_id: String,
    pub action: String,
    pub destination_id: String,
    pub actor: String,
    pub created_at: String,
    pub result: Value,
}
#[derive(Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Event {
    format: String,
    company_slug: String,
    intent_hash: String,
    request: Request,
    receipt: Receipt,
}
pub struct ProtectedBackup {
    pub backup_id: String,
    pub archive_sha256: String,
    pub archive_size_bytes: u64,
    pub backup_created_at: String,
}
fn intent(slug: &str, request: &Request) -> String {
    format!(
        "{:x}",
        Sha256::digest(
            json!({"companySlug":slug,"request":request})
                .to_string()
                .as_bytes()
        )
    )
}
fn result(
    request: &Request,
    actor: &str,
    at: &str,
    backup: Option<&ProtectedBackup>,
) -> Result<Value, Error> {
    Ok(match request {
        Request::Add {
            destination_id,
            label,
            kind,
            location,
            attested_by,
            in_eea_or_eu,
            non_related_party,
            region_country,
            region_note,
            it_security_meets_standards,
            it_security_note,
            operator_target_id,
            ..
        } => {
            json!({"destinationId":destination_id,"label":label,"kind":kind,"location":location,"attestedBy":attested_by,"inEeaOrEu":in_eea_or_eu,"nonRelatedParty":non_related_party,"regionCountry":region_country,"regionNote":region_note,"itSecurityMeetsStandards":it_security_meets_standards,"itSecurityNote":it_security_note,"operatorTargetId":operator_target_id,"createdAt":at,"createdBy":actor,"active":true})
        }
        Request::Remove { destination_id, .. } => {
            json!({"destinationId":destination_id,"active":false})
        }
        Request::Declare {
            destination_id,
            backup_id,
            note,
            ..
        } => {
            let b = backup.ok_or_else(invalid)?;
            if &b.backup_id != backup_id
                || b.archive_sha256 != b.backup_id
                || !valid_hash(&b.backup_id)
                || b.archive_size_bytes == 0
                || b.archive_size_bytes > 96 * 1024 * 1024
                || time(&b.backup_created_at)? < 0
                || time(&b.backup_created_at)? > time(at)?
            {
                return Err(invalid());
            }
            json!({"destinationId":destination_id,"backupId":b.backup_id,"archiveSha256":b.archive_sha256,"archiveSizeBytes":b.archive_size_bytes,"backupCreatedAt":b.backup_created_at,"placedAt":at,"actor":actor,"note":note,"verified":false,"verifyMethod":"declared","remoteSyncVerified":false,"attestationIndependentlyVerified":false})
        }
    })
}
fn events(tx: &Transaction<'_>) -> Result<Vec<Event>, Error> {
    let mut q=tx.prepare("SELECT entity_id,actor FROM workspace_audit WHERE event_type='native_backup_registration' AND entity_type='backup_registration_operation' ORDER BY id").map_err(|_|invalid())?;
    let mut events = vec![];
    let mut operations = BTreeMap::new();
    let mut seen = BTreeSet::new();
    let mut active = BTreeSet::new();
    for row in q
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|_| invalid())?
    {
        let (wire, actor) = row.map_err(|_| invalid())?;
        if wire.len() > 32768 {
            return Err(invalid());
        }
        let e: Event = serde_json::from_str(&wire).map_err(|_| invalid())?;
        if e.format != "rentemester-native-backup-registration-event/1"
            || text(&e.company_slug, 128)? != e.company_slug
            || e.request.canonical()? != e.request
            || e.intent_hash != intent(&e.company_slug, &e.request)
            || e.receipt.format != "rentemester-native-backup-registration-operation/1"
            || e.receipt.operation_id != e.request.operation_id()
            || e.receipt.destination_id != e.request.destination_id()
            || e.receipt.action != e.request.action()
            || actor != e.receipt.actor
            || !actor.starts_with("user:")
            || actor.len() <= 5
            || actor.len() > 256
            || actor.chars().any(char::is_control)
            || time(&e.receipt.created_at)? < 0
        {
            return Err(invalid());
        }
        let backup = if matches!(e.request, Request::Declare { .. }) {
            Some(ProtectedBackup {
                backup_id: e.receipt.result["backupId"]
                    .as_str()
                    .ok_or_else(invalid)?
                    .into(),
                archive_sha256: e.receipt.result["archiveSha256"]
                    .as_str()
                    .ok_or_else(invalid)?
                    .into(),
                archive_size_bytes: e.receipt.result["archiveSizeBytes"]
                    .as_u64()
                    .ok_or_else(invalid)?,
                backup_created_at: e.receipt.result["backupCreatedAt"]
                    .as_str()
                    .ok_or_else(invalid)?
                    .into(),
            })
        } else {
            None
        };
        if result(&e.request, &actor, &e.receipt.created_at, backup.as_ref())? != e.receipt.result {
            return Err(invalid());
        }
        if let Some(prior) = operations.insert(
            e.request.operation_id().to_owned(),
            (e.intent_hash.clone(), e.receipt.clone()),
        ) {
            if prior != (e.intent_hash.clone(), e.receipt.clone()) {
                return Err(invalid());
            }
            continue;
        }
        let key = (
            e.company_slug.clone(),
            e.request.destination_id().to_owned(),
        );
        match e.request {
            Request::Add { .. } => {
                if !seen.insert(key.clone()) {
                    return Err(invalid());
                }
                active.insert(key);
            }
            Request::Remove { .. } => {
                if !active.remove(&key) {
                    return Err(invalid());
                }
            }
            Request::Declare { .. } => {
                if !active.contains(&key) {
                    return Err(invalid());
                }
            }
        }
        events.push(e);
    }
    Ok(events)
}
/// Validate inert registration history before portable export or publication.
/// Historical receipts remain metadata; this never qualifies files or remote copies.
pub(crate) fn validate_history(tx: &Transaction<'_>) -> Result<(), Error> {
    events(tx).map(|_| ())
}

pub fn replay(
    tx: &Transaction<'_>,
    slug: &str,
    request: &Request,
) -> Result<Option<Receipt>, Error> {
    let request = request.canonical()?;
    for e in events(tx)? {
        if e.request.operation_id() == request.operation_id() {
            if e.company_slug != slug || e.request != request {
                return Err(Error::new(
                    "backup_registration_conflict",
                    "Operation already belongs to another registration intent or scope",
                ));
            }
            return Ok(Some(e.receipt));
        }
    }
    Ok(None)
}
pub fn inspect(
    tx: &Transaction<'_>,
    slug: &str,
    operation: &str,
) -> Result<Option<Receipt>, Error> {
    if !valid_id(operation) {
        return Err(invalid());
    }
    Ok(events(tx)?
        .into_iter()
        .find(|e| e.company_slug == slug && e.request.operation_id() == operation)
        .map(|e| e.receipt))
}
pub fn list(tx: &Transaction<'_>, slug: &str) -> Result<(Vec<Value>, Vec<Receipt>), Error> {
    let history: Vec<_> = events(tx)?
        .into_iter()
        .filter(|e| e.company_slug == slug)
        .map(|e| e.receipt)
        .collect();
    let removed: BTreeSet<_> = history
        .iter()
        .filter(|r| r.action == "remove")
        .map(|r| r.destination_id.as_str())
        .collect();
    let destinations = history
        .iter()
        .filter(|r| r.action == "add" && !removed.contains(r.destination_id.as_str()))
        .map(|r| r.result.clone())
        .collect();
    Ok((destinations, history))
}
pub fn apply(
    tx: &Transaction<'_>,
    slug: &str,
    request: &Request,
    actor: &str,
    at: &str,
    backup: Option<&ProtectedBackup>,
) -> Result<Receipt, Error> {
    let request = request.canonical()?;
    if let Some(receipt) = replay(tx, slug, &request)? {
        return Ok(receipt);
    }
    if text(slug, 128)? != slug
        || !actor.starts_with("user:")
        || actor.len() <= 5
        || actor.len() > 256
        || actor.chars().any(char::is_control)
        || time(at)? < 0
    {
        return Err(invalid());
    }
    let (destinations, history) = list(tx, slug)?;
    let active = destinations
        .iter()
        .any(|v| v["destinationId"] == request.destination_id());
    match &request {
        Request::Add { .. }
            if history
                .iter()
                .any(|r| r.destination_id == request.destination_id()) =>
        {
            return Err(invalid());
        }
        Request::Remove { .. } | Request::Declare { .. } if !active => return Err(invalid()),
        _ => {}
    }
    let receipt = Receipt {
        format: "rentemester-native-backup-registration-operation/1".into(),
        operation_id: request.operation_id().into(),
        action: request.action().into(),
        destination_id: request.destination_id().into(),
        actor: actor.into(),
        created_at: at.into(),
        result: result(&request, actor, at, backup)?,
    };
    let event = Event {
        format: "rentemester-native-backup-registration-event/1".into(),
        company_slug: slug.into(),
        intent_hash: intent(slug, &request),
        request,
        receipt: receipt.clone(),
    };
    tx.execute("INSERT INTO workspace_audit(event_type,entity_type,entity_id,actor,created_at)VALUES('native_backup_registration','backup_registration_operation',?1,?2,?3)",params![serde_json::to_string(&event).map_err(|_|invalid())?,actor,at]).map_err(|_|invalid())?;
    Ok(receipt)
}
#[cfg(test)]
mod tests;
