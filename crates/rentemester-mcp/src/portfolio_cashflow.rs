//! Read-only authoritative reports; no source/default/amount inference.
use crate::tools::tool;
use rentemester_client::{ApiClient, Error};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn accepts(name: &str) -> bool {
    matches!(name, "portfolio" | "cashflow")
}
pub fn tools() -> Vec<Value> {
    vec![
        tool(
            "portfolio",
            "Current visible-company portfolio asOf; server owns cutoff, visibility and totals. Not consolidated and not an alias for TS portfolio_overview. Preserve partial/unavailable/null source facts; no company selector or local sums.",
            json!({"type":"object","properties":{"asOf":{"type":"string"}},"additionalProperties":false}),
            true,
        ),
        tool(
            "cashflow",
            "Read actual imported bank cashflow by authoritative fiscal year; distinct from future forecast. Preserve shifted fiscal months, partial valuations, unknown statement balances and limitations; no local computations.",
            json!({"type":"object","required":["company"],"properties":{"company":{"type":"string","minLength":1},"year":{"type":"string","minLength":1,"maxLength":32}},"additionalProperties":false}),
            true,
        ),
    ]
}
pub async fn call(
    api: &ApiClient,
    name: &str,
    args: Value,
) -> Result<Result<Value, Error>, &'static str> {
    if name == "portfolio" {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Input {
            as_of: Option<String>,
        }
        let input: Input = serde_json::from_value(args)
            .map_err(|_| "exact readonly portfolio selectors required")?;
        Ok(api.portfolio(input.as_of.as_deref()).await)
    } else {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Input {
            company: String,
            year: Option<String>,
        }
        let input: Input = serde_json::from_value(args)
            .map_err(|_| "exact readonly cashflow selectors required")?;
        Ok(api.cashflow(&input.company, input.year.as_deref()).await)
    }
}
