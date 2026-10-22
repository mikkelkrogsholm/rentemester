//! Canonical native latest-year Dinero source transport, without client parsing or automatic retries.
use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{ApiClient, Error, MAX_DINERO_IMPORT_BODY, dinero_import_body_schema};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn accepts(name: &str) -> bool {
    [
        "dinero_import_plan",
        "dinero_import_apply",
        "dinero_imports",
        "dinero_import_original",
        "invoice_imported_receivables",
    ]
    .contains(&name)
}
pub fn tools() -> Vec<Value> {
    let mut out = vec![];
    for (name, apply) in [("dinero_import_plan", false), ("dinero_import_apply", true)] {
        let mut body = native_body(dinero_import_body_schema(apply));
        body["maxLength"] = json!(MAX_DINERO_IMPORT_BODY);
        let mut schema = json!({"type":"object","properties":{"company":company_schema(),"body":body},"required":["company","body"],"additionalProperties":false});
        if apply {
            schema["properties"]["confirm"] = json!({"const":true});
            schema["required"]
                .as_array_mut()
                .unwrap()
                .push(json!("confirm"));
        }
        out.push(tool(name,"Review/apply latest-year Dinero journals, opening and imported source schedule atomically. Exact native35MiB source JSON; apply requires reviewed planHash, caller idempotencyKey, inner+outer confirm:true. Server owns strict CSV/source attestation/current admin/fences. Current human MFA CompanyAdmin for apply. No inferred currency, parsing/math/generatedkeys/retry. Reconcile unknown apply via canonical dinero_imports and original source hash before deliberate exact-key action.",schema,!apply));
    }
    out.push(tool("dinero_imports","Read canonical accepted latest-year Dinero imports and receipts; never retries an import.",json!({"type":"object","properties":{"company":company_schema()},"required":["company"],"additionalProperties":false}),true));
    out.push(tool("dinero_import_original","Read exact retained source package envelope original.contentType/application/json and original.bytes integers, max25MiB; current CompanyDocumentsRead.",json!({"type":"object","properties":{"company":company_schema(),"attemptId":{"type":"integer","minimum":1,"maximum":9007199254740991u64}},"required":["company","attemptId"],"additionalProperties":false}),true));
    out.push(tool("invoice_imported_receivables","Read externally issued source receivables as of an explicit date, including settled rows. Result retains ok/importedReceivables and adds identical schedule fields directly for original callers. No native issue, send or settlement actions; declared source hashes are not proof of physical invoice originals.",json!({"type":"object","properties":{"company":company_schema(),"asOf":{"type":"string","format":"date"}},"required":["company","asOf"],"additionalProperties":false}),true));
    out
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    company: String,
    body: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Apply {
    company: String,
    body: String,
    confirm: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Imports {
    company: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Original {
    company: String,
    attempt_id: i64,
}
pub async fn call(
    api: &ApiClient,
    name: &str,
    args: Value,
) -> Result<Result<Value, Error>, &'static str> {
    Ok(match name {
        "dinero_import_plan" => {
            let v: Plan = serde_json::from_value(args)
                .map_err(|_| "exact company and read-only body required")?;
            if v.body.len() > MAX_DINERO_IMPORT_BODY {
                return Err("native35MiB source body required");
            }
            api.dinero_import(&v.company, v.body.into_bytes(), false, false)
                .await
        }
        "dinero_import_apply" => {
            let v: Apply = serde_json::from_value(args)
                .map_err(|_| "exact company, body and confirm:true required")?;
            if !v.confirm || v.body.len() > MAX_DINERO_IMPORT_BODY {
                return Err("explicit confirmation and native35MiB source body required");
            }
            api.dinero_import(&v.company, v.body.into_bytes(), true, v.confirm)
                .await
        }
        "dinero_imports" => {
            let v: Imports = serde_json::from_value(args).map_err(|_| "exact company required")?;
            api.dinero_imports(&v.company).await
        }
        "invoice_imported_receivables" => {
            let v: Receivables =
                serde_json::from_value(args).map_err(|_| "exact company and asOf date required")?;
            api.imported_receivables(&v.company, &v.as_of)
                .await
                .map(flatten_receivables)
        }
        "dinero_import_original" => {
            let v: Original =
                serde_json::from_value(args).map_err(|_| "exact company and attemptId required")?;
            api.dinero_import_original(&v.company, v.attempt_id).await
        }
        _ => return Err("unknown dinero tool"),
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Receivables {
    company: String,
    as_of: String,
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dinero_catalogue_preserves_readonly_boundaries_and_double_confirmation() {
        let t = tools();
        assert_eq!(t.len(), 5);
        for tool in &t {
            assert_eq!(tool["inputSchema"]["additionalProperties"], false);
        }
        let apply = t
            .iter()
            .find(|v| v["name"] == "dinero_import_apply")
            .unwrap();
        assert_eq!(apply["inputSchema"]["properties"]["confirm"]["const"], true);
        assert!(
            serde_json::from_value::<Apply>(
                json!({"company":"synthetic","body":"{}","confirm":true,"actor":"spoof"})
            )
            .is_err()
        );
        assert!(serde_json::from_value::<Receivables>(json!({"company":"synthetic"})).is_err());
    }
}

fn flatten_receivables(mut value: Value) -> Value {
    if let Some(schedule) = value
        .get("importedReceivables")
        .and_then(Value::as_object)
        .cloned()
    {
        for (key, field) in schedule {
            if key != "ok" && key != "importedReceivables" {
                value[key] = field;
            }
        }
    }
    value
}

#[cfg(test)]
mod source_mapping_tests {
    use super::*;
    #[test]
    fn actual_tool_adds_exact_nonempty_schedule_and_retains_settled_rows() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
        };
        let schedule = json!({"asOfDate":"2026-10-20","boundary":"imported","count":2,"totalOpen":17.25,"rows":[{"invoiceNumber":"SYN-7","openAmount":17.25},{"invoiceNumber":"SYN-SETTLED","openAmount":0,"status":"paid"}]});
        let response = json!({"ok":true,"importedReceivables":schedule}).to_string();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut request = vec![];
            loop {
                let mut buf = [0; 1024];
                let n = socket.read(&mut buf).unwrap();
                assert!(n > 0);
                request.extend_from_slice(&buf[..n]);
                if request.windows(4).any(|b| b == b"\r\n\r\n") {
                    break;
                }
            }
            assert!(request.starts_with(b"GET /api/companies/synthetic/invoices/imported-receivables?asOf=2026-10-20 HTTP/1.1"));
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{}",
                response.len(),
                response
            )
            .unwrap();
        });
        let api = ApiClient::new(&origin, true, None).unwrap();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let result = runtime
            .block_on(crate::tools::call(
                &api,
                "invoice_imported_receivables",
                json!({"company":"synthetic","asOf":"2026-10-20"}),
            ))
            .unwrap();
        server.join().unwrap();
        let data = &result["structuredContent"]["data"];
        assert_eq!(data["ok"], true);
        assert_eq!(data["importedReceivables"], schedule);
        for (key, value) in schedule.as_object().unwrap() {
            assert_eq!(&data[key], value);
        }
    }
    #[test]
    fn additive_fields_retain_native_envelope_and_exact_schedule() {
        let schedule = json!({"asOfDate":"2026-10-20","boundary":"imported","count":2,"totalOpen":17.25,"rows":[{"invoiceNumber":"SYN-7","openAmount":17.25},{"invoiceNumber":"SYN-SETTLED","openAmount":0,"status":"paid"}]});
        let result = flatten_receivables(json!({"ok":true,"importedReceivables":schedule}));
        assert_eq!(result["ok"], true);
        assert_eq!(result["importedReceivables"], schedule);
        for (key, value) in schedule.as_object().unwrap() {
            assert_eq!(&result[key], value);
        }
    }
}
