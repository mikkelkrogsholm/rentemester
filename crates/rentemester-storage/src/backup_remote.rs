//! Immutable successful remote observations; never current provider availability or offsite compliance.
use rentemester_engine::Error;
use rusqlite::{Transaction, params};
use serde::{Deserialize, Serialize};

pub const FORMAT: &str = "rentemester-native-remote-backup-verification/1";
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Receipt {
    pub format: String,
    pub destination_id: String,
    pub operation_id: String,
    pub placement_operation_id: String,
    pub remote_object_id: String,
    pub archive_sha256: String,
    pub archive_size_bytes: u64,
    pub backup_created_at: String,
    pub provider: String,
    pub remote_file_name: String,
    pub remote_parent_id: String,
    pub observed_at: String,
    pub verified_at: String,
    pub verified_by: String,
    pub remote_sync_verified: bool,
    pub verify_method: String,
    pub signature_verified: bool,
    pub attestation_independently_verified: bool,
}
fn invalid() -> Error {
    Error::new(
        "backup_remote_receipt_unavailable",
        "Remote backup receipt could not be read safely",
    )
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
fn timestamp(value: &str) -> Result<i128, Error> {
    time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
        .map(|v| v.unix_timestamp_nanos())
        .map_err(|_| invalid())
}
impl Receipt {
    pub fn validate(&self) -> Result<(), Error> {
        if self.format != FORMAT
            || !id(&self.destination_id)
            || !id(&self.operation_id)
            || !id(&self.placement_operation_id)
            || !object(&self.remote_object_id)
            || !object(&self.remote_parent_id)
            || self.archive_sha256.len() != 64
            || !self
                .archive_sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || self.archive_size_bytes == 0
            || self.archive_size_bytes > 96 * 1024 * 1024
            || self.provider != "google-drive"
            || self.remote_file_name
                != format!("{}.native-backup.json", self.placement_operation_id)
            || !self.verified_by.starts_with("user:")
            || self.verified_by.len() <= 5
            || self.verified_by.len() > 256
            || self.verified_by.chars().any(char::is_control)
            || !self.remote_sync_verified
            || !self.signature_verified
            || self.attestation_independently_verified
            || self.verify_method != "google-drive-metadata-and-sha256-reread"
        {
            return Err(invalid());
        }
        let backup = timestamp(&self.backup_created_at)?;
        let observed = timestamp(&self.observed_at)?;
        let verified = timestamp(&self.verified_at)?;
        if backup < 0
            || observed < backup
            || verified < observed
            || verified - observed > 300_000_000_000
        {
            return Err(invalid());
        }
        Ok(())
    }
}
pub fn boundary(tx: &Transaction<'_>) -> Result<i64, Error> {
    tx.query_row("SELECT COALESCE(MAX(id),0) FROM workspace_audit WHERE event_type='native_portable_restore'", [], |row| row.get(0)).map_err(|_| invalid())
}
/// Scope is this admitted workspace-control DB. Imported historical rows are not active receipts.
pub fn inspect(
    tx: &Transaction<'_>,
    destination: &str,
    operation: &str,
) -> Result<Option<Receipt>, Error> {
    if !id(destination) || !id(operation) {
        return Err(invalid());
    }
    let mut query = tx.prepare("SELECT entity_id FROM workspace_audit WHERE event_type='native_backup_remote_verified' AND entity_type='native_backup_remote_verification' AND id>?1 ORDER BY id").map_err(|_| invalid())?;
    let mut result = None;
    for row in query
        .query_map([boundary(tx)?], |row| row.get::<_, String>(0))
        .map_err(|_| invalid())?
    {
        let wire = row.map_err(|_| invalid())?;
        if wire.len() > 16384 {
            return Err(invalid());
        }
        let value: Receipt = serde_json::from_str(&wire).map_err(|_| invalid())?;
        value.validate()?;
        if value.destination_id == destination && value.operation_id == operation {
            if result.as_ref().is_some_and(|prior| prior != &value) {
                return Err(invalid());
            }
            result = Some(value);
        }
    }
    Ok(result)
}
pub fn append(tx: &Transaction<'_>, receipt: &Receipt, actor: &str) -> Result<Receipt, Error> {
    receipt.validate()?;
    if actor != receipt.verified_by {
        return Err(invalid());
    }
    if let Some(prior) = inspect(tx, &receipt.destination_id, &receipt.operation_id)? {
        if prior != *receipt {
            return Err(Error::new(
                "backup_remote_conflict",
                "Verification operation already identifies another completed observation",
            ));
        }
        return Ok(prior);
    }
    let wire = serde_json::to_string(receipt).map_err(|_| invalid())?;
    tx.execute("INSERT INTO workspace_audit(event_type,entity_type,entity_id,actor,created_at)VALUES('native_backup_remote_verified','native_backup_remote_verification',?1,?2,?3)", params![wire, actor, receipt.verified_at]).map_err(|_| invalid())?;
    Ok(receipt.clone())
}
#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn receipt() -> Receipt {
        Receipt {
            format: FORMAT.into(),
            destination_id: "synthetic-drive".into(),
            operation_id: "synthetic-remote".into(),
            placement_operation_id: "synthetic-placement".into(),
            remote_object_id: "Synthetic_Object".into(),
            archive_sha256: "a".repeat(64),
            archive_size_bytes: 123,
            backup_created_at: "2026-01-01T00:00:00.000Z".into(),
            provider: "google-drive".into(),
            remote_file_name: "synthetic-placement.native-backup.json".into(),
            remote_parent_id: "Synthetic_Folder".into(),
            observed_at: "2026-01-01T00:01:00.000Z".into(),
            verified_at: "2026-01-01T00:02:00.000Z".into(),
            verified_by: "user:synthetic".into(),
            remote_sync_verified: true,
            verify_method: "google-drive-metadata-and-sha256-reread".into(),
            signature_verified: true,
            attestation_independently_verified: false,
        }
    }
    #[test]
    fn exact_receipt_is_immutable_scoped_and_inactive_after_restore() {
        let mut control = rusqlite::Connection::open_in_memory().unwrap();
        let mut company = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut company, "Synthetic remote backup")
            .unwrap();
        let tx = control.transaction().unwrap();
        let value = receipt();
        assert!(
            inspect(&tx, &value.destination_id, &value.operation_id)
                .unwrap()
                .is_none()
        );
        append(&tx, &value, &value.verified_by).unwrap();
        let changes = tx.total_changes();
        assert_eq!(append(&tx, &value, &value.verified_by).unwrap(), value);
        assert_eq!(tx.total_changes(), changes);
        assert!(
            inspect(&tx, "another-destination", &value.operation_id)
                .unwrap()
                .is_none()
        );
        let mut changed = value.clone();
        changed.remote_object_id = "Other_Object".into();
        assert_eq!(
            append(&tx, &changed, &value.verified_by).unwrap_err().code,
            "backup_remote_conflict"
        );
        tx.execute("INSERT INTO workspace_audit(event_type,entity_type,entity_id,actor)VALUES('native_portable_restore','backup','synthetic','system:restore')", []).unwrap();
        assert!(
            inspect(&tx, &value.destination_id, &value.operation_id)
                .unwrap()
                .is_none()
        );
        append(&tx, &changed, &value.verified_by).unwrap();
        assert_eq!(
            inspect(&tx, &value.destination_id, &value.operation_id).unwrap(),
            Some(changed)
        );
    }
    #[test]
    fn false_claims_changed_digest_stale_time_and_actor_refuse() {
        let mut value = receipt();
        value.attestation_independently_verified = true;
        assert!(value.validate().is_err());
        let mut value = receipt();
        value.archive_sha256 = "A".repeat(64);
        assert!(value.validate().is_err());
        let mut value = receipt();
        value.verified_at = "2026-01-01T00:07:00.001Z".into();
        assert!(value.validate().is_err());
        let mut value = receipt();
        value.remote_file_name = "caller-path".into();
        assert!(value.validate().is_err());
        let mut c = rusqlite::Connection::open_in_memory().unwrap();
        let tx = c.transaction().unwrap();
        assert!(append(&tx, &receipt(), "agent:caller").is_err());
    }
}
