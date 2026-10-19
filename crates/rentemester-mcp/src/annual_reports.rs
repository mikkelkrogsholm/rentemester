//! Read-only annual preparation through the shared client.
use crate::tools::{company_schema, tool};
use rentemester_client::{ApiClient, Error, valid_annual_bounds};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn tools() -> Vec<Value> {
    let date = json!({"type":"string","pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$","description":"Explicit real Gregorian YYYY-MM-DD date"});
    vec![tool(
        "annual_report",
        "Read class-B annual preparation for explicit fiscal dates. Current company.read permission required. Preserves report.ok:false/errors for missing CVR/open year or other blockers. No closing, filing, statutory export, PDF, confirmation, body or files.",
        json!({"type":"object","required":["company","fiscalYearStart","fiscalYearEnd"],"properties":{"company":company_schema(),"fiscalYearStart":date,"fiscalYearEnd":date},"additionalProperties":false}),
        true,
    )]
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Read {
    company: String,
    fiscal_year_start: String,
    fiscal_year_end: String,
}
pub async fn call(api: &ApiClient, args: Value) -> Result<Result<Value, Error>, &'static str> {
    let input: Read = serde_json::from_value(args)
        .map_err(|_| "annual_report requires only company, fiscalYearStart and fiscalYearEnd")?;
    if !valid_annual_bounds(&input.fiscal_year_start, &input.fiscal_year_end) {
        return Err("annual_report requires ascending real ISO fiscal dates");
    }
    Ok(api
        .annual_report(
            &input.company,
            &input.fiscal_year_start,
            &input.fiscal_year_end,
        )
        .await)
}
