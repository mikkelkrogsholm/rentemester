//! Server-owned closed-year tax preparation and deterministic iXBRL. No local files or accounting.
use crate::{ApiClient, Error, company_path, valid_annual_bounds};
use reqwest::Method;
use serde_json::Value;
use sha2::{Digest, Sha256};
impl ApiClient {
    pub async fn annual_export(
        &self,
        company: &str,
        start: &str,
        end: &str,
        kind: &str,
    ) -> Result<Value, Error> {
        if !valid_annual_bounds(start, end) || !matches!(kind, "tax" | "ixbrl") {
            return Err(Error::InvalidInput(
                "annual export requires ascending real fiscal dates and kind tax|ixbrl",
            ));
        }
        let mut response = self
            .send(
                Method::GET,
                &company_path(company, "annual-exports")?,
                &[
                    ("fiscalYearStart", start),
                    ("fiscalYearEnd", end),
                    ("kind", kind),
                ],
                None,
                self.cookie.as_ref().map(|c| c.as_str()),
            )
            .await?;
        if !response.status().is_success() {
            return Err(Error::Refused {
                status: response.status().as_u16(),
            });
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| Error::Transport)? {
            if bytes.len().saturating_add(chunk.len()) > 96 * 1024 * 1024 {
                return Err(Error::Protocol);
            }
            bytes.extend_from_slice(&chunk);
        }
        let response: Value = serde_json::from_slice(&bytes).map_err(|_| Error::Protocol)?;
        if response["ok"] != true {
            return Err(Error::Protocol);
        }
        let value = response
            .get("annualExport")
            .filter(|v| v.is_object())
            .ok_or(Error::Protocol)?;
        if !value["ok"].is_boolean()
            || !value["errors"]
                .as_array()
                .is_some_and(|a| a.iter().all(Value::is_string))
            || value["ok"] == false && value["errors"].as_array().is_none_or(Vec::is_empty)
        {
            return Err(Error::Protocol);
        }
        if kind == "ixbrl" && value["ok"] == true {
            let xhtml = value["xhtml"].as_str().ok_or(Error::Protocol)?;
            let sha = value["sha256"].as_str().ok_or(Error::Protocol)?;
            if xhtml.is_empty() || sha != format!("{:x}", Sha256::digest(xhtml.as_bytes())) {
                return Err(Error::Protocol);
            }
            if !value["annualReport"].is_object()
                || value["annualReport"]["ok"] != true
                || !value["taxonomy"].is_object()
                || !value["appliedRules"].is_array()
            {
                return Err(Error::Protocol);
            }
        }
        Ok(value.clone())
    }
}
