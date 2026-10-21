use crate::tools::{company_schema, tool};
use rentemester_client::{ApiClient, Error};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn tools() -> Vec<Value> {
    vec![tool(
        "system_backup_governance",
        "Current human CompanyAdmin: read this company's latest qualified retained backup, matching recorded placements and historical authenticated external-copy observations. Only company is accepted. No provider requests or mutations. Operator offsite declarations are not independently verified, historical observations do not establish current availability, and recovery is not tested.",
        json!({"type":"object","required":["company"],"properties":{"company":company_schema()},"additionalProperties":false}),
        true,
    )]
}
pub async fn call(api: &ApiClient, args: Value) -> Result<Result<Value, Error>, &'static str> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Input {
        company: String,
    }
    let input: Input = serde_json::from_value(args)
        .map_err(|_| "system_backup_governance accepts only company")?;
    Ok(api.backup_governance(&input.company).await)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_scope_service_refusal_and_readonly_registry() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let api =
                ApiClient::with_service_key("http://127.0.0.1:9", true, "synthetic-service".into())
                    .unwrap();
            assert!(matches!(
                call(&api, json!({"company":"synthetic"})).await.unwrap(),
                Err(Error::InvalidInput(_))
            ));
            for field in [
                "asOf",
                "actor",
                "operationId",
                "body",
                "confirm",
                "serviceKey",
            ] {
                let mut input = json!({"company":"synthetic"});
                input[field] = json!("forged");
                assert!(call(&api, input).await.is_err())
            }
            let tool = tools().remove(0);
            assert_eq!(tool["name"], "system_backup_governance");
            assert_eq!(tool["annotations"]["readOnlyHint"], true);
            assert_eq!(tool["inputSchema"]["additionalProperties"], false);
        });
    }
}
