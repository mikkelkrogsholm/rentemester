//! Existing task tool names and flat DTOs, same protocol-bound HTTP client.
use crate::tools::tool;
use rentemester_client::{ApiClient, Error, TASK_OPERATIONS, task_input_schema, task_writes};
use serde_json::{Value, json};
pub fn operation(name: &str) -> Option<&str> {
    let op = name.strip_prefix("tasks_")?;
    let op = if op == "operation_receipt" {
        "receipt"
    } else {
        op
    };
    TASK_OPERATIONS.contains(&op).then_some(op)
}
pub fn tools() -> Vec<Value> {
    TASK_OPERATIONS.iter().map(|op|{let mut schema=task_input_schema(op);if task_writes(op){schema["properties"]["confirm"]=json!({"const":true});schema["required"].as_array_mut().unwrap().push(json!("confirm"));}tool(&format!("tasks_{}",if *op=="receipt"{"operation_receipt"}else{op}),"Manual tasks through the shared Rust server. Current identity/all-company rights before receipt; writes require confirm, original idempotencyKey and expectedVersion for existing tasks. No actor/local grants, financial effects, generated keys or automatic retry. After unknown result read exact receipt/history before another action. Automatic sources/series/runtime are not enabled by this tool.",schema,!task_writes(op))}).collect()
}
pub async fn call(
    api: &ApiClient,
    name: &str,
    mut args: Value,
) -> Result<Result<Value, Error>, &'static str> {
    let op = operation(name).ok_or("unknown task tool")?;
    let object = args
        .as_object_mut()
        .ok_or("task arguments object required")?;
    let confirm = if task_writes(op) {
        object.remove("confirm") == Some(json!(true))
    } else {
        false
    };
    Ok(api.task_operation(op, &args, confirm).await)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tools_keep_original_shapes_and_explicit_confirmation() {
        let all = tools();
        assert_eq!(all.len(), 9);
        assert!(
            all.iter().find(|t| t["name"] == "tasks_create").unwrap()["inputSchema"]["required"]
                .as_array()
                .unwrap()
                .contains(&json!("confirm"))
        );
        assert!(operation("tasks_run").is_none());
    }
}
