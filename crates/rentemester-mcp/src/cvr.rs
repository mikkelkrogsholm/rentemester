//! Explicit CVR provider/cache operations; all credentials stay with the operator.
use crate::tools::{company_schema, tool};
use rentemester_client::{ApiClient, Error};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn accepts(name: &str) -> bool {
    matches!(name, "cvr_status" | "cvr_lookup" | "company_sync_cvr")
}
pub fn tools() -> Vec<Value> {
    vec![
        tool(
            "cvr_status",
            "Read authenticated workspace CVR credential-presence status only; no provider or cache effects and no credential values.",
            json!({"type":"object","properties":{},"additionalProperties":false}),
            true,
        ),
        tool(
            "cvr_lookup",
            "Explicit CompanyExternalLookup: look up caller CVR text through the operator provider or retained company-scoped cache. Can retain a raw CVR snapshot; confirmed operation, not read-only. Exact inner business result/errors/fetchedAt; no caller credentials/endpoint, forced refresh, retries or automatic contact creation.",
            json!({"type":"object","required":["company","cvr","confirm"],"properties":{"company":company_schema(),"cvr":{"type":"string","maxLength":128},"confirm":{"const":true}},"additionalProperties":false}),
            false,
        ),
        tool(
            "company_sync_cvr",
            "Explicit current CompanyMasterData and CompanyExternalLookup: synchronize the company's own current CVR master data and audit the action. Never changes fiscal year, payment settings or accounting. Returns exact inner business result including mismatch warning and errors. No caller source data, credentials, keys or automatic retry; read canonical profile after uncertainty.",
            json!({"type":"object","required":["company","confirm"],"properties":{"company":company_schema(),"confirm":{"const":true}},"additionalProperties":false}),
            false,
        ),
    ]
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Lookup {
    company: String,
    cvr: String,
    confirm: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Sync {
    company: String,
    confirm: bool,
}
pub async fn call(
    api: &ApiClient,
    name: &str,
    args: Value,
) -> Result<Result<Value, Error>, &'static str> {
    match name {
        "cvr_status" => {
            serde_json::from_value::<Empty>(args).map_err(|_| "cvr_status accepts no arguments")?;
            Ok(api.cvr_status().await)
        }
        "cvr_lookup" => {
            let a: Lookup = serde_json::from_value(args)
                .map_err(|_| "cvr_lookup requires only company/cvr/confirm:true")?;
            Ok(api.cvr_lookup(&a.company, &a.cvr, a.confirm).await)
        }
        "company_sync_cvr" => {
            let a: Sync = serde_json::from_value(args)
                .map_err(|_| "company_sync_cvr requires only company/confirm:true")?;
            Ok(api.sync_company_cvr(&a.company, a.confirm).await)
        }
        _ => Err("unknown CVR tool"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_status_is_read_only_and_provider_operations_require_flat_confirmation() {
        let t = tools();
        assert_eq!(t.len(), 3);
        assert_eq!(t[0]["annotations"]["readOnlyHint"], true);
        for t in &t[1..] {
            assert_eq!(t["annotations"]["readOnlyHint"], false);
            assert_eq!(t["inputSchema"]["properties"]["confirm"]["const"], true);
            assert_eq!(t["inputSchema"]["additionalProperties"], false);
        }
    }
    #[test]
    fn strict_arguments_and_unconfirmed_requests_never_reach_network() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
            for (name,args) in [
                ("cvr_status",json!({"company":"synthetic"})),
                ("cvr_lookup",json!({"company":"synthetic","cvr":"12345678","confirm":true,"endpoint":"forged"})),
                ("cvr_lookup",json!({"company":"synthetic","cvr":null,"confirm":true})),
                ("company_sync_cvr",json!({"company":"synthetic","confirm":true,"actor":"forged"})),
            ] { assert!(call(&api,name,args).await.is_err()); }
            for (name,args) in [
                ("cvr_lookup",json!({"company":"synthetic","cvr":"12345678","confirm":false})),
                ("company_sync_cvr",json!({"company":"synthetic","confirm":false})),
            ] { assert!(matches!(call(&api,name,args).await.unwrap(),Err(Error::InvalidInput(_)))); }
        });
    }
}
