use crate::tools::tool;
use rentemester_client::{ApiClient, Error};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn tools() -> Vec<Value> {
    vec![tool(
        "dashboard_export",
        "Read one standalone Danish dashboard HTML artifact. Current human company read permission; optional asOf defaults to server UTC today. Exact UTF-8 bytes, safe filename and HTML MIME, max8MiB. No local path, opener, calculations or provider effects.",
        json!({"type":"object","required":["company"],"properties":{"company":{"type":"string"},"asOf":{"type":"string","pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"}},"additionalProperties":false}),
        true,
    )]
}
pub async fn call(api: &ApiClient, args: Value) -> Result<Result<Value, Error>, &'static str> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Input {
        company: String,
        as_of: Option<String>,
    }
    let input: Input =
        serde_json::from_value(args).map_err(|_| "exact company/asOf arguments required")?;
    Ok(api
        .dashboard_export(&input.company, input.as_of.as_deref())
        .await)
}
