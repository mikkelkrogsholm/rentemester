use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{ApiClient, Error, MAX_BACKUP_LOCK_BODY, backup_lock_body_schema};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn tools() -> Vec<Value> {
    let mut body = native_body(backup_lock_body_schema());
    body["maxLength"] = json!(MAX_BACKUP_LOCK_BODY);
    vec![
        tool(
            "backup_status",
            "Current human CompanyAdmin: read verified local-retained snapshot freshness and opt-in lock policy for one company. No cloud/offsite/restore compliance claim, selectors or mutation.",
            json!({"type":"object","required":["company"],"properties":{"company":company_schema()},"additionalProperties":false}),
            true,
        ),
        tool(
            "backup_lock",
            "Current human CompanyAdmin: explicitly configure opt-in bookkeeping lock; exact4KiB native JSON string {confirm:true,enforced:boolean,graceDays:integer0..365}, outer confirm:true. No time/actor/path/key or generic replay. After unknown outcome read backup_status before any further action; never automatically retry. Local-retained backup scope does not prove cloud compliance.",
            json!({"type":"object","required":["company","body","confirm"],"properties":{"company":company_schema(),"body":body,"confirm":{"const":true}},"additionalProperties":false}),
            false,
        ),
    ]
}
pub async fn call(
    api: &ApiClient,
    name: &str,
    args: Value,
) -> Result<Result<Value, Error>, &'static str> {
    if name == "backup_status" {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Read {
            company: String,
        }
        let input: Read =
            serde_json::from_value(args).map_err(|_| "backup_status requires only company")?;
        return Ok(api.backup_status(&input.company).await);
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Write {
        company: String,
        body: String,
        confirm: bool,
    }
    let input: Write = serde_json::from_value(args)
        .map_err(|_| "backup_lock requires company, exact native body and confirm:true")?;
    Ok(api
        .configure_backup_lock(&input.company, input.body.into_bytes(), input.confirm)
        .await)
}
