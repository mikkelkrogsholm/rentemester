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
#[derive(Clone, Copy, Debug)]
pub enum PrivacyOperation { Export, Erase, Discover, Forget, LegalHold, AuditLog }
impl PrivacyOperation {
    pub fn path(self) -> &'static str { match self { Self::Export=>"gdpr/export",Self::Erase=>"gdpr/erase",Self::Discover=>"gdpr/discover",Self::Forget=>"gdpr/forget",Self::LegalHold=>"gdpr/legal-hold",Self::AuditLog=>"gdpr/audit-log" } }
    pub fn schema(self) -> Value {
        let mut schema = privacy_body_schema(matches!(self,Self::Erase|Self::Forget|Self::LegalHold));
        if matches!(self,Self::AuditLog) { return object(json!({"since":{"type":["string","null"]},"until":{"type":["string","null"]},"asOf":{"type":["string","null"]},"confirm":{"const":true}}), &["confirm"]); }
        if matches!(self,Self::Forget) { schema["properties"]["afterRetentionExpiry"]=json!({"const":true}); schema["required"].as_array_mut().unwrap().push(json!("afterRetentionExpiry")); }
        if matches!(self,Self::LegalHold) {
            schema["properties"]["holdId"]=json!({"type":"string","minLength":1,"maxLength":128});
            schema["properties"]["reason"]=json!({"type":"string","minLength":1,"maxLength":2000});
            schema["properties"]["operation"]=json!({"enum":["place","release"],"type":"string"});
            for k in ["holdId","reason","operation"] { schema["required"].as_array_mut().unwrap().push(json!(k)); }
        }
        schema
    }
}
impl ApiClient {
    pub async fn gdpr_discover(&self, company:&str, bytes:Vec<u8>, confirmed:bool)->Result<Value,Error>{self.privacy_operation(company,PrivacyOperation::Discover,bytes,confirmed).await}
    pub async fn gdpr_forget(&self, company:&str, bytes:Vec<u8>, confirmed:bool)->Result<Value,Error>{self.privacy_operation(company,PrivacyOperation::Forget,bytes,confirmed).await}
    pub async fn gdpr_legal_hold(&self, company:&str, bytes:Vec<u8>, confirmed:bool)->Result<Value,Error>{self.privacy_operation(company,PrivacyOperation::LegalHold,bytes,confirmed).await}
    pub async fn gdpr_audit_log(&self, company:&str, bytes:Vec<u8>, confirmed:bool)->Result<Value,Error>{self.privacy_operation(company,PrivacyOperation::AuditLog,bytes,confirmed).await}
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
        self.privacy_operation(company, PrivacyOperation::Export, bytes, confirmed).await
    }
    /// Confirmed server-date scoped overlay only. Preserve complete coverage/refused/unknown facts.
    /// Original files, immutable journals and backups are outside any universal erasure claim.
    pub async fn gdpr_erase(
        &self,
        company: &str,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        self.privacy_operation(company, PrivacyOperation::Erase, bytes, confirmed).await
    }
    pub async fn privacy_operation(
        &self,
        company: &str,
        operation: PrivacyOperation,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if self.service_key.is_some() { return Err(Error::InvalidInput("privacy requires current human cookie authority")); }
        if !confirmed {
            return Err(Error::InvalidInput(
                "explicit scoped privacy confirmation required",
            ));
        }
        if bytes.len() > MAX_PRIVACY_BODY {
            return Err(Error::InvalidInput("privacy body exceeds native16KiB"));
        }
        body(&bytes, &operation.schema())?;
        let input: Value = serde_json::from_slice(&bytes)
            .map_err(|_| Error::InvalidInput("native privacy JSON required"))?;
        for key in ["asOf", "since", "until"] {
            if let Some(date) = input[key].as_str() { crate::regulatory::validate_date(date)?; }
        }
        if matches!(operation, PrivacyOperation::LegalHold) && ["holdId","reason"].iter().any(|k| input[k].as_str().is_none_or(|s|s.trim().is_empty())) { return Err(Error::InvalidInput("nonblank hold identity and reason required")); }
        if !matches!(operation, PrivacyOperation::AuditLog) && !["cvr", "name"]
            .iter()
            .any(|k| input[k].as_str().is_some_and(|s| !s.trim().is_empty()))
        {
            return Err(Error::InvalidInput(
                "at least one nonblank subject cvr/name required",
            ));
        }
        self.request(
            Method::POST,
            &company_path(company, operation.path())?,
            &[],
            Some(bytes),
        )
        .await
    }
}
