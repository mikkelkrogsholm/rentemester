use crate::tools::{native_body, tool};
use rentemester_client::{
    ApiClient, Error, MAX_COMPANY_CREATION_BODY, company_creation_body_schema,
};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn tools() -> Vec<Value> {
    let mut body = native_body(company_creation_body_schema());
    body["maxLength"] = json!(MAX_COMPANY_CREATION_BODY);
    vec![tool(
        "create_company",
        "WorkspaceManage: create one fresh separate company ledger and explicit creator owner grant. Exact native16KiB body, outer confirm:true only; no company selector or inner confirm/key. Server validates namespace/current rights and owns slug/calendar/VAT defaults. No copied chart/balances, inferred mappings or automatic retry. Unknown publication outcome requires companies/current profile reads and operator reconciliation before another attempt; duplicate refusal is no replay receipt.",
        json!({"type":"object","required":["body","confirm"],"properties":{"body":body,"confirm":{"const":true}},"additionalProperties":false}),
        false,
    )]
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    body: String,
    confirm: bool,
}
pub async fn call(api: &ApiClient, args: Value) -> Result<Result<Value, Error>, &'static str> {
    let input: Input = serde_json::from_value(args)
        .map_err(|_| "create_company requires only body and confirm:true")?;
    if input.body.len() > MAX_COMPANY_CREATION_BODY {
        return Err("company creation exceeds native16KiB");
    }
    Ok(api
        .create_company(input.body.into_bytes(), input.confirm)
        .await)
}
