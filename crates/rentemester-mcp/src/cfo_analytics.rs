//! Canonical read-only CFO projection; no client aggregation or visibility inference.
use crate::tools::tool;
use rentemester_client::{ApiClient, CfoAnalyticsQuery, Error, cfo_analytics_schema};
use serde_json::Value;
pub fn accepts(name: &str) -> bool {
    name == "cfo_analytics"
}
pub fn tools() -> Vec<Value> {
    vec![tool(
        "cfo_analytics",
        "Read canonical CFO company/portfolio/group overview, exact dates and optional filters/page/cursor. companySlugs encodes repeated companySlug; company scope one company, group requires approved groupProfileId. Server owns signed amounts, source/party/dimension evidence and current visibility. Portfolio is juxtaposed, no implicit sum or consolidation; explicit aggregate sum only with complete authority. Return complete partial/blocked/unsupported state; no invented zero, FX, local math, confirmation or writes. Physical archive originals are verified by the native producer before source-linked archive collection.",
        cfo_analytics_schema(),
        true,
    )]
}
pub async fn call(
    api: &ApiClient,
    _name: &str,
    args: Value,
) -> Result<Result<Value, Error>, &'static str> {
    let q: CfoAnalyticsQuery = serde_json::from_value(args)
        .map_err(|_| "exact documented read-only CFO selectors required")?;
    Ok(api.cfo_analytics(&q).await)
}
