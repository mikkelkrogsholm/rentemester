use super::*;
use std::time::{Duration, Instant};

fn customer(id: i64, name: &str) -> Value {
    json!({"id":id,"name":name,"address":null,"vatOrCvr":"DK12345678","email":null,"phone":"12345678","website":null,"eanNumber":null,"paymentTermsDays":null,"defaultCurrency":"DKK","notes":null,"archived":false,"createdAt":"2026-10-22T12:00:00Z"})
}
fn vendor(id: i64, name: &str) -> Value {
    json!({"id":id,"name":name,"address":"Synthetic street 1, 1000 Synthetic","vatOrCvr":"DK87654321","countryCode":"DK","identifierKind":"dk_cvr","identityStatus":"resolved","email":null,"phone":null,"website":null,"defaultExpenseAccount":null,"defaultVatTreatment":null,"notes":null,"archived":true,"createdAt":"2026-10-22T12:00:00Z"})
}
fn validation(valid: bool) -> Value {
    json!({"ok":true,"validation":{"countryCode":"DK","vatNumber":"12345678","normalized":"DK12345678","valid":valid,"name":null,"address":null,"validatedAt":"2026-10-22T12:00:00Z","expiresAt":"2026-10-23T12:00:00Z","rawResponse":null},"appliedRules":["DK-VAT-REVERSE-CHARGE-001"],"errors":[]})
}
// A five-second startup deadline is separate from the bounded post-response
// observation. Accepted sockets are blocking, including after nonblocking accept.
fn mock(
    core: Value,
    status: &'static str,
) -> (String, mpsc::Receiver<String>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let (send, recv) = mpsc::channel();
    let task = thread::spawn(move || {
        listener.set_nonblocking(true).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut socket = loop {
            match listener.accept() {
                Ok((socket, _)) => break socket,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        Instant::now() < deadline,
                        "MCP did not connect within startup deadline"
                    );
                    thread::sleep(Duration::from_millis(5));
                }
                Err(e) => panic!("accept: {e}"),
            }
        };
        socket.set_nonblocking(false).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut bytes = Vec::new();
        loop {
            let mut buffer = [0; 4096];
            let n = socket.read(&mut buffer).unwrap();
            assert!(n > 0, "incomplete request");
            bytes.extend_from_slice(&buffer[..n]);
            if let Some(i) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..i]).to_lowercase();
                let length = headers
                    .lines()
                    .find_map(|s| s.strip_prefix("content-length: "))
                    .map(|s| s.parse::<usize>().unwrap())
                    .unwrap_or(0);
                if bytes.len() >= i + 4 + length {
                    break;
                }
            }
        }
        send.send(String::from_utf8(bytes).unwrap()).unwrap();
        let body = core.to_string();
        socket.write_all(format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{body}",body.len()).as_bytes()).unwrap();
        drop(socket);
        let until = Instant::now() + Duration::from_millis(150);
        while Instant::now() < until {
            assert!(
                matches!(listener.accept(),Err(e) if e.kind()==std::io::ErrorKind::WouldBlock),
                "unexpected retry"
            );
            thread::sleep(Duration::from_millis(5));
        }
    });
    (origin, recv, task)
}
fn invoke(name: &str, args: Value, core: Value, status: &'static str) -> (Value, String) {
    let (origin, recv, task) = mock(core, status);
    let output = run(&origin, &[init("2025-11-25"), ready(), call(2, name, args)]);
    let rows = parsed(&output);
    let wire = recv.recv_timeout(Duration::from_secs(5)).unwrap();
    task.join().unwrap();
    let result = &rows[1]["result"];
    let envelope = result["structuredContent"].clone();
    assert_eq!(
        serde_json::from_str::<Value>(result["content"][0]["text"].as_str().unwrap()).unwrap(),
        envelope
    );
    assert_eq!(result["isError"], envelope["ok"] != true);
    assert!(!String::from_utf8_lossy(&output.stdout).contains("never-print-this"));
    (envelope, wire)
}
#[test]
fn master_data_original_catalogue_exact_five_schemas_and_annotations() {
    let rows = parsed(&run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        ],
    ));
    let tools = rows[1]["result"]["tools"].as_array().unwrap();
    let names: std::collections::BTreeSet<_> = tools
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert_eq!(names.len(), tools.len(), "duplicate tool names");
    assert_eq!(tools.len(), EXPECTED_NATIVE_TOOLS);
    for name in [
        "customer_list",
        "vendor_list",
        "customer_create",
        "vendor_create",
        "customer_validate_vat",
    ] {
        let tool = tools.iter().find(|tool| tool["name"] == name).unwrap();
        let read = name.ends_with("_list");
        let vat = name == "customer_validate_vat";
        assert_eq!(
            tool["annotations"],
            json!({"readOnlyHint":read,"destructiveHint":false,"idempotentHint":read || vat,"openWorldHint":vat})
        );
        assert_eq!(tool["inputSchema"]["additionalProperties"], false);
        assert_eq!(tool["outputSchema"]["required"], json!(["ok", "errors"]));
        assert_eq!(
            tool["outputSchema"]["properties"]["errors"],
            json!({"type":"array","items":{"type":"string"}})
        );
        let required = tool["inputSchema"]["required"].as_array().unwrap();
        assert!(!required.contains(&json!("confirm")));
        if read {
            assert_eq!(tool["inputSchema"]["properties"]["limit"]["maximum"], 5000);
            assert_eq!(tool["inputSchema"]["properties"]["limit"]["default"], 500);
        } else {
            assert_eq!(
                tool["inputSchema"]["properties"]["confirm"],
                json!({"type":"boolean"})
            );
            if !vat {
                let input = &tool["inputSchema"]["properties"]["input"];
                assert_eq!(input["additionalProperties"], false);
                assert!(
                    input
                        .get("required")
                        .is_none_or(|keys| keys.as_array().unwrap().is_empty())
                );
                assert!(input["properties"].get("note").is_none());
                assert!(input["properties"]["notes"].is_object());
            }
        }
    }
    for name in [
        "contacts",
        "create_customer",
        "create_vendor",
        "cvr_lookup",
        "vies_validate",
    ] {
        assert!(names.contains(name), "native tool removed: {name}");
    }
}
#[test]
fn master_data_original_offline_confirmation_and_invalid_schema() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    for name in ["customer_create", "vendor_create", "customer_validate_vat"] {
        let base = if name == "customer_validate_vat" {
            json!({"company":"synthetic","cvr":"DK12345678"})
        } else {
            json!({"company":"synthetic","input":{}})
        };
        for confirm in [None, Some(false)] {
            let mut args = base.clone();
            if let Some(confirm) = confirm {
                args["confirm"] = json!(confirm);
            }
            let rows = parsed(&run(
                &origin,
                &[init("2025-11-25"), ready(), call(2, name, args)],
            ));
            assert_eq!(
                rows[1]["result"]["structuredContent"],
                json!({"ok":false,"code":"CONFIRM_REQUIRED","errors":[format!("confirm: true required for write tool {name}")]})
            );
        }
    }
    for (name, args) in [
        (
            "customer_create",
            json!({"company":"synthetic","input":{"name":""},"confirm":true}),
        ),
        (
            "customer_create",
            json!({"company":"synthetic","input":{"name":null},"confirm":true}),
        ),
        (
            "customer_create",
            json!({"company":"synthetic","input":{"paymentTermsDays":0},"confirm":true}),
        ),
        (
            "customer_create",
            json!({"company":"synthetic","input":{"paymentTermsDays":1.5},"confirm":true}),
        ),
        (
            "customer_create",
            json!({"company":"synthetic","input":{"note":"unknown"},"confirm":true}),
        ),
        (
            "customer_create",
            json!({"company":"synthetic","input":{},"confirm":null}),
        ),
        (
            "customer_create",
            json!({"company":"synthetic","input":{},"fromCvr":null,"confirm":true}),
        ),
        (
            "vendor_create",
            json!({"company":"synthetic","input":{"countryCode":"DNK"},"confirm":true}),
        ),
        (
            "vendor_create",
            json!({"company":"synthetic","input":{"identifierKind":"unknown"},"confirm":true}),
        ),
        (
            "vendor_create",
            json!({"company":"synthetic","input":{"actor":"forged"},"confirm":true}),
        ),
        (
            "vendor_create",
            json!({"company":"synthetic","input":{},"authority":"owner","confirm":true}),
        ),
        (
            "customer_validate_vat",
            json!({"company":"synthetic","cvr":"DK12345678","confirm":"true"}),
        ),
        (
            "customer_validate_vat",
            json!({"company":"synthetic","cvr":"","confirm":true}),
        ),
        (
            "customer_validate_vat",
            json!({"company":"synthetic","cvr":"DK12345678","endpoint":"https://example.invalid","confirm":true}),
        ),
        (
            "customer_list",
            json!({"company":"synthetic","archived":null}),
        ),
        ("customer_list", json!({"company":"synthetic","limit":5001})),
        ("customer_list", json!({"company":"synthetic","limit":0})),
        ("vendor_list", json!({"company":"synthetic","offset":-1})),
        ("vendor_list", json!({"company":"synthetic","limit":1.5})),
        ("vendor_list", json!({"company":"","archived":true})),
    ] {
        let rows = parsed(&run(
            &origin,
            &[init("2025-11-25"), ready(), call(2, name, args)],
        ));
        assert_eq!(rows[1]["error"]["code"], -32602, "{name}");
    }
    listener.set_nonblocking(true).unwrap();
    assert!(matches!(listener.accept(),Err(e) if e.kind()==std::io::ErrorKind::WouldBlock));
}
#[test]
fn master_data_original_full_rows_nulls_and_pagination_preserve_server_order() {
    for (name, rows) in [
        (
            "customer_list",
            json!([
                customer(4, "alpha"),
                customer(9, "Alpha"),
                customer(2, "Beta")
            ]),
        ),
        (
            "vendor_list",
            json!([vendor(4, "alpha"), vendor(9, "Alpha"), vendor(2, "Beta")]),
        ),
    ] {
        for (limit, offset, expected, more) in [
            (2, 0, json!([rows[0], rows[1]]), true),
            (2, 2, json!([rows[2]]), false),
            (5000, 99, json!([]), false),
        ] {
            let (envelope, wire) = invoke(
                name,
                json!({"company":"synthetic","archived":true,"limit":limit,"offset":offset}),
                json!({"ok":true,"count":3,"rows":rows,"errors":[]}),
                "200 OK",
            );
            let plural = if name == "customer_list" {
                "customers"
            } else {
                "vendors"
            };
            assert!(wire.starts_with(&format!(
                "GET /api/companies/synthetic/master-data/original/{plural}?archived=true HTTP/1.1"
            )));
            let data = &envelope["data"];
            assert_eq!(data["rows"], expected);
            assert_eq!(data["total"], 3);
            assert_eq!(data["count"], expected.as_array().unwrap().len());
            assert_eq!(data["limit"], limit);
            assert_eq!(data["offset"], offset);
            assert_eq!(data["hasMore"], more);
            assert_eq!(data.get("nextOffset").is_some(), more);
            if more {
                assert_eq!(data["nextOffset"], 2);
            }
        }
    }
    let rows: Vec<_> = (1..=501).map(|id| customer(id, "same")).collect();
    let (envelope, wire) = invoke(
        "customer_list",
        json!({"company":"synthetic"}),
        json!({"ok":true,"count":501,"rows":rows,"errors":[]}),
        "200 OK",
    );
    assert!(wire.contains("?archived=false HTTP/1.1"));
    assert_eq!(envelope["data"]["count"], 500);
    assert_eq!(envelope["data"]["limit"], 500);
    assert_eq!(envelope["data"]["offset"], 0);
    assert_eq!(envelope["data"]["nextOffset"], 500);
}
#[test]
fn master_data_original_create_exact_wire_and_source_envelope() {
    for (name, id_field, rule, input) in [
        (
            "customer_create",
            "customerId",
            "DK-MASTER-DATA-CUSTOMER-001",
            json!({"name":" Manual override ","address":"Explicit address","vatOrCvr":"DK12345678","email":"synthetic@example.invalid","phone":"12345678","website":"https://example.invalid","eanNumber":"5790000000000","paymentTermsDays":7,"defaultCurrency":" eur ","notes":"Synthetic"}),
        ),
        (
            "vendor_create",
            "vendorId",
            "DK-MASTER-DATA-VENDOR-001",
            json!({"name":"Manual vendor","address":"Synthetic","vatOrCvr":"DK87654321","countryCode":"DK","identifierKind":"dk_cvr","email":"vendor@example.invalid","phone":"87654321","website":"https://example.invalid","defaultExpenseAccount":"3000","defaultVatTreatment":"standard","notes":"Synthetic"}),
        ),
    ] {
        let mut core = json!({"ok":true,"appliedRules":[rule],"errors":[]});
        core[id_field] = json!(17);
        let (envelope, wire) = invoke(
            name,
            json!({"company":"synthetic","input":input,"fromCvr":"12345678","confirm":true}),
            core,
            "200 OK",
        );
        let plural = if name == "customer_create" {
            "customers"
        } else {
            "vendors"
        };
        assert!(wire.starts_with(&format!(
            "POST /api/companies/synthetic/master-data/original/{plural} HTTP/1.1"
        )));
        let body: Value = serde_json::from_str(wire.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(
            body,
            json!({"input":input,"fromCvr":"12345678","confirm":true})
        );
        assert_eq!(
            envelope,
            json!({"ok":true,"data":{(id_field):17},"appliedRules":[rule],"errors":[]})
        );
    }
    let (envelope, wire) = invoke(
        "customer_create",
        json!({"company":"synthetic","input":{},"fromCvr":"12345678","confirm":true}),
        json!({"ok":true,"customerId":18,"appliedRules":["DK-MASTER-DATA-CUSTOMER-001"],"errors":[]}),
        "200 OK",
    );
    assert_eq!(envelope["data"]["customerId"], 18);
    assert!(wire.contains("\"input\":{}"));
}
#[test]
fn master_data_original_vat_valid_false_is_success_and_each_call_uses_provider_route() {
    for valid in [true, false] {
        let core = validation(valid);
        let (envelope, wire) = invoke(
            "customer_validate_vat",
            json!({"company":"synthetic","cvr":"DK 12.345.678","confirm":true}),
            core.clone(),
            "200 OK",
        );
        assert!(wire.starts_with(
            "POST /api/companies/synthetic/master-data/original/customer-validate-vat HTTP/1.1"
        ));
        let body: Value = serde_json::from_str(wire.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(body, json!({"cvr":"DK 12.345.678","confirm":true}));
        assert_eq!(envelope["ok"], true);
        assert_eq!(envelope["data"], json!({"validation":core["validation"]}));
        assert_eq!(envelope["appliedRules"], core["appliedRules"]);
    }
}
#[test]
fn master_data_original_business_failures_keep_actual_errors_code_rules_without_data() {
    for status in ["200 OK", "422 Unprocessable Entity"] {
        let core = json!({"ok":false,"code":"human_resolution_required","status":"human_resolution_required","errors":["Supplier identity requires human resolution"],"appliedRules":["DK-MASTER-DATA-VENDOR-001"]});
        let (envelope, _) = invoke(
            "vendor_create",
            json!({"company":"synthetic","input":{"name":"Synthetic"},"confirm":true}),
            core.clone(),
            status,
        );
        assert_eq!(envelope["code"], core["code"]);
        assert_eq!(envelope["errors"], core["errors"]);
        assert_eq!(envelope["appliedRules"], core["appliedRules"]);
        assert!(envelope.get("data").is_none());
    }
    let (envelope, _) = invoke(
        "customer_validate_vat",
        json!({"company":"synthetic","cvr":"DE333333333","confirm":true}),
        json!({"ok":false,"code":"provider_unavailable","errors":["VIES unavailable"],"appliedRules":["DK-VAT-REVERSE-CHARGE-001"]}),
        "422 Unprocessable Entity",
    );
    assert_eq!(envelope["errors"], json!(["VIES unavailable"]));
    assert!(envelope.get("data").is_none());
}
#[test]
fn master_data_original_authority_redaction_and_unknown_outcome_no_retry() {
    for (status, core, code) in [
        (
            "401 Unauthorized",
            json!({"private":"never-print-this"}),
            "server_refused",
        ),
        (
            "403 Forbidden",
            json!({"private":"never-print-this"}),
            "server_refused",
        ),
        (
            "200 OK",
            json!({"ok":true,"private":"never-print-this"}),
            "outcome_unknown",
        ),
        (
            "503 Unavailable",
            json!({"private":"never-print-this"}),
            "outcome_unknown",
        ),
    ] {
        let (envelope, _) = invoke(
            "customer_create",
            json!({"company":"synthetic","input":{"name":"Synthetic"},"confirm":true}),
            core,
            status,
        );
        assert_eq!(envelope["code"], code);
        assert!(envelope.get("data").is_none());
    }
}
