//! Explicit sensitive workspace export only; no operator keys or filesystem authority.
use crate::tools::tool;
use rentemester_client::{ApiClient, Error};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn tools() -> Vec<Value> {
    vec![tool(
        "workspace_backup",
        "Explicitly confirm one POST of exact {confirm:true} to native /api/workspace/backup. Requires current WorkspaceManage and CompanyAdmin for every registered company. Returns exact native artifact bytes in {backup:{contentType,bytes}}, max96MiB. Sensitive full-workspace accounting/original evidence; unsupported coverage or unavailable operator signing configuration may refuse. No company/year/id/body/path/key selectors, local files, verification/restore claim, automatic retry or new accounting entries. Hold trusted public key separately and use operator server CLI for independent verification/fresh restore.",
        json!({"type":"object","required":["confirm"],"properties":{"confirm":{"const":true}},"additionalProperties":false}),
        false,
    )]
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    confirm: bool,
}
pub async fn call(api: &ApiClient, args: Value) -> Result<Result<Value, Error>, &'static str> {
    let input: Input = serde_json::from_value(args)
        .map_err(|_| "workspace_backup requires only confirm:true; no selectors/body/path/key")?;
    Ok(api.workspace_backup(input.confirm).await)
}
