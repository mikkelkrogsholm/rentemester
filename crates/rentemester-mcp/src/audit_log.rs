use crate::tools::tool;
use rentemester_client::{ApiClient, AuditLogQuery, Error, audit_log_query_schema};
use serde_json::{Value, json};
pub fn tools() -> Vec<Value> {
    let mut schema = audit_log_query_schema();
    schema["properties"]["company"] = json!({"type":"string","minLength":1,"description":"Company slug, or absolute company path inside the verified local workspace"});
    schema["required"] = json!(["company"]);
    let mut t = tool(
        "audit_log_list",
        "Current CompanyRead via MFA cookie or scoped service. Read company audit history with optional limit1..5000(default500), safe offset(default0), fromDate/toDate YYYY-MM-DD or explicit-offset RFC3339, eventTypeLike/actorLike. Dates normalize to UTC; bare upper dates include the full day. LIKE substring filters are case-insensitive and ANDed; % and _ remain wildcards. Stable createdAt DESC,id DESC and exact server total/count/hasMore/nextOffset; each page is a new observation if writes intervene. Audit messages are plain retained text. No body, mutation, actor override or health/full-verification claim.",
        schema,
        true,
    );
    t["annotations"]["openWorldHint"] = json!(false);
    vec![t]
}
pub async fn call(api: &ApiClient, mut args: Value) -> Result<Result<Value, Error>, &'static str> {
    let company = args
        .as_object_mut()
        .and_then(|o| o.remove("company"))
        .and_then(|v| v.as_str().map(str::to_owned))
        .ok_or("audit_log_list requires company and only typed audit filters")?;
    let query:AuditLogQuery=serde_json::from_value(args).map_err(|_|"audit_log_list requires only company/limit/offset/fromDate/toDate/eventTypeLike/actorLike; no null or authority fields")?;
    Ok(api.audit_log_original717(&company, &query).await)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalogue_is_readonly_idempotent_closedworld_and_typed() {
        let t = tools().remove(0);
        assert_eq!(t["name"], "audit_log_list");
        for key in ["readOnlyHint", "idempotentHint"] {
            assert_eq!(t["annotations"][key], true)
        }
        assert_eq!(t["annotations"]["openWorldHint"], false);
        assert_eq!(t["inputSchema"]["additionalProperties"], false);
        assert_eq!(t["inputSchema"]["properties"]["limit"]["maximum"], 5000);
        assert!(
            crate::tools::list()["tools"]
                .as_array()
                .unwrap()
                .iter()
                .any(|t| t["name"] == "audit_log_list")
        );
    }
    #[test]
    fn unknown_null_and_noninteger_filters_reject_before_network() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let api =
            ApiClient::with_service_key("http://127.0.0.1:9", true, "synthetic-service".into())
                .unwrap();
        for args in [
            json!({"company":"synthetic","actor":"forged"}),
            json!({"company":"synthetic","limit":null}),
            json!({"company":"synthetic","offset":-1}),
            json!({"company":"synthetic","confirm":true}),
        ] {
            assert!(runtime.block_on(call(&api, args)).is_err())
        }
    }
}
