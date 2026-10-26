//! Original task DTOs over the origin-bound protocol transport. No identity or local engine fallback.
use crate::{ApiClient, Error};
use reqwest::Method;
use serde_json::{Value, json};
pub const MAX_TASK_BODY: usize = 128 * 1024;
pub const TASK_OPERATIONS: [&str; 17] = [
    "list",
    "get",
    "history",
    "receipt",
    "sync",
    "create",
    "update",
    "move",
    "complete",
    "reopen",
    "series-list",
    "series-save",
    "series-project",
    "series-materialize",
    "boards-list",
    "boards-preview",
    "boards-save",
];
pub fn task_writes(op: &str) -> bool {
    matches!(
        op,
        "sync"
            | "create"
            | "update"
            | "move"
            | "complete"
            | "reopen"
            | "series-save"
            | "series-materialize"
            | "boards-save"
    )
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
            let path = if op == "series-save" || op == "boards-save" {
                if !input["expectedVersion"]
                    .as_u64()
                    .is_some_and(|n| n <= 9_007_199_254_740_991)
                {
                    return Err(Error::InvalidInput(
                        "nonnegative safe expectedVersion required",
                    ));
                }
                if op == "boards-save" {
                    "/api/task-boards"
                } else {
                    "/api/task-series"
                }
                .to_owned()
            } else if op == "series-materialize" {
                "/api/task-series/materialize".to_owned()
            } else if op == "sync" {
                "/api/tasks/sync".to_owned()
            } else if op == "create" {
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
            "boards-preview" => {
                self.request(
                    Method::POST,
                    "/api/task-boards/preview",
                    &[],
                    Some(
                        serde_json::to_vec(input)
                            .map_err(|_| Error::InvalidInput("board JSON required"))?,
                    ),
                )
                .await
            }
            "list" | "series-list" | "series-project" | "boards-list" => {
                let mut query = vec![];
                for (k, v) in object {
                    if k == "companySlugs" {
                        let companies = v
                            .as_array()
                            .ok_or(Error::InvalidInput("companySlugs array required"))?;
                        if companies.len() > if op == "list" { 100 } else { 200 } {
                            return Err(Error::InvalidInput("company selection exceeds limit"));
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
                    match op {
                        "boards-list" => "/api/task-boards",
                        "series-list" => "/api/task-series",
                        "series-project" => "/api/task-series/project",
                        _ => "/api/tasks",
                    },
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
    async fn series_routes_preserve_flat_bodies_keys_queries_and_empty_selection() {
        use std::{
            io::{Read, Write},
            thread,
        };
        for (op, input, expected, body) in [
            ("sync",json!({"companySlugs":["a"],"asOfDate":"2026-10-26","idempotencyKey":"source-key"}),"POST /api/tasks/sync",Some(json!({"companySlugs":["a"],"asOfDate":"2026-10-26","idempotencyKey":"source-key","confirm":true}))),
            (
                "boards-list",
                json!({"companySlugs":[]}),
                "GET /api/task-boards?companySlugs=",
                None,
            ),
            (
                "boards-preview",
                json!({"board":{"boardId":"company:a"},"expectedVersion":0}),
                "POST /api/task-boards/preview",
                Some(json!({"board":{"boardId":"company:a"},"expectedVersion":0})),
            ),
            (
                "boards-save",
                json!({"board":{"boardId":"company:a"},"expectedVersion":0,"idempotencyKey":"board-key"}),
                "POST /api/task-boards",
                Some(
                    json!({"board":{"boardId":"company:a"},"expectedVersion":0,"idempotencyKey":"board-key","confirm":true}),
                ),
            ),
            (
                "series-list",
                json!({"companySlugs":[],"includeArchived":false}),
                "GET /api/task-series?companySlugs=&includeArchived=false",
                None,
            ),
            (
                "series-project",
                json!({"companySlugs":["a","b"],"from":"2026-01-01","to":"2026-02-01"}),
                "GET /api/task-series/project?companySlug=a&companySlug=b&from=2026-01-01&to=2026-02-01",
                None,
            ),
            (
                "series-save",
                json!({"series":{"seriesId":"routine"},"expectedVersion":0,"idempotencyKey":"exact-key"}),
                "POST /api/task-series",
                Some(
                    json!({"series":{"seriesId":"routine"},"expectedVersion":0,"idempotencyKey":"exact-key","confirm":true}),
                ),
            ),
            (
                "series-materialize",
                json!({"companySlugs":[],"asOfDate":"2026-01-01","idempotencyKey":"exact-materialize"}),
                "POST /api/task-series/materialize",
                Some(
                    json!({"companySlugs":[],"asOfDate":"2026-01-01","idempotencyKey":"exact-materialize","confirm":true}),
                ),
            ),
        ] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let origin = format!("http://{}", listener.local_addr().unwrap());
            let server = thread::spawn(move || {
                // The shared legacy fixture manifest intentionally predates series.
                // Admit only this test's actual operation, without changing production discovery.
                let (mut discovery, _) = listener.accept().unwrap();
                discovery
                    .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .unwrap();
                let mut headers = vec![];
                while !headers.ends_with(b"\r\n\r\n") {
                    let mut byte = [0];
                    discovery.read_exact(&mut byte).unwrap();
                    headers.push(byte[0]);
                }
                assert!(
                    String::from_utf8(headers)
                        .unwrap()
                        .starts_with("GET /api/protocol HTTP/1.1")
                );
                let (method, path) = expected.split_once(' ').unwrap();
                let path = path.split('?').next().unwrap();
                let manifest = json!({"format":"rentemester-http-protocol/1","protocolMajor":1,"manifestId":"a".repeat(64),"wireProfiles":["rust-http-v1"],"operations":[{"method":method,"path":path,"capability":format!("{method} {path}")}]}).to_string();
                write!(discovery,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",manifest.len(),manifest).unwrap();
                drop(discovery);
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = vec![];
                loop {
                    let mut buf = [0; 4096];
                    let n = socket.read(&mut buf).unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buf[..n]);
                    if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                        let length = headers
                            .lines()
                            .find_map(|l| {
                                l.strip_prefix("content-length:")
                                    .map(|v| v.trim().parse::<usize>().unwrap())
                            })
                            .unwrap_or(0);
                        if bytes.len() >= end + 4 + length {
                            break;
                        }
                    }
                }
                let reply = r#"{"ok":true,"data":[]}"#;
                write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", reply.len(), reply).unwrap();
                String::from_utf8(bytes).unwrap()
            });
            let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
            api.task_operation(op, &input, task_writes(op))
                .await
                .unwrap();
            let request = server.join().unwrap();
            assert!(
                request.starts_with(&format!("{expected} HTTP/1.1")),
                "{request}"
            );
            if let Some(body) = body {
                assert_eq!(
                    serde_json::from_str::<Value>(request.split_once("\r\n\r\n").unwrap().1)
                        .unwrap(),
                    body
                );
            }
        }
    }
    #[test]
    fn series_schema_is_bounded_and_does_not_expose_runtime_or_authority() {
        assert_eq!(TASK_OPERATIONS.len(), 17);
        let save = task_input_schema("series-save");
        assert_eq!(
            save["properties"]["series"]["properties"]["workDayOffset"]["maximum"],
            366
        );
        assert_eq!(save["properties"]["expectedVersion"]["minimum"], 0);
        assert_eq!(save["additionalProperties"], false);
        assert!(save["properties"].get("actor").is_none());
        let sync=task_input_schema("sync");
        assert_eq!(sync["additionalProperties"],false);
        assert_eq!(sync["properties"]["companySlugs"]["maxItems"],200);
        assert!(sync["properties"].get("expectedVersion").is_none());
        assert_eq!(sync["required"],json!(["idempotencyKey"]));
        assert!(task_input_schema("run").is_null());
    }
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
