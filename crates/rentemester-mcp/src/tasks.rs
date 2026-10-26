//! Existing task tool names and flat DTOs, same protocol-bound HTTP client.
use crate::tools::tool;
use rentemester_client::{ApiClient, Error, TASK_OPERATIONS, task_input_schema, task_writes};
use serde_json::{Value, json};
pub fn operation(name: &str) -> Option<&str> {
    if let Some(suffix) = name.strip_prefix("task_boards_") {
        return match suffix {
            "list" => Some("boards-list"),
            "preview" => Some("boards-preview"),
            "save" => Some("boards-save"),
            _ => None,
        };
    }
    if let Some(suffix) = name.strip_prefix("task_series_") {
        return match suffix {
            "list" => Some("series-list"),
            "save" => Some("series-save"),
            "project" => Some("series-project"),
            "materialize" => Some("series-materialize"),
            _ => None,
        };
    }
    let op = name.strip_prefix("tasks_")?;
    let op = if op == "operation_receipt" {
        "receipt"
    } else {
        op
    };
    (TASK_OPERATIONS.contains(&op) && !op.starts_with("series-") && !op.starts_with("boards-"))
        .then_some(op)
}
pub fn tools() -> Vec<Value> {
    TASK_OPERATIONS.iter().map(|op|{let mut schema=task_input_schema(op);if task_writes(op){schema["properties"]["confirm"]=json!({"const":true});schema["required"].as_array_mut().unwrap().push(json!("confirm"));}tool(&if let Some(suffix) = op.strip_prefix("series-") { format!("task_series_{suffix}") } else if let Some(suffix)=op.strip_prefix("boards-"){format!("task_boards_{suffix}")} else { format!("tasks_{}", if *op=="receipt"{"operation_receipt"}else{op}) },"Tasks, custom boards and explicit routine series operations through the shared Rust server. Current identity/all-company rights before receipt; writes require confirm, original idempotencyKey and expectedVersion for existing tasks. No actor/local grants, financial effects, generated keys or automatic retry. After unknown result read exact receipt/history before another action. Board preview and series projection are read-only; board save and materialization are explicit. Source sync uses current native ledger facts; automatic runtime is not enabled by this tool.",schema,!task_writes(op))}).collect()
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
        assert_eq!(all.len(), 17);
        for suffix in ["list", "preview", "save"] {
            let name = format!("task_boards_{suffix}");
            assert!(operation(&name).is_some());
            let schema = &all.iter().find(|t| t["name"] == name).unwrap()["inputSchema"];
            assert_eq!(
                schema["properties"].get("confirm").is_some(),
                suffix == "save"
            );
        }
        assert!(
            all.iter().find(|t| t["name"] == "tasks_create").unwrap()["inputSchema"]["required"]
                .as_array()
                .unwrap()
                .contains(&json!("confirm"))
        );
        for suffix in ["list", "save", "project", "materialize"] {
            let name = format!("task_series_{suffix}");
            assert!(operation(&name).is_some());
            let schema = &all.iter().find(|t| t["name"] == name).unwrap()["inputSchema"];
            assert_eq!(
                schema["properties"].get("confirm").is_some(),
                matches!(suffix, "save" | "materialize")
            );
        }
        assert!(operation("task_series_run").is_none());
        assert!(operation("tasks_series-save").is_none());
        assert!(operation("tasks_run").is_none());
    }
}
