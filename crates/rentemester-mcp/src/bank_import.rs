//! Original flat bank_import tool; exact receipts come from the shared server.
use rentemester_client::{ApiClient, Error};
use serde_json::{Value, json};
use std::path::Path;
pub fn tools() -> Vec<Value> {
    let mut tool = crate::tools::tool(
        "bank_import",
        "Import caller-local CSV bytes or inline CSV with explicit confirmation. Nonempty csvPath wins even if unreadable; never inline fallback.25MiB original/35MiB body. Existing server owns parsing/profile/source/audit/deduplication; no temporary files, accounting, automatic retry or default account selection.",
        json!({"type":"object","required":["company"],"properties":{"company":{"type":"string","minLength":1},"csvPath":{"type":"string"},"csvContent":{"type":"string"},"account":{"type":"string"},"profile":{"type":"string","enum":["danske-bank"]},"statementOrder":{"type":"string","enum":["ascending","descending"]},"confirm":{"type":"boolean"}},"additionalProperties":false}),
        false,
    );
    tool["annotations"] = json!({"readOnlyHint":false,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false});
    tool["outputSchema"] = json!({"type":"object","required":["ok","errors"],"properties":{"ok":{"type":"boolean"},"errors":{"type":"array","items":{"type":"string"}},"data":{"type":"object"},"code":{"type":"string"}},"additionalProperties":false});
    vec![tool]
}
fn result(value: Value) -> Value {
    json!({"content":[{"type":"text","text":value.to_string()}],"structuredContent":value,"isError":value["ok"]!=true})
}
fn failure(error: Error) -> Value {
    let code = match error {
        Error::InvalidInput(_) => "invalid_input",
        Error::Refused { .. } => "server_refused",
        Error::Transport => "transport",
        Error::Protocol => "protocol",
        Error::OutcomeUnknown => "outcome_unknown",
    };
    result(json!({"ok":false,"errors":[error.to_string()],"code":code}))
}
pub async fn call(api: &ApiClient, args: Value) -> Result<Value, &'static str> {
    let object = args.as_object().ok_or("bank_import requires an object")?;
    if object.keys().any(|k| {
        ![
            "company",
            "csvPath",
            "csvContent",
            "account",
            "profile",
            "statementOrder",
            "confirm",
        ]
        .contains(&k.as_str())
    }) {
        return Err("unknown bank_import argument");
    }
    let company = object
        .get("company")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or("nonempty company required")?;
    for key in ["csvPath", "csvContent", "account", "profile", "statementOrder"] {
        if object.get(key).is_some_and(|v| !v.is_string()) {
            return Err("optional import arguments must be strings, never null");
        }
    }
    if object.get("confirm").is_some_and(|v| !v.is_boolean()) {
        return Err("confirm must be boolean");
    }
    if object.get("profile").is_some_and(|v| v != "danske-bank") {
        return Err("profile must be danske-bank");
    }
    if object.get("statementOrder").is_some_and(|v|v!="ascending" && v!="descending") {return Err("statementOrder must be ascending or descending");}
    if object.get("confirm") != Some(&json!(true)) {
        return Ok(result(
            json!({"ok":false,"code":"CONFIRM_REQUIRED","errors":["confirm: true required for write tool bank_import"]}),
        ));
    }
    let account = object
        .get("account")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty());
    let profile = object.get("profile").and_then(Value::as_str);
    let body = if let Some(path) = object
        .get("csvPath")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        match rentemester_client::bank_import_ordered_file_body(Path::new(path), account, profile, object.get("statementOrder").and_then(Value::as_str)) {
            Ok(body) => body,
            Err(e) => return Ok(failure(e)),
        }
    } else {
        let Some(content) = object
            .get("csvContent")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        else {
            return Ok(result(
                json!({"ok":false,"errors":["either csvPath or csvContent is required"]}),
            ));
        };
        let mut body = json!({"confirm":true,"csvContent":content});
        if let Some(account) = account {
            body["account"] = json!(account);
        }
        if let Some(profile) = profile {
            body["profile"] = json!(profile);
        }
        serde_json::to_vec(&body).map_err(|_| "import body unavailable")?
    };
    let mut body:Value=serde_json::from_slice(&body).map_err(|_|"import body unavailable")?;if let Some(order)=object.get("statementOrder") {body["statementOrder"]=order.clone();}
    let body=serde_json::to_vec(&body).map_err(|_|"import body unavailable")?;
    Ok(match api.bank_import_original(company, body).await {
        Ok(value) => result(value),
        Err(e) => failure(e),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn api() -> ApiClient {
        ApiClient::new("http://127.0.0.1:9", true, None).unwrap()
    }
    async fn confirmation_schema_and_nonexistent_path_precedence_stop_before_network() {
        for confirm in [None, Some(false)] {
            let mut args = json!({"company":"synthetic","csvPath":"/definitely/missing/rm142.csv","csvContent":"inline"});
            if let Some(confirm) = confirm {
                args["confirm"] = json!(confirm);
            }
            let result = crate::tools::call(&api(), "bank_import", args)
                .await
                .unwrap();
            assert_eq!(result["structuredContent"]["code"], "CONFIRM_REQUIRED");
            assert_eq!(result["isError"], true);
        }
        let result=call(&api(),json!({"company":"synthetic","csvPath":"/definitely/missing/rm142.csv","csvContent":"inline","confirm":true})).await.unwrap();
        assert!(
            result["structuredContent"]["errors"][0]
                .as_str()
                .unwrap()
                .contains("file could not be opened")
        );
        assert_eq!(result["structuredContent"]["code"], "invalid_input");
        for args in [
            json!({"company":"synthetic","confirm":null}),
            json!({"company":"synthetic","csvPath":null}),
            json!({"company":"synthetic","csvContent":42}),
            json!({"company":"synthetic","account":null}),
            json!({"company":"synthetic","profile":"unknown"}),
            json!({"company":"synthetic","body":"{}"}),
        ] {
            assert!(call(&api(), args).await.is_err());
        }
        assert_eq!(
            crate::tools::list()["tools"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|t| t["name"] == "bank_import")
                .count(),
            1
        );
    }
    async fn empty_path_uses_inline_and_exact_server_envelope_matches_text() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
            thread,
        };
        for value in [
            json!({"ok":true,"data":{"imported":0,"skippedDuplicates":1,"exceptionsCreated":0},"errors":[]}),
            json!({"ok":false,"errors":["server-owned refusal"]}),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let origin = format!("http://{}", listener.local_addr().unwrap());
            let response = value.to_string();
            let thread = thread::spawn(move || {
                let (mut s, _) = listener.accept().unwrap();
                s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .unwrap();
                let mut request = vec![];
                let end = loop {
                    let mut b = [0; 1024];
                    let n = s.read(&mut b).unwrap();
                    assert!(n > 0);
                    request.extend_from_slice(&b[..n]);
                    if let Some(i) = request.windows(4).position(|b| b == b"\r\n\r\n") {
                        break i + 4;
                    }
                };
                let header = String::from_utf8_lossy(&request[..end]);
                assert!(header.starts_with("POST /api/companies/synthetic/bank/import-original "));
                let length: usize = header
                    .lines()
                    .find_map(|l| {
                        l.to_lowercase()
                            .strip_prefix("content-length:")
                            .map(|v| v.trim().parse().unwrap())
                    })
                    .unwrap();
                while request.len() < end + length {
                    let mut b = [0; 1024];
                    let n = s.read(&mut b).unwrap();
                    assert!(n > 0);
                    request.extend_from_slice(&b[..n]);
                }
                write!(
                    s,
                    "HTTP/1.1 200 OK\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{response}",
                    response.len()
                )
                .unwrap();
                serde_json::from_slice::<Value>(&request[end..]).unwrap()
            });
            let api = ApiClient::new(&origin, true, None).unwrap();
            let result=call(&api,json!({"company":"synthetic","csvPath":"","csvContent":"raw\r\ninline","confirm":true})).await.unwrap();
            assert_eq!(result["structuredContent"], value);
            assert_eq!(
                serde_json::from_str::<Value>(result["content"][0]["text"].as_str().unwrap())
                    .unwrap(),
                value
            );
            assert_eq!(result["isError"], value["ok"] != true);
            assert_eq!(
                thread.join().unwrap(),
                json!({"confirm":true,"csvContent":"raw\r\ninline"})
            );
        }
    }
    #[test]
    fn original_confirmation_and_path_precedence() {
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(confirmation_schema_and_nonexistent_path_precedence_stop_before_network());
    }
    #[test]
    fn original_inline_envelope() {
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(empty_path_uses_inline_and_exact_server_envelope_matches_text());
    }
}
