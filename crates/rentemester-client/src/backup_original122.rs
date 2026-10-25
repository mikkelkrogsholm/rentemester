//! Original backup diagnostics and explicit policy updates through current authority.
use crate::{
    ApiClient, Error, company_path,
    domain::{body, object},
};
use reqwest::Method;
use serde_json::{Value, json};

/// Structural validation only: domain-invalid dates/numbers reach the common engine.
pub fn system_backup_lock_original122_body_schema() -> Value {
    object(
        json!({"confirm":{"const":true},"enforced":{"type":"boolean"},"graceDays":{"type":"number"},"at":{"type":"string"}}),
        &["confirm"],
    )
}
fn nullable_string(value: Option<&Value>) -> bool {
    value.is_some_and(|v| v.is_null() || v.is_string())
}
fn valid_core(core: &Value, lock: bool) -> bool {
    let Some(ok) = core["ok"].as_bool() else {
        return false;
    };
    if core["appliedRules"] != json!(["DK-BOOKKEEPING-BACKUP-001"])
        || !core["errors"]
            .as_array()
            .is_some_and(|a| a.iter().all(Value::is_string) && (!ok || a.is_empty()) && (!lock || ok || !a.is_empty()))
    {
        return false;
    }
    if lock {
        return if ok {
            let c = &core["config"];
            c["version"] == "backup-lock-v1"
                && c["enforced"].is_boolean()
                && c["graceDays"]
                    .as_u64()
                    .is_some_and(|n| n <= u32::MAX as u64)
                && c["updatedAt"].is_string()
                && c["updatedBy"].is_string()
        } else {
            core.get("config").is_none()
        };
    }
    ["latestBackupAt", "latestBackupId", "requiredBy"]
        .iter()
        .all(|k| nullable_string(core.get(*k)))
        && core["backupDue"].is_boolean()
        && core["hasActivitySinceBackup"].is_boolean()
        && core
            .get("daysSinceLatestBackup")
            .is_some_and(|v| v.is_null() || v.is_number())
        && core["checkedAt"].is_string()
        && core["backupsFound"].as_u64().is_some()
        && [
            "latestJournalEntryAt",
            "latestDocumentAt",
            "latestBankImportAt",
        ]
        .iter()
        .all(|k| nullable_string(core["evidence"].get(*k)))
}
fn extract(value: Value, field: &str, lock: bool) -> Result<Value, Error> {
    if value["ok"] != true || !valid_core(&value[field], lock) {
        return Err(Error::Protocol);
    }
    Ok(value[field].clone())
}
impl ApiClient {
    pub async fn system_backup_status_original122(
        &self,
        company: &str,
        as_of: Option<&str>,
    ) -> Result<Value, Error> {
        let company = self.diagnostics_company_original118(company)?;
        let query = as_of.map(|s| vec![("asOf", s)]).unwrap_or_default();
        extract(
            self.get(&company_path(&company, "system-backup-status")?, &query)
                .await?,
            "backupStatus",
            false,
        )
    }
    /// Sends the exact confirmed bytes once. Never retry an unknown policy append.
    pub async fn system_backup_lock_original122(
        &self,
        company: &str,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if !confirmed {
            return Err(Error::InvalidInput(
                "explicit backup lock confirmation required",
            ));
        }
        if bytes.len() > 4096 {
            return Err(Error::InvalidInput("backup lock body exceeds4KiB"));
        }
        body(&bytes, &system_backup_lock_original122_body_schema())?;
        let company = self.diagnostics_company_original118(company)?;
        let value = self
            .request(
                Method::POST,
                &company_path(&company, "system-backup-lock")?,
                &[],
                Some(bytes),
            )
            .await?;
        // A malformed success cannot establish whether a mutation committed.
        extract(value, "backupLock", true).map_err(|_| Error::OutcomeUnknown)
    }
}
