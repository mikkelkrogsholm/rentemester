//! Read-only exact CFO selectors. No calculations, currency conversion or inferred sources.
use crate::{ApiClient, Error, MAX_RESPONSE};
use reqwest::Method;
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CfoAnalyticsQuery {
    pub scope: String,
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub company_slugs: Vec<String>,
    pub group_profile_id: Option<String>,
    pub account: Option<String>,
    pub party: Option<String>,
    pub dimension: Option<String>,
    pub currency: Option<String>,
    pub aggregate: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<u16>,
}
pub fn cfo_analytics_schema() -> Value {
    json!({"type":"object","properties":{"scope":{"type":"string","enum":["company","portfolio","group"]},"from":{"type":"string","minLength":1},"to":{"type":"string","minLength":1},"companySlugs":{"type":"array","maxItems":1000,"items":{"type":"string","minLength":1},"description":"Exact requested companies, encoded as repeated companySlug. Company scope requires one; omission on portfolio/group leaves selection to server."},"groupProfileId":{"type":"string"},"account":{"type":"string"},"party":{"type":"string"},"dimension":{"type":"string"},"currency":{"type":"string"},"aggregate":{"type":"string","enum":["none","sum"],"description":"Server default none. Explicit sum never grants visibility or consolidates legal companies."},"cursor":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":200}},"required":["scope","from","to"],"additionalProperties":false})
}
impl ApiClient {
    pub async fn cfo_analytics(&self, input: &CfoAnalyticsQuery) -> Result<Value, Error> {
        if !["company", "portfolio", "group"].contains(&input.scope.as_str())
            || input.from.is_empty()
            || input.to.is_empty()
            || input.company_slugs.len() > 1000
            || input.company_slugs.iter().any(String::is_empty)
            || input.limit.is_some_and(|n| !(1..=200).contains(&n))
            || input
                .aggregate
                .as_deref()
                .is_some_and(|s| !["none", "sum"].contains(&s))
        {
            return Err(Error::InvalidInput("exact explicit CFO selectors required"));
        }
        let mut fields = vec![
            ("scope", input.scope.clone()),
            ("from", input.from.clone()),
            ("to", input.to.clone()),
        ];
        fields.extend(
            input
                .company_slugs
                .iter()
                .cloned()
                .map(|s| ("companySlug", s)),
        );
        for (name, value) in [
            ("groupProfileId", &input.group_profile_id),
            ("account", &input.account),
            ("party", &input.party),
            ("dimension", &input.dimension),
            ("currency", &input.currency),
            ("aggregate", &input.aggregate),
            ("cursor", &input.cursor),
        ] {
            if let Some(value) = value {
                fields.push((name, value.clone()));
            }
        }
        if let Some(limit) = input.limit {
            fields.push(("limit", limit.to_string()));
        }
        let pairs: Vec<_> = fields.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let mut response = self
            .send(
                Method::GET,
                "/api/cfo-analytics",
                &pairs,
                None,
                self.cookie.as_ref().map(|v| v.as_str()),
            )
            .await?;
        let status = response.status();
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| Error::Transport)? {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE {
                return Err(Error::Protocol);
            }
            bytes.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| {
            if status.is_success() {
                Error::Protocol
            } else {
                Error::Refused {
                    status: status.as_u16(),
                }
            }
        })?;
        if status.is_success() && value["ok"] == true
            || value["ok"] == false && value["code"].as_str().is_some_and(|s| !s.is_empty())
        {
            Ok(value)
        } else if status.is_success() {
            Err(Error::Protocol)
        } else {
            Err(Error::Refused {
                status: status.as_u16(),
            })
        }
    }
}
