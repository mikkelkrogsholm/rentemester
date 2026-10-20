use crate::tools::{company_schema, tool};
use rentemester_client::{ApiClient, Error};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn accepts(s: &str) -> bool {
    matches!(s, "exceptions_list" | "exception_resolve")
}
pub fn tools() -> Vec<Value> {
    vec![
        tool(
            "exceptions_list",
            "CompanyRead: exact canonical task queue and context. Optional status open/resolved/all; includeArchived defaults false. Read-only: no task refresh, financial effects or local aggregation.",
            json!({"type":"object","required":["company"],"properties":{"company":company_schema(),"status":{"type":"string","enum":["open","resolved","all"]},"includeArchived":{"type":"boolean"}},"additionalProperties":false}),
            true,
        ),
        tool(
            "exception_resolve",
            "CompanyReview: explicitly review one task with optional note, current actor from authentication. Resolves the task only; does not book or clear an underlying bank condition. No caller keys/actor, inner confirm or automatic retries. If the outcome is unknown, read canonical queue/audit before further action.",
            json!({"type":"object","required":["company","id","confirm"],"properties":{"company":company_schema(),"id":{"type":"integer","minimum":1,"maximum":9_007_199_254_740_991_i64},"note":{"type":["string","null"],"maxLength":4096},"confirm":{"const":true}},"additionalProperties":false}),
            false,
        ),
    ]
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Read {
    company: String,
    status: Option<String>,
    #[serde(default, rename = "includeArchived")]
    include_archived: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Resolve {
    company: String,
    id: i64,
    #[serde(rename = "note")]
    _note: Option<String>,
    confirm: bool,
}
pub async fn call(
    api: &ApiClient,
    name: &str,
    args: Value,
) -> Result<Result<Value, Error>, &'static str> {
    match name {
        "exceptions_list" => {
            let r: Read = serde_json::from_value(args)
                .map_err(|_| "exceptions_list accepts only company/status/includeArchived")?;
            Ok(api
                .exceptions(&r.company, r.status.as_deref(), r.include_archived)
                .await)
        }
        "exception_resolve" => {
            let r: Resolve = serde_json::from_value(args.clone())
                .map_err(|_| "exception_resolve requires company/id/confirm and optional note")?;
            let mut body = json!({});
            // Preserve a caller-supplied null as well as exact text; never forward confirmation.
            if let Some(note) = args.get("note") {
                body["note"] = note.clone();
            }
            Ok(api
                .resolve_exception(
                    &r.company,
                    r.id,
                    serde_json::to_vec(&body).map_err(|_| "invalid exception note")?,
                    r.confirm,
                )
                .await)
        }
        _ => Err("unknown exceptions tool"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tools_expose_flat_resolution_and_read_only_queue() {
        let t = tools();
        assert_eq!(t[0]["name"], "exceptions_list");
        assert_eq!(t[0]["annotations"]["readOnlyHint"], true);
        assert_eq!(t[1]["name"], "exception_resolve");
        assert_eq!(t[1]["annotations"]["readOnlyHint"], false);
        assert_eq!(t[1]["inputSchema"]["additionalProperties"], false);
        assert!(t[1]["inputSchema"]["properties"].get("body").is_none());
        assert_eq!(t[1]["inputSchema"]["properties"]["confirm"]["const"], true);
    }
    #[test]
    fn malformed_flat_resolution_cannot_reach_network() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
            for args in [
                json!({"company":"synthetic","id":1,"confirm":true,"body":"{}"}),
                json!({"company":"synthetic","id":1,"confirm":true,"actor":"agent:caller"}),
                json!({"company":"synthetic","id":1,"confirm":true,"note":1}),
            ] {
                assert!(call(&api, "exception_resolve", args).await.is_err());
            }
            for args in [
                json!({"company":"synthetic","id":1,"confirm":false}),
                json!({"company":"synthetic","id":0,"confirm":true}),
            ] {
                assert!(matches!(
                    call(&api, "exception_resolve", args).await.unwrap(),
                    Err(Error::InvalidInput(_))
                ));
            }
        });
    }
}
