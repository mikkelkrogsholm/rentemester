//! Source-flat exception transport. Native methods and their contracts are unchanged.
use crate::{ApiClient, Error, MAX_RESPONSE, company_path};
use reqwest::Method;
use serde_json::Value;

const MAX_ID: i64 = 9_007_199_254_740_991;
fn id(v: &Value) -> bool { v.as_i64().is_some_and(|n| (1..=MAX_ID).contains(&n)) }
fn text(v: &Value) -> bool { v.as_str().is_some_and(|s| !s.trim().is_empty()) }
fn nullable_text(v: &Value) -> bool { v.is_null() || v.is_string() }
fn keys(v: &Value, allowed: &[&str]) -> bool {
    v.as_object().is_some_and(|o| o.keys().all(|k| allowed.contains(&k.as_str())))
}
fn row(v: &Value) -> bool {
    const FIELDS: &[&str] = &["id","type","severity","status","relatedBankTransactionId","relatedDocumentId","message","requiredAction","sourceEvidence","postingPreview","createdAt","resolvedAt","resolvedBy","resolutionNote","archived"];
    v.as_object().is_some_and(|o| o.len() == FIELDS.len()) && keys(v,FIELDS)
        && id(&v["id"]) && text(&v["type"]) && text(&v["severity"])
        && matches!(v["status"].as_str(),Some("open" | "resolved" | "ignored"))
        && ["relatedBankTransactionId","relatedDocumentId"].iter().all(|k| v[*k].is_null() || id(&v[*k]))
        && v["message"].is_string() && nullable_text(&v["requiredAction"])
        && text(&v["createdAt"]) && ["resolvedAt","resolvedBy","resolutionNote"].iter().all(|k| nullable_text(&v[*k]))
        && v["archived"].is_boolean()
}
fn valid(v: &Value, write: bool, strict: bool, status: Option<&str>, archived: bool) -> bool {
    let allowed = if write { &["ok","resolved","errors","code","appliedRules"][..] } else { &["ok","count","rows","errors","code","appliedRules"][..] };
    if !keys(v,allowed) || !v["ok"].is_boolean()
        || !v["errors"].as_array().is_some_and(|a| a.iter().all(text))
        || v.get("code").is_some_and(|c| !text(c))
        || v.get("appliedRules").is_some_and(|r| !r.as_array().is_some_and(|a| a.iter().all(text))) { return false; }
    if v["ok"] == false {
        return !v["errors"].as_array().unwrap().is_empty() && (!strict || text(&v["code"]))
            && if write { v["resolved"] == false } else { v["count"] == 0 && v["rows"].as_array().is_some_and(Vec::is_empty) };
    }
    if strict || !v["errors"].as_array().unwrap().is_empty() { return false; }
    if write { return v["resolved"].is_boolean(); }
    v["rows"].as_array().is_some_and(|rows| {
        v["count"].as_u64() == Some(rows.len() as u64)
            && rows.iter().all(|r| row(r) && (archived || r["archived"] == false) && (status == Some("all") || r["status"] == status.unwrap_or("open")))
            && rows.windows(2).all(|p| p[0]["id"].as_i64() > p[1]["id"].as_i64())
    })
}
impl ApiClient {
    pub async fn exceptions_original_list(&self, company: &str, status: Option<&str>, include_archived: bool) -> Result<Value, Error> {
        // Bounded unknown statuses are business refusals, not local usage errors.
        let mut query = vec![("includeArchived", if include_archived { "true" } else { "false" })];
        if let Some(s) = status { query.push(("status",s)); }
        self.exceptions_original_request(&company_path(company,"exceptions/original")?, &query, None, status, include_archived).await
    }
    pub async fn exceptions_original_resolve(&self, company: &str, id: i64, bytes: Vec<u8>, confirmed: bool) -> Result<Value, Error> {
        if !confirmed || !(1..=MAX_ID).contains(&id) { return Err(Error::InvalidInput("confirmed positive safe exception id required")); }
        if bytes.len() > crate::MAX_EXCEPTION_BODY { return Err(Error::InvalidInput("exception resolution exceeds native16KiB body")); }
        let body: Value = serde_json::from_slice(&bytes).map_err(|_| Error::InvalidInput("invalid exception resolution body"))?;
        if !keys(&body,&["note"]) || body.get("note").is_some_and(|n| n.as_str().is_none_or(|s| s.len() > 4096)) { return Err(Error::InvalidInput("only optional string note supported")); }
        self.exceptions_original_request(&company_path(company,&format!("exceptions/original/{id}/resolve"))?, &[], Some(bytes), None, false).await
    }
    async fn exceptions_original_request(&self, path: &str, query: &[(&str,&str)], body: Option<Vec<u8>>, filter: Option<&str>, archived: bool) -> Result<Value,Error> {
        let write = body.is_some();
        let mut response = self.send(if write { Method::POST } else { Method::GET },path,query,body,self.cookie.as_ref().map(|c| c.as_str())).await?;
        let status = response.status();
        if write && (status.is_server_error() || status.as_u16() == 408) { return Err(Error::OutcomeUnknown); }
        let strict = status.as_u16() == 422;
        if !status.is_success() && !strict { return Err(Error::Refused{status:status.as_u16()}); }
        let protocol = || if strict { Error::Refused{status:422} } else if write { Error::OutcomeUnknown } else { Error::Protocol };
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| if strict { protocol() } else if write { Error::OutcomeUnknown } else { Error::Transport })? {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE { return Err(protocol()); }
            bytes.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| protocol())?;
        if valid(&value,write,strict,filter,archived) { Ok(value) } else { Err(protocol()) }
    }
}
