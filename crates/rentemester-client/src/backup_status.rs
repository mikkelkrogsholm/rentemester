//! Current human-admin backup freshness and opt-in lock; never cloud compliance.
use crate::{
    ApiClient, Error, company_path,
    domain::{body, object},
};
use reqwest::Method;
use serde_json::{Value, json};
pub const MAX_BACKUP_LOCK_BODY: usize = 4096;
pub fn backup_lock_body_schema() -> Value {
    object(
        json!({"confirm":{"const":true},"enforced":{"type":"boolean"},"graceDays":{"type":"integer","minimum":0,"maximum":365}}),
        &["confirm", "enforced", "graceDays"],
    )
}
impl ApiClient {
    pub async fn backup_status(&self, company: &str) -> Result<Value, Error> {
        self.get(&company_path(company, "backup-status")?, &[])
            .await
    }
    /// Exact body once. Unknown outcomes require explicit backup_status readback.
    pub async fn configure_backup_lock(
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
        if bytes.len() > MAX_BACKUP_LOCK_BODY {
            return Err(Error::InvalidInput("backup lock body exceeds4KiB"));
        }
        body(&bytes, &backup_lock_body_schema())?;
        self.request(
            Method::POST,
            &company_path(company, "backup-lock")?,
            &[],
            Some(bytes),
        )
        .await
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_confirmed_policy_schema_rejects_unknowns_and_out_of_range_grace() {
        for valid in [
            br#"{"confirm":true,"enforced":false,"graceDays":0}"#.as_slice(),
            br#"{"confirm":true,"enforced":true,"graceDays":365}"#,
        ] {
            assert!(body(valid, &backup_lock_body_schema()).is_ok());
        }
        for invalid in [
            br#"{"confirm":false,"enforced":true,"graceDays":0}"#.as_slice(),
            br#"{"confirm":true,"enforced":true,"graceDays":366}"#,
            br#"{"confirm":true,"enforced":true,"graceDays":-1}"#,
            br#"{"confirm":true,"enforced":true,"graceDays":0,"at":"2020-01-01"}"#,
        ] {
            assert!(body(invalid, &backup_lock_body_schema()).is_err());
        }
    }
}
