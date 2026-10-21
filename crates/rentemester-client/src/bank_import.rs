//! Original bank import transport; parsing, receipts and accounting stay on the server.
use crate::{ApiClient, Error, company_path};
use base64::{Engine, engine::general_purpose::STANDARD};
use reqwest::Method;
use serde_json::{Value, json};
use std::{fs::File, io::Read, path::Path};
/// Read bounded caller-local original bytes without decoding or temporary files.
/// Callers must establish write intent/confirmation before invoking this helper.
pub fn bank_import_file_body(
    path: &Path,
    account: Option<&str>,
    profile: Option<&str>,
) -> Result<Vec<u8>, Error> {
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|_| Error::InvalidInput("bank CSV file could not be opened"))?
        .take(25 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Error::InvalidInput("bank CSV file could not be read"))?;
    if bytes.is_empty() || bytes.len() > 25 * 1024 * 1024 {
        return Err(Error::InvalidInput(
            "bank CSV file must contain 1..25MiB original bytes",
        ));
    }
    let mut body = json!({"confirm":true,"csvBase64":STANDARD.encode(bytes)});
    if let Some(account) = account {
        body["account"] = json!(account);
    }
    if let Some(profile) = profile {
        body["profile"] = json!(profile);
    }
    serde_json::to_vec(&body).map_err(|_| Error::InvalidInput("bank import body unavailable"))
}
impl ApiClient {
    /// One confirmed request. Lost/malformed success or5xx remains OutcomeUnknown;
    /// never automatically retry an import, even when rows may deduplicate.
    pub async fn bank_import_original(&self, company: &str, body: Vec<u8>) -> Result<Value, Error> {
        crate::bank_payables::import_body(&body)?;
        let value = self
            .request(
                Method::POST,
                &company_path(company, "bank/import-original")?,
                &[],
                Some(body),
            )
            .await?;
        let valid = value["ok"].is_boolean()
            && value["errors"]
                .as_array()
                .is_some_and(|a| a.iter().all(Value::is_string))
            && value.get("code").is_none_or(Value::is_string)
            && if value["ok"] == true {
                value["data"].is_object() && value["errors"].as_array().is_some_and(Vec::is_empty)
            } else {
                value.get("data").is_none()
                    && value["errors"].as_array().is_some_and(|a| !a.is_empty())
            };
        if !valid {
            return Err(Error::OutcomeUnknown);
        }
        Ok(value)
    }
}
