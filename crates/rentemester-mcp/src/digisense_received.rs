//! Exact native receive body; server owns intake/authority/dedup.

use rentemester_client::{ApiClient, Error};

use serde_json::Value;
pub fn accepts(name: &str) -> bool {
    matches!(name, "efaktura_modtag" | "efaktura_modtag_workspace")
}
pub fn tools() -> Vec<Value> {
    crate::public_einvoice::original717::tools()
        .into_iter()
        .filter(|t| {
            matches!(
                t["name"].as_str(),
                Some("efaktura_modtag" | "efaktura_modtag_workspace")
            )
        })
        .collect()
}
pub async fn call(
    api: &ApiClient,
    name: &str,
    args: Value,
) -> Result<Result<Value, Error>, &'static str> {
    crate::public_einvoice::original717::call(api, name, args).await
}

pub use crate::public_einvoice::original717_tool_result;
