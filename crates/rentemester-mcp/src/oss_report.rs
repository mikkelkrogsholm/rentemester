//! Canonical read-only OSS base report.
use crate::tools::tool;
use rentemester_client::{ApiClient, Error};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn tools() -> Vec<Value> {
    vec![tool(
        "vat_oss_report",
        "Read explicitly classified OSS sales base and distinct entry count over inclusive from/to dates, with underlying Danish VAT report. Current company read authority. Base only: no destination/rate splitting, foreign VAT calculation or OSS submission. No local aggregation.",
        json!({"type":"object","required":["company","from","to"],"properties":{"company":{"type":"string"},"from":{"type":"string","minLength":10,"maxLength":10},"to":{"type":"string","minLength":10,"maxLength":10}},"additionalProperties":false}),
        true,
    )]
}
pub async fn call(api: &ApiClient, args: Value) -> Result<Result<Value, Error>, &'static str> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Input {
        company: String,
        from: String,
        to: String,
    }
    let i: Input = serde_json::from_value(args).map_err(|_| "exact company/from/to required")?;
    Ok(api.oss_report(&i.company, &i.from, &i.to).await)
}
