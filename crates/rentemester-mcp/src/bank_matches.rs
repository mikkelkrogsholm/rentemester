//! Canonical advisory reads; scores and figures are never computed here.
use crate::tools::{company_schema, tool};
use rentemester_client::{ApiClient, Error};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn accepts(name: &str) -> bool {
    name == "bank_suggest_matches"
}
pub fn tools() -> Vec<Value> {
    vec![tool(
        "bank_suggest_matches",
        "CompanyRead: read canonical server suggestions for unmatched bank rows across invoices, purchases, customer credit refunds and tentative supplier refunds. Optional positive bankTransactionId selects one unmatched row; max defaults5 and limits suggestions per row, not row count. Scores may exceed1 and are not probabilities. Preserves source ok:false/errors for unknown or already matched rows. Read-only: no posting, task refresh, automatic match or local aggregation; review originals and use separate explicit financial actions.",
        json!({"type":"object","required":["company"],"properties":{"company":company_schema(),"bankTransactionId":{"type":"integer","minimum":1,"maximum":9_007_199_254_740_991_i64},"max":{"type":"integer","minimum":1}},"additionalProperties":false}),
        true,
    )]
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Read {
    company: String,
    bank_transaction_id: Option<i64>,
    max: Option<usize>,
}
pub async fn call(
    api: &ApiClient,
    name: &str,
    args: Value,
) -> Result<Result<Value, Error>, &'static str> {
    if !accepts(name) {
        return Err("unknown bank match suggestions tool");
    }
    if args
        .get("bankTransactionId")
        .is_some_and(|v| v.as_i64().is_none())
        || args.get("max").is_some_and(|v| v.as_u64().is_none())
    {
        return Err("bankTransactionId and max must be positive integers when supplied");
    }
    let input: Read = serde_json::from_value(args)
        .map_err(|_| "bank_suggest_matches accepts only company, bankTransactionId and max")?;
    Ok(api
        .bank_suggest_matches(&input.company, input.bank_transaction_id, input.max)
        .await)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discovery_is_flat_read_only_with_unbounded_positive_max() {
        let t = tools();
        assert_eq!(t.len(), 1);
        assert_eq!(t[0]["annotations"]["readOnlyHint"], true);
        assert_eq!(t[0]["inputSchema"]["additionalProperties"], false);
        let properties = &t[0]["inputSchema"]["properties"];
        assert!(properties["max"].get("maximum").is_none());
        assert!(properties.get("confirm").is_none());
        assert!(properties.get("body").is_none());
    }
    #[test]
    fn malformed_or_mutating_arguments_never_reach_network() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
            for args in [
                json!({"company":"synthetic","confirm":true}),
                json!({"company":"synthetic","actor":"forged"}),
                json!({"company":"synthetic","max":null}),
                json!({"company":"synthetic","max":"5"}),
                json!({"company":"synthetic","bankTransactionId":null}),
                json!({"company":"synthetic","bankTransactionId":1.5}),
            ] {
                assert!(call(&api, "bank_suggest_matches", args).await.is_err());
            }
            for args in [
                json!({"company":"synthetic","max":0}),
                json!({"company":"synthetic","bankTransactionId":0}),
                json!({"company":"synthetic","bankTransactionId":9007199254740992_i64}),
            ] {
                assert!(matches!(
                    call(&api, "bank_suggest_matches", args).await.unwrap(),
                    Err(Error::InvalidInput(_))
                ));
            }
        });
    }
}
