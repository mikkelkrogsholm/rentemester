use crate::tools::tool;
use rentemester_client::{ApiClient, Error};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn tools() -> Vec<Value> {
    let mut value = tool(
        "audit_verify",
        "Read-only CompanyRead through the common accounting verifier. Inspect ok/errors as the business verdict and data.entries as the checked entry count. Human MFA cookie or scoped service with current access. An absolute company path requires the verified local profile; remote callers use a slug. No repair or caller authority.",
        json!({"type":"object","required":["company"],"properties":{"company":{"type":"string","minLength":1,"description":"Company slug, or absolute company path inside the verified local workspace"}},"additionalProperties":false}),
        true,
    );
    value["annotations"]["openWorldHint"] = json!(false);
    vec![value]
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    company: String,
}
pub async fn call(api: &ApiClient, args: Value) -> Result<Result<Value, Error>, &'static str> {
    let input: Input =
        serde_json::from_value(args).map_err(|_| "audit_verify requires only company")?;
    Ok(api.audit_verify_original717(&input.company).await)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn audit_verify_is_read_only_closed_and_rejects_authority_fields() {
        let value = tools().remove(0);
        assert_eq!(value["annotations"]["readOnlyHint"], true);
        assert_eq!(value["annotations"]["openWorldHint"], false);
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let api =
            ApiClient::with_service_key("http://127.0.0.1:9", true, "synthetic-service".into())
                .unwrap();
        assert!(
            runtime
                .block_on(call(&api, json!({"company":"synthetic","actor":"forged"})))
                .is_err()
        );
    }
}
