//! The original portfolio_overview keeps its errors[] envelope independently of native tools.
use crate::tools::tool;
use rentemester_client::{ApiClient, Error};
use serde_json::{Value, json};
pub fn tools() -> Vec<Value> {
    let mut tool = tool(
        "portfolio_overview",
        "Read status side by side for currently visible active legal entities: VAT, dated native DKK receivables, current authorized local backup, audit and exceptions. No cross-company totals or consolidation. Omitted asOf uses server UTC today; current metadata and unavailable values retain their source basis. Omitted/blank workspace uses this configured server. Nonempty filesystem workspace selectors are refused pending the shared local-workspace launcher; local/default/environment compatibility is not qualified. No paths are opened, provider effects or client financial calculations.",
        json!({"type":"object","properties":{"workspace":{"type":"string","description":"Omit or leave blank for the connected server; nonempty local workspace paths are explicitly unsupported."},"asOf":{"type":"string","pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$","description":"Optional real Gregorian date; default is trusted server UTC today."}},"additionalProperties":false}),
        true,
    );
    tool["annotations"]["openWorldHint"] = json!(false);
    tool["outputSchema"] = json!({"type":"object","required":["ok","errors"],"properties":{"ok":{"type":"boolean"},"data":{"type":"object","additionalProperties":true},"errors":{"type":"array","items":{"type":"string"}},"appliedRules":{"type":"array","items":{"type":"string"}},"code":{"type":"string"}},"additionalProperties":false});
    vec![tool]
}
fn result(envelope: Value) -> Value {
    json!({"isError":envelope["ok"]==false,"content":[{"type":"text","text":envelope.to_string()}],"structuredContent":envelope})
}
fn failure(message: &str, code: &str) -> Value {
    result(json!({"ok":false,"errors":[message],"code":code}))
}
pub async fn call(api: &ApiClient, args: Value) -> Result<Value, &'static str> {
    let object = args
        .as_object()
        .ok_or("portfolio_overview requires an arguments object")?;
    if object
        .keys()
        .any(|k| !matches!(k.as_str(), "workspace" | "asOf"))
    {
        return Err("portfolio_overview allows only workspace and asOf");
    }
    let string = |key| {
        object
            .get(key)
            .map(|value| {
                value
                    .as_str()
                    .ok_or("portfolio_overview optional selectors must be strings, never null")
            })
            .transpose()
    };
    let workspace = string("workspace")?;
    let as_of = string("asOf")?;
    if as_of.is_some_and(|date| {
        date.len() != 10
            || !date.bytes().enumerate().all(|(i, b)| {
                if i == 4 || i == 7 {
                    b == b'-'
                } else {
                    b.is_ascii_digit()
                }
            })
    }) {
        return Err("asOf must be YYYY-MM-DD");
    }
    if workspace.is_some_and(|value| !value.trim().is_empty()) {
        return Ok(failure(
            "Nonempty filesystem workspace selectors are unavailable in connected-server mode. Omit workspace to use the configured server; shared local-workspace launcher compatibility remains pending.",
            "local_workspace_unavailable",
        ));
    }
    Ok(match api.portfolio_overview(as_of).await {
        Ok(data) => result(json!({"ok":true,"data":data,"errors":[]})),
        Err(error) => {
            let code = match error {
                Error::InvalidInput(_) => "invalid_input",
                Error::Refused { .. } => "server_refused",
                Error::Transport => "transport",
                Error::Protocol => "protocol",
                Error::OutcomeUnknown => "outcome_unknown",
            };
            failure(&error.to_string(), code)
        }
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_name_schema_envelope_and_closedworld_annotations() {
        let t = tools().remove(0);
        assert_eq!(t["name"], "portfolio_overview");
        assert_eq!(t["inputSchema"]["additionalProperties"], false);
        assert_eq!(t["inputSchema"]["properties"].as_object().unwrap().len(), 2);
        assert_eq!(t["outputSchema"]["required"], json!(["ok", "errors"]));
        for key in ["readOnlyHint", "idempotentHint"] {
            assert_eq!(t["annotations"][key], true);
        }
        assert_eq!(t["annotations"]["openWorldHint"], false);
        assert_eq!(
            crate::tools::list()["tools"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|t| t["name"] == "portfolio_overview")
                .count(),
            1
        );
    }
    #[test]
    fn local_path_and_calendar_failures_are_legacy_business_errors_without_network() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
        for args in [
            json!({"workspace":"/private/synthetic-workspace"}),
            json!({"asOf":"2026-02-30"}),
        ] {
            let value = runtime
                .block_on(crate::tools::call(&api, "portfolio_overview", args))
                .unwrap();
            assert_eq!(value["isError"], true);
            let envelope = &value["structuredContent"];
            assert_eq!(envelope["ok"], false);
            assert!(!envelope["errors"].as_array().unwrap().is_empty());
            assert!(envelope.get("data").is_none());
            assert!(envelope.get("error").is_none());
            assert!(!value.to_string().contains("/private/synthetic-workspace"));
        }
        let native = runtime
            .block_on(crate::tools::call(
                &api,
                "portfolio",
                json!({"asOf":"2026-02-30"}),
            ))
            .unwrap();
        assert!(native["structuredContent"]["error"].is_object());
        assert!(native["structuredContent"].get("errors").is_none());
    }
    #[test]
    fn stdio_protocol_distinguishes_bad_arguments_from_business_refusals() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
        runtime.block_on(async {let mut session=crate::Session::default();let init=session.handle(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"synthetic-portfolio134","version":"1"}}}),&api).await.unwrap();assert!(init.get("result").is_some());session.handle(json!({"jsonrpc":"2.0","method":"notifications/initialized"}),&api).await;for(id,args)in [(2,json!({"workspace":null})),(3,json!({"company":"hidden"})),(4,json!({"asOf":7})),(5,json!({"confirm":true})),(6,json!({"asOf":null})),(7,json!({"asOf":""})),(8,json!({"asOf":"2026-2-03"}))]{let response=session.handle(json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":"portfolio_overview","arguments":args}}),&api).await.unwrap();assert_eq!(response["error"]["code"],-32602);}let response=session.handle(json!({"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"portfolio_overview","arguments":{"workspace":"/private/synthetic"}}}),&api).await.unwrap();assert!(response.get("error").is_none());assert_eq!(response["result"]["isError"],true);assert_eq!(response["result"]["structuredContent"]["code"],"local_workspace_unavailable");});
    }
    #[test]
    fn configured_server_blank_workspace_uses_one_http_read_and_original_success_envelope() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
            thread,
        };
        for workspace in [None, Some(" \t ")] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let origin = format!("http://{}", listener.local_addr().unwrap());
            let overview = json!({"asOf":"2026-10-24","companyCount":1,"companies":[{"slug":"synthetic-a","name":"Synthetic A","cvr":null,"ok":true,"vat":{"canCompute":false,"netVatPayable":null,"errors":["unavailable"]},"openReceivables":{"count":1,"totalOpenBalance":74.95,"currency":"DKK"},"backup":{"backupDue":null,"available":false},"audit":{"ok":true,"entries":3,"firstError":null},"openExceptions":{"count":0,"highSeverityCount":0}}]});
            let expected = overview.clone();
            let task = thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = vec![];
                while !bytes.ends_with(b"\r\n\r\n") {
                    let mut b = [0; 4096];
                    let n = socket.read(&mut b).unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&b[..n]);
                }
                let body = json!({"ok":true,"overview":overview}).to_string();
                write!(
                    socket,
                    "HTTP/1.1 200 OK\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
                String::from_utf8(bytes).unwrap()
            });
            let mut args = json!({});
            if let Some(workspace) = workspace {
                args["workspace"] = json!(workspace);
            }
            let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
            let runtime = tokio::runtime::Runtime::new().unwrap();
            let response = runtime
                .block_on(crate::tools::call(&api, "portfolio_overview", args))
                .unwrap();
            assert_eq!(response["isError"], false);
            assert_eq!(
                response["structuredContent"],
                json!({"ok":true,"data":expected,"errors":[]})
            );
            let text = &response["content"][0]["text"];
            assert_eq!(
                serde_json::from_str::<Value>(text.as_str().unwrap()).unwrap(),
                response["structuredContent"]
            );
            let request = task.join().unwrap();
            assert!(request.starts_with("GET /api/portfolio/overview HTTP/1.1"));
            assert!(request.to_lowercase().contains("cookie: session=synthetic"));
        }
    }
}
