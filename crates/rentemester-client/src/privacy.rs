//! Audited scoped subject export/overlay, never a universal physical-deletion guarantee.
use crate::{
    ApiClient, Error, company_path,
    domain::{body, object},
};
use reqwest::Method;
use serde_json::{Value, json};
pub const MAX_PRIVACY_BODY: usize = 16 * 1024;
pub fn privacy_body_schema(erasure: bool) -> Value {
    let subject = json!({"type":["string","null"],"description":"Exact subject identity, at least one nonblank cvr/name; both supplied must match the same server subject"});
    let mut fields = json!({"cvr":subject,"name":subject,"confirm":{"const":true}});
    if !erasure {
        fields["asOf"] = json!({"type":["string","null"],"description":"Optional export review date only; never authorizes erasure at that date"});
    }
    object(fields, &["confirm"])
}
impl ApiClient {
    pub async fn retention(&self, company: &str) -> Result<Value, Error> {
        self.get(&company_path(company, "retention")?, &[]).await
    }
    /// Audited export may strengthen subject aliases; exact bytes once, no keys/retries.
    pub async fn gdpr_export(
        &self,
        company: &str,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        self.privacy_request(company, bytes, confirmed, false).await
    }
    /// Confirmed server-date scoped overlay only. Preserve complete coverage/refused/unknown facts.
    /// Original files, immutable journals and backups are outside any universal erasure claim.
    pub async fn gdpr_erase(
        &self,
        company: &str,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        self.privacy_request(company, bytes, confirmed, true).await
    }
    async fn privacy_request(
        &self,
        company: &str,
        bytes: Vec<u8>,
        confirmed: bool,
        erasure: bool,
    ) -> Result<Value, Error> {
        if !confirmed {
            return Err(Error::InvalidInput(
                "explicit scoped privacy confirmation required",
            ));
        }
        if bytes.len() > MAX_PRIVACY_BODY {
            return Err(Error::InvalidInput("privacy body exceeds native16KiB"));
        }
        body(&bytes, &privacy_body_schema(erasure))?;
        let input: Value = serde_json::from_slice(&bytes)
            .map_err(|_| Error::InvalidInput("native privacy JSON required"))?;
        if !["cvr", "name"]
            .iter()
            .any(|k| input[k].as_str().is_some_and(|s| !s.trim().is_empty()))
        {
            return Err(Error::InvalidInput(
                "at least one nonblank subject cvr/name required",
            ));
        }
        self.request(
            Method::POST,
            &company_path(company, if erasure { "gdpr/erase" } else { "gdpr/export" })?,
            &[],
            Some(bytes),
        )
        .await
    }
}
