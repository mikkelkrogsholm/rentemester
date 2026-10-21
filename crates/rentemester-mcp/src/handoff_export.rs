//! Original irreversible authority export with process-local verified materialization.
use crate::tools::tool;
use rentemester_client::{ApiClient, Error, write_handoff_package};
use serde_json::{Value, json};
use std::path::Path;
pub fn tools() -> Vec<Value> {
    let string = json!({"type":"string","minLength":1});
    let mut t = tool(
        "system_export_authority",
        "Export authority handover material once, auditing the authenticated server principal. confirm:true is required before HTTP or filesystem effects. company is a connected-server slug; filesystem/default launcher compatibility remains pending R44. out is the MCP PROCESS filesystem (not the HTTP server): verified package is created beneath it in a fresh root; no overwrite. Request context is preserved by server, no client time/math. Lost response/local failure must not trigger automatic re-export. No filing or signed-backup claim.",
        json!({"type":"object","required":["company","from","to","out"],"properties":{"company":string,"from":string,"to":string,"out":string,"requestedAt":{"type":"string"},"requester":{"type":"string"},"confirm":{"type":"boolean"}},"additionalProperties":false}),
        false,
    );
    t["annotations"]["destructiveHint"] = json!(false);
    t["annotations"]["openWorldHint"] = json!(false);
    t["outputSchema"] = json!({"type":"object","required":["ok","errors"],"properties":{"ok":{"type":"boolean"},"errors":{"type":"array","items":{"type":"string"}},"data":{"type":"object","additionalProperties":true},"appliedRules":{"type":"array","items":{"type":"string"}},"code":{"type":"string"}},"additionalProperties":false});
    vec![t]
}
fn result(value: Value) -> Value {
    json!({"isError":value["ok"]==false,"content":[{"type":"text","text":value.to_string()}],"structuredContent":value})
}
fn failure(message: &str, code: &str) -> Value {
    result(json!({"ok":false,"errors":[message],"code":code}))
}
pub async fn call(api: &ApiClient, args: Value) -> Result<Value, &'static str> {
    let object = args
        .as_object()
        .ok_or("system_export_authority requires object arguments")?;
    if object.keys().any(|k| {
        ![
            "company",
            "from",
            "to",
            "out",
            "requestedAt",
            "requester",
            "confirm",
        ]
        .contains(&k.as_str())
    }) {
        return Err("unknown system_export_authority argument");
    }
    let required = |key| {
        object
            .get(key)
            .and_then(Value::as_str)
            .filter(|v| !v.is_empty())
            .ok_or("company/from/to/out must be nonempty strings")
    };
    let (company, from, to, out) = (
        required("company")?,
        required("from")?,
        required("to")?,
        required("out")?,
    );
    for key in ["requestedAt", "requester"] {
        if object.get(key).is_some_and(|v| !v.is_string()) {
            return Err("optional request context must be strings, not null");
        }
    }
    if object.get("confirm").is_some_and(|v| !v.is_boolean()) {
        return Err("confirm must be boolean when present");
    }
    if object.get("confirm").and_then(Value::as_bool) != Some(true) {
        return Ok(failure(
            "Explicit confirm:true is required before authority export HTTP or local filesystem effects",
            "confirmation_required",
        ));
    }
    if !company
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
    {
        return Ok(failure(
            "Use a connected-server company slug; filesystem/default company launcher compatibility remains pending R44",
            "local_company_unavailable",
        ));
    }
    let mut body = json!({"periodStart":from,"periodEnd":to,"confirm":true});
    for key in ["requestedAt", "requester"] {
        if let Some(value) = object.get(key) {
            body[key] = value.clone();
        }
    }
    let download = match api
        .handoff_export(
            company,
            "authority",
            serde_json::to_vec(&body).unwrap(),
            true,
        )
        .await
    {
        Ok(value) => value,
        Err(error) => {
            let code = if error == Error::OutcomeUnknown {
                "outcome_unknown"
            } else {
                "export_refused"
            };
            return Ok(failure(
                &format!("{error}. No automatic retry: export audit may already be committed."),
                code,
            ));
        }
    };
    let value = match write_handoff_package(&download, Path::new(out)) {
        Ok(value) => value,
        Err(e) => {
            return Ok(failure(
                &format!(
                    "{e}. Server export audit may already be committed; never automatically export again."
                ),
                "local_export_failed",
            ));
        }
    };
    Ok(success(value))
}
fn success(mut value: Value) -> Value {
    let object = value.as_object_mut().unwrap();
    let rules = object.remove("appliedRules").unwrap_or_else(|| json!([]));
    object.remove("ok");
    object.remove("errors");
    result(json!({"ok":true,"data":value,"errors":[],"appliedRules":rules}))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_success_envelope_preserves_exact_flat_metadata() {
        let metadata = json!({"ok":true,"exportDir":"/local/package","manifestPath":"/local/package/manifest.json","periodStart":"2026-01-01","periodEnd":"2026-12-31","generatedAt":"2027-01-01T12:00:00Z","deadlineAt":null,"journalEntryCount":2,"bankTransactionCount":0,"documentCount":1,"appliedRules":["DK-BOOKKEEPING-AUTHORITY-EXPORT-001"],"errors":[]});
        let value = success(metadata.clone());
        assert_eq!(value["isError"], false);
        let mut flat = value["structuredContent"]["data"].clone();
        for key in ["ok", "errors", "appliedRules"] {
            flat[key] = value["structuredContent"][key].clone();
        }
        assert_eq!(flat, metadata);
        assert_eq!(
            serde_json::from_str::<Value>(value["content"][0]["text"].as_str().unwrap()).unwrap(),
            value["structuredContent"]
        );
    }
    #[test]
    fn original_schema_and_confirmation_precede_all_effects() {
        let t = tools().remove(0);
        for key in [
            "readOnlyHint",
            "destructiveHint",
            "idempotentHint",
            "openWorldHint",
        ] {
            assert_eq!(t["annotations"][key], false);
        }
        assert_eq!(t["inputSchema"]["additionalProperties"], false);
        let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let out = std::env::temp_dir().join(format!("rm136-not-created-{}", std::process::id()));
        for confirm in [None, Some(false)] {
            let mut args =
                json!({"company":"synthetic","from":"2020-01-01","to":"2020-12-31","out":out});
            if let Some(confirm) = confirm {
                args["confirm"] = json!(confirm);
            }
            let value = rt
                .block_on(crate::tools::call(&api, "system_export_authority", args))
                .unwrap();
            assert_eq!(value["structuredContent"]["code"], "confirmation_required");
            assert!(value["structuredContent"].get("data").is_none());
            assert!(!out.exists());
        }
        for extra in [
            json!({"confirm":null}),
            json!({"requestedAt":null}),
            json!({"actor":"forged"}),
        ] {
            let mut args =
                json!({"company":"synthetic","from":"2020-01-01","to":"2020-12-31","out":out});
            args.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            assert!(rt.block_on(call(&api, args)).is_err());
        }
    }
    #[test]
    fn lost_export_response_is_outcome_unknown_and_is_not_retried() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
            thread,
        };
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let task = thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut b = [0; 8192];
            let received = s.read(&mut b).unwrap();
            assert!(received > 0);
            s.write_all(b"HTTP/1.1 500 Internal Server Error\r\nContent-Length:0\r\nConnection:close\r\n\r\n").unwrap();
        });
        let api = ApiClient::new(&base, true, None).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let out = std::env::temp_dir().join(format!("rm136-unknown-{}", std::process::id()));
        let value=rt.block_on(crate::tools::call(&api,"system_export_authority",json!({"company":"synthetic","from":"2020-01-01","to":"2020-12-31","out":out,"confirm":true}))).unwrap();
        assert_eq!(value["isError"], true);
        assert_eq!(value["structuredContent"]["code"], "outcome_unknown");
        assert!(!out.exists());
        task.join().unwrap();
    }
}
