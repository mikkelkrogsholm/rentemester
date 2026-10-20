//! Canonical EU report and explicit external VIES observation.
use crate::tools::tool;
use rentemester_client::{ApiClient, Error};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn accepts(name: &str) -> bool {
    matches!(name, "eu_sales" | "vies_validate")
}
pub fn tools() -> Vec<Value> {
    vec![
        tool(
            "eu_sales",
            "Read dated posted foreign EU reverse-charge sales and signed credits in DKK using verified frozen invoice valuations and supported explicit legal bases. Current company read authority; explicit inclusive from/to dates. Preparation only, no tax submission or local aggregation.",
            json!({"type":"object","required":["company","from","to"],"properties":{"company":{"type":"string"},"from":{"type":"string"},"to":{"type":"string"}},"additionalProperties":false}),
            true,
        ),
        tool(
            "vies_validate",
            "Explicitly query operator-configured VIES once and retain trusted observation. Requires current company write authority and confirm:true. Never accepts caller validity, provider URL or timestamps. Inconclusive provider failure does not mean invalid. No automatic retry or invoice issuance.",
            json!({"type":"object","required":["company","vatOrCvr","confirm"],"properties":{"company":{"type":"string"},"vatOrCvr":{"type":"string","minLength":1,"maxLength":64},"confirm":{"const":true}},"additionalProperties":false}),
            false,
        ),
    ]
}
pub async fn call(
    api: &ApiClient,
    name: &str,
    args: Value,
) -> Result<Result<Value, Error>, &'static str> {
    if name == "eu_sales" {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Input {
            company: String,
            from: String,
            to: String,
        }
        let i: Input =
            serde_json::from_value(args).map_err(|_| "exact company/from/to required")?;
        Ok(api.eu_sales(&i.company, &i.from, &i.to).await)
    } else if name == "vies_validate" {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Input {
            company: String,
            vat_or_cvr: String,
            confirm: bool,
        }
        let i: Input =
            serde_json::from_value(args).map_err(|_| "exact company/vatOrCvr/confirm required")?;
        if !i.confirm {
            return Err("explicit confirm:true required");
        }
        Ok(api.vies_validate(&i.company, &i.vat_or_cvr).await)
    } else {
        Err("unknown EU operation")
    }
}
