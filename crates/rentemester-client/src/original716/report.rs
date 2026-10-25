//! Local output paths never cross this report transport boundary.
use crate::{ApiClient, Error, company_path};
use serde_json::Value;
impl ApiClient {
    pub async fn original716_compliance(
        &self,
        company: &str,
        input: &Value,
    ) -> Result<Value, Error> {
        if !input.is_object()
            || input.as_object().unwrap().iter().any(|(k, v)| {
                !["asOf", "generatedAt", "commitSha", "expectedActor"].contains(&k.as_str())
                    || !v.is_string()
            })
        {
            return Err(Error::InvalidInput("invalid report metadata"));
        }
        if let Some(date) = input["asOf"].as_str() {
            crate::regulatory::validate_date(date)?;
        }
        let value = self
            .request(
                reqwest::Method::POST,
                &company_path(company, "original716-compliance-report")?,
                &[],
                Some(serde_json::to_vec(input).map_err(|_| Error::Protocol)?),
            )
            .await?;
        if value["ok"] != true || !value["result"]["ok"].is_boolean() {
            return Err(Error::Protocol);
        }
        let result = &value["result"];
        if result["ok"] == true
            && (!result["html"].is_string()
                || !result["fingerprint"].is_string()
                || !result["generatedAt"].is_string()
                || !result["asOfDate"].is_string())
        {
            return Err(Error::Protocol);
        }
        Ok(result.clone())
    }
}
impl ApiClient {
    pub async fn original716_remaining(
        &self,
        company: &str,
        operation: &str,
        input: &Value,
    ) -> Result<Value, Error> {
        if !["retention", "export", "erase"].contains(&operation) {
            return Err(Error::InvalidInput("Unknown remaining operation"));
        }
        let v = self
            .request(
                reqwest::Method::POST,
                &company_path(company, &format!("original716-{operation}"))?,
                &[],
                Some(serde_json::to_vec(input).map_err(|_| Error::Protocol)?),
            )
            .await?;
        if v["ok"] != true || !v["result"]["ok"].is_boolean() {
            return Err(Error::Protocol);
        }
        Ok(v["result"].clone())
    }
}
