//! Original task DTOs over the origin-bound protocol transport. No identity or local engine fallback.
use crate::{ApiClient, Error};
use reqwest::Method;
use serde_json::{Value, json};
pub const MAX_TASK_BODY: usize = 128 * 1024;
pub const TASK_OPERATIONS: [&str; 9] = [
    "list", "get", "history", "receipt", "create", "update", "move", "complete", "reopen",
];
pub fn task_writes(op: &str) -> bool {
    matches!(op, "create" | "update" | "move" | "complete" | "reopen")
}
/// Generated from src/task-interface-input.ts with z.toJSONSchema; receipt is metadata-only.
pub fn task_input_schema(op: &str) -> Value {
    let all: Value =
        serde_json::from_str(include_str!("tasks-input.json")).expect("checked task schema");
    all[op].clone()
}
fn id(v: &Value) -> Result<String, Error> {
    let s = v
        .as_str()
        .filter(|s| {
            !s.trim().is_empty()
                && s.encode_utf16().count() <= 160
                && !s.chars().any(char::is_control)
                && !matches!(*s, "." | "..")
        })
        .ok_or(Error::InvalidInput("bounded task ID/key required"))?;
    Ok(s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect())
}
impl ApiClient {
    /// One attempt only. Caller retains exact input/key after OutcomeUnknown and reads receipt/history.
    pub async fn task_operation(
        &self,
        op: &str,
        input: &Value,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if !TASK_OPERATIONS.contains(&op) || confirmed != task_writes(op) {
            return Err(Error::InvalidInput(
                "supported task operation and explicit write-only confirmation required",
            ));
        }
        let schema = task_input_schema(op);
        let object = input
            .as_object()
            .ok_or(Error::InvalidInput("task arguments object required"))?;
        if object.keys().any(|k| schema["properties"].get(k).is_none())
            || schema["required"]
                .as_array()
                .is_some_and(|r| r.iter().any(|k| !object.contains_key(k.as_str().unwrap())))
        {
            return Err(Error::InvalidInput(
                "task arguments differ from discoverable schema",
            ));
        }
        if serde_json::to_vec(input)
            .map_err(|_| Error::InvalidInput("task JSON required"))?
            .len()
            > MAX_TASK_BODY - 16
        {
            return Err(Error::InvalidInput("task body exceeds128KiB"));
        }
        if task_writes(op) {
            id(&input["idempotencyKey"])?;
            let mut body = input.clone();
            let path = if op == "create" {
                "/api/tasks".to_owned()
            } else {
                if !input["expectedVersion"]
                    .as_u64()
                    .is_some_and(|n| (1..=9_007_199_254_740_991).contains(&n))
                {
                    return Err(Error::InvalidInput(
                        "positive safe expectedVersion required",
                    ));
                }
                let path = format!("/api/tasks/{}/{}", id(&input["taskId"])?, op);
                body.as_object_mut().unwrap().remove("taskId");
                path
            };
            body["confirm"] = json!(true);
            let bytes =
                serde_json::to_vec(&body).map_err(|_| Error::InvalidInput("task JSON required"))?;
            return self.request(Method::POST, &path, &[], Some(bytes)).await;
        }
        match op {
            "list" => {
                let mut query = vec![];
                for (k, v) in object {
                    if k == "companySlugs" {
                        let companies = v
                            .as_array()
                            .ok_or(Error::InvalidInput("companySlugs array required"))?;
                        if companies.len() > 100 {
                            return Err(Error::InvalidInput("company selection max100"));
                        }
                        // Keep explicit [] distinct from an omitted all-visible selection.
                        if companies.is_empty() {
                            query.push(("companySlugs".to_owned(), String::new()));
                        }
                        for c in companies {
                            query.push((
                                "companySlug".to_owned(),
                                c.as_str()
                                    .ok_or(Error::InvalidInput("company slug string required"))?
                                    .to_owned(),
                            ));
                        }
                    } else {
                        let value = if let Some(s) = v.as_str() {
                            s.to_owned()
                        } else if let Some(b) = v.as_bool() {
                            b.to_string()
                        } else {
                            return Err(Error::InvalidInput("task filter string/boolean required"));
                        };
                        query.push((k.clone(), value));
                    }
                }
                self.get(
                    "/api/tasks",
                    &query
                        .iter()
                        .map(|(k, v)| (k.as_str(), v.as_str()))
                        .collect::<Vec<_>>(),
                )
                .await
            }
            "get" => {
                self.get(&format!("/api/tasks/{}", id(&input["taskId"])?), &[])
                    .await
            }
            "history" => {
                self.get(
                    &format!("/api/tasks/{}/history", id(&input["taskId"])?),
                    &[],
                )
                .await
            }
            "receipt" => {
                self.get(
                    &format!("/api/tasks/operations/{}", id(&input["idempotencyKey"])?),
                    &[],
                )
                .await
            }
            _ => unreachable!(),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn refuses_authority_and_unconfirmed_mutations_before_network() {
        let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
        for (op, input, confirm) in [
            (
                "create",
                json!({"title":"Task","scope":{"kind":"company","companySlug":"a"},"idempotencyKey":"k"}),
                false,
            ),
            (
                "create",
                json!({"title":"Task","scope":{},"idempotencyKey":"k","actor":"user:owner"}),
                true,
            ),
            ("list", json!({"local":true}), false),
            (
                "update",
                json!({"taskId":"id","patch":{},"idempotencyKey":"k","expectedVersion":0}),
                true,
            ),
        ] {
            assert!(matches!(
                api.task_operation(op, &input, confirm).await,
                Err(Error::InvalidInput(_))
            ));
        }
    }
}
