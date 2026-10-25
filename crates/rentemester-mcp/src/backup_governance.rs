use crate::tools::{company_schema, tool};
use rentemester_client::{ApiClient, Error};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn tools() -> Vec<Value> {
    super::backup_registration::original717::tool("system_backup_governance")
        .into_iter()
        .collect()
}
pub async fn call(api: &ApiClient, args: Value) -> Result<Result<Value, Error>, &'static str> {
    super::backup_registration::original717::call(api, "system_backup_governance", args).await
}

pub async fn source_call(api: &ApiClient, args: Value) -> Result<Value, &'static str> {
    super::backup_registration::original717::source_call(api, "system_backup_governance", args)
        .await
}
