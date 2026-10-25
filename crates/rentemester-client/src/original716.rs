//! Original structure overview over the common authenticated protocol transport.
use crate::{ApiClient, Error};
use serde_json::Value;
impl ApiClient {
    pub async fn group_overview_original716(&self, as_of: &str) -> Result<Value, Error> {
        if !crate::valid_commitment_start(as_of) {
            return Err(Error::InvalidInput("asOf must be a real YYYY-MM-DD date"));
        }
        let value = self.get("/api/group/overview", &[("asOf", as_of)]).await?;
        if value["ok"] != true || value["asOf"] != as_of
            || value["scope"] != "structure-status-only"
            || value["consolidationStatus"] != "not-available"
            || !value.get("consolidatedFigures").is_some_and(Value::is_null)
            || !value.get("rawCompanySums").is_some_and(Value::is_null)
            || !matches!(value["manifestStatus"].as_str(), Some("not-configured" | "ready" | "blocked"))
            || !value["blockers"].as_array().is_some_and(|a| a.iter().all(Value::is_string))
            || !value["groups"].as_array().is_some_and(|a| a.iter().all(|g| {
                g["partial"].is_boolean()
                    && matches!(g["readiness"].as_str(), Some("ready" | "blocked"))
                    && g["visibleMemberships"].as_array().is_some_and(|a| !a.is_empty() && a.iter().all(|m| m["archived"].is_boolean() && m["companySlug"].is_string()))
                    && g["visibleOwnership"].is_array()
                    && g["blockers"].as_array().is_some_and(|a| a.iter().all(Value::is_string))
                    && if g["partial"] == false { g["id"].is_string() && g["name"].is_string() } else { g.get("id").is_none() && g.get("name").is_none() }
            })) {
            return Err(Error::Protocol);
        }
        Ok(value)
    }
}
