use super::*;
use std::time::{Duration, Instant};

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
fn documents_original_registry_exact_schemas_annotations_and_native_aliases() {
    let rows = parsed(&run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        ],
    ));
    let tools = rows[1]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), EXPECTED_NATIVE_TOOLS);
    for (name, required, read, idempotent) in [
        (
            "documents_ingest",
            vec!["company", "metadata", "filePath"],
            false,
            false,
        ),
        ("documents_list", vec!["company"], true, true),
        (
            "documents_enrich",
            vec!["company", "metadata", "documentId"],
            false,
            true,
        ),
        (
            "documents_set_company_context",
            vec![
                "company",
                "sourceReference",
                "businessUseReason",
                "documentId",
            ],
            false,
            true,
        ),
        (
            "documents_parse",
            vec!["company", "documentId"],
            false,
            true,
        ),
        ("documents_parse_pending", vec!["company"], false, true),
        (
            "documents_parse_status",
            vec!["company", "documentId"],
            true,
            true,
        ),
        (
            "documents_parsed_text",
            vec!["company", "documentId"],
            true,
            true,
        ),
    ] {
        assert_eq!(tools.iter().filter(|t| t["name"] == name).count(), 1);
        let t = tools.iter().find(|t| t["name"] == name).unwrap();
        assert_eq!(
            t["annotations"],
            json!({"readOnlyHint":read,"destructiveHint":false,"idempotentHint":idempotent,"openWorldHint":false})
        );
        let actual = t["inputSchema"]["required"].as_array().unwrap();
        assert_eq!(actual.len(), required.len());
        for field in required {
            assert!(actual.contains(&json!(field)));
        }
        assert_eq!(t["inputSchema"]["additionalProperties"], false);
        assert_eq!(t["outputSchema"]["required"], json!(["ok", "errors"]));
        if !read {
            assert_eq!(
                t["inputSchema"]["properties"]["confirm"],
                json!({"type":"boolean"})
            );
        }
        assert!(t["inputSchema"]["properties"].get("fileBase64").is_none());
        assert!(t["inputSchema"]["properties"].get("actor").is_none());
    }
    for name in [
        "ingest_document",
        "documents",
        "document_original",
        "parse_document",
        "document_parse_status",
        "document_parsed_text",
        "parse_pending_documents",
    ] {
        assert!(tools.iter().any(|t| t["name"] == name));
    }
}
#[test]
fn documents_original_confirmation_precedes_local_file_and_http() {
    // A FIFO would block forever if touched before the confirmation latch.
    let file = Cookie::new();
    std::fs::remove_file(&file.0).unwrap();
    assert!(
        Command::new("mkfifo")
            .arg(&file.0)
            .status()
            .unwrap()
            .success()
    );
    for confirm in [None, Some(false)] {
        let mut messages = vec![init("2025-11-25"), ready()];
        for (i,(name,mut args)) in [
            ("documents_ingest",json!({"company":"synthetic","filePath":file.0,"metadata":{"source":"synthetic"}})),
            ("documents_enrich",json!({"company":"synthetic","documentId":1,"metadata":{"source":"synthetic"}})),
            ("documents_set_company_context",json!({"company":"synthetic","documentId":1,"sourceReference":"source","businessUseReason":"business"})),
            ("documents_parse",json!({"company":"synthetic","documentId":1})),
            ("documents_parse_pending",json!({"company":"synthetic"})),
        ].into_iter().enumerate() {
            if let Some(confirm)=confirm { args["confirm"]=json!(confirm); }
            messages.push(call(i as i32+2,name,args));
        }
        let rows = parsed(&run("http://127.0.0.1:9", &messages));
        for (row, name) in rows[1..].iter().zip([
            "documents_ingest",
            "documents_enrich",
            "documents_set_company_context",
            "documents_parse",
            "documents_parse_pending",
        ]) {
            let expected = json!({"ok":false,"errors":[format!("confirm: true required for write tool {name}")],"code":"CONFIRM_REQUIRED"});
            assert_eq!(row["result"]["structuredContent"], expected);
            assert_eq!(row["result"]["isError"], true);
            assert_eq!(
                serde_json::from_str::<Value>(
                    row["result"]["content"][0]["text"].as_str().unwrap()
                )
                .unwrap(),
                expected
            );
        }
    }
}
#[test]
fn documents_original_invalid_arguments_are_rpc_errors_before_transport() {
    for (name, args) in [
        (
            "documents_ingest",
            json!({"company":"a","filePath":"missing","metadata":{"source":"x"},"confirm":null}),
        ),
        (
            "documents_ingest",
            json!({"company":"a","filePath":"","metadata":{"source":"x"}}),
        ),
        (
            "documents_ingest",
            json!({"company":"a","filePath":"missing","metadata":{}}),
        ),
        (
            "documents_ingest",
            json!({"company":"a","filePath":"missing","metadata":{"source":"x","amountIncVat":"1"}}),
        ),
        (
            "documents_ingest",
            json!({"company":"a","filePath":"missing","metadata":{"source":"x","purchaseVatLines":[]}}),
        ),
        (
            "documents_ingest",
            json!({"company":"a","filePath":"missing","metadata":{"source":"x","sender":{"countryCode":"DNK"}}}),
        ),
        (
            "documents_ingest",
            json!({"company":"a","filePath":"missing","metadata":{"source":"x","purchaseVatLines":[{"classification":"exempt","netAmount":-1}]}}),
        ),
        (
            "documents_ingest",
            json!({"company":"a","filePath":"missing","metadata":{"source":"x"},"vendorId":null}),
        ),
        (
            "documents_enrich",
            json!({"company":"a","documentId":0,"metadata":{"source":"x"}}),
        ),
        (
            "documents_parse",
            json!({"company":"a","documentId":1,"confirm":"true"}),
        ),
        (
            "documents_parse_status",
            json!({"company":"a","documentId":1,"confirm":true}),
        ),
        ("documents_list", json!({"company":"a","limit":5001})),
        ("documents_list", json!({"company":"a","offset":-1})),
        ("documents_list", json!({"company":"a","limit":null})),
        ("documents_list", json!({"company":""})),
        (
            "documents_parse_pending",
            json!({"company":"a","limit":101}),
        ),
        (
            "documents_parsed_text",
            json!({"company":"a","documentId":1,"limit":11}),
        ),
        (
            "documents_set_company_context",
            json!({"company":"a","documentId":1,"sourceReference":"x","businessUseReason":"x".repeat(2001)}),
        ),
        ("documents_list", json!({"company":"a","actor":"forged"})),
        (
            "documents_ingest",
            json!({"company":"a","filePath":"missing","metadata":{"source":"x"},"serverPath":"/tmp/a"}),
        ),
    ] {
        let rows = parsed(&run(
            "http://127.0.0.1:9",
            &[init("2025-11-25"), ready(), call(2, name, args.clone())],
        ));
        assert_eq!(rows[1]["error"]["code"], -32602, "{name}: {args}: {rows:?}");
    }
}
fn document(id: i64) -> Value {
    json!({"id":id,"documentNo":format!("B-{id}"),"source":"synthetic","originalFilename":"source.pdf","documentType":"purchase_sale","invoiceDate":"2026-10-22","amountIncVat":125.5,"currency":"DKK","status":"ingested","storedPath":null,"purchaseVatLines":[{"classification":"dk_purchase_25","netAmount":100.4,"vatAmount":25.1}],"senderVatOrCvr":"DK12345678","supplierCountryCode":"DK","supplierIdentifierKind":"dk_cvr","supplierIdentityStatus":"resolved","sourceBankTransactionId":null,"accountingRationale":null,"preparedBy":null,"preparedByProgram":null})
}
#[test]
fn documents_original_list_exact_nineteen_fields_and_pure_pagination() {
    let all = vec![document(3), document(2), document(1)];
    for (args, offset, limit, count, more) in [
        (json!({"company":"synthetic"}), 0, 500, 3, false),
        (
            json!({"company":"synthetic","offset":1,"limit":1}),
            1,
            1,
            1,
            true,
        ),
        (
            json!({"company":"synthetic","offset":3,"limit":5000}),
            3,
            5000,
            0,
            false,
        ),
    ] {
        let (envelope, wire) = invoke(
            "documents_list",
            args,
            json!({"ok":true,"documents":all,"errors":[]}),
            "200 OK",
        );
        assert!(wire.starts_with("GET /api/companies/synthetic/documents/original/list HTTP/1.1"));
        assert!(!wire.contains("?"));
        let data = &envelope["data"];
        assert_eq!(envelope["errors"], json!([]));
        assert_eq!(data["total"], 3);
        assert_eq!(data["count"], count);
        assert_eq!(data["limit"], limit);
        assert_eq!(data["offset"], offset);
        assert_eq!(data["hasMore"], more);
        if more {
            assert_eq!(data["nextOffset"], 2);
        } else {
            assert!(data.get("nextOffset").is_none());
        }
        for row in data["documents"].as_array().unwrap() {
            assert_eq!(row.as_object().unwrap().len(), 19);
            assert!(row["storedPath"].is_null());
            assert!(row.get("payload_json").is_none());
        }
    }
}
#[test]
fn documents_original_ingest_local_bytes_exact_wire_and_flat_result() {
    let mut file = Cookie::new();
    // macOS temp_dir crosses /var -> /private/var; exercise real regular bytes.
    file.0 = file.0.canonicalize().unwrap();
    std::fs::write(&file.0, [0, 255, 1, 2, 3]).unwrap();
    let metadata = json!({"source":"synthetic","amountIncVat":125.5,"vatAmount":25.1,"exemptionCode":null,"recipient":{"name":"Synthetic buyer","countryCode":"DK","identifierKind":"dk_cvr"}});
    let core = json!({"ok":true,"documentId":7,"documentNo":"B-7","sha256":"a".repeat(64),"storedPath":null,"message":"Bilag B-7 er indlæst."});
    let (envelope, wire) = invoke(
        "documents_ingest",
        json!({"company":"synthetic","filePath":file.0,"metadata":metadata,"vendorId":3,"force":true,"confirm":true}),
        core.clone(),
        "200 OK",
    );
    assert!(wire.starts_with("POST /api/companies/synthetic/documents/original/ingest HTTP/1.1"));
    let body: Value = serde_json::from_str(wire.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(
        body,
        json!({"confirm":true,"fileName":file.0.file_name().unwrap().to_str().unwrap(),"fileBase64":"AP8BAgM=","metadata":metadata,"vendorId":3,"force":true})
    );
    let mut expected = core;
    expected.as_object_mut().unwrap().remove("ok");
    assert_eq!(envelope, json!({"ok":true,"errors":[],"data":expected}));
    assert!(!wire.contains(file.0.to_str().unwrap()));
}
#[test]
fn documents_original_mutations_preserve_false_noops_refusals_rules_and_unknown_once() {
    for (name, args, path, core) in [
        (
            "documents_enrich",
            json!({"company":"synthetic","documentId":7,"metadata":{"source":"synthetic"},"confirm":true}),
            "enrich",
            json!({"ok":true,"documentId":7,"enriched":false}),
        ),
        (
            "documents_set_company_context",
            json!({"company":"synthetic","documentId":7,"sourceReference":"source","businessUseReason":"business","confirm":true}),
            "company-context",
            json!({"ok":true,"documentId":7,"applied":false,"errors":[]}),
        ),
    ] {
        let (envelope, wire) = invoke(name, args.clone(), core, "200 OK");
        assert!(wire.starts_with(&format!(
            "POST /api/companies/synthetic/documents/original/7/{path} HTTP/1.1"
        )));
        assert_eq!(envelope["ok"], true);
        assert_eq!(envelope["data"]["documentId"], 7);
        let (failure, _) = invoke(
            name,
            args.clone(),
            json!({"ok":false,"errors":["actual immutable history conflict"],"code":"HISTORY_CONFLICT","appliedRules":["source-binding"]}),
            "200 OK",
        );
        assert_eq!(
            failure,
            json!({"ok":false,"errors":["actual immutable history conflict"],"code":"HISTORY_CONFLICT","appliedRules":["source-binding"]})
        );
        let (unknown, _) = invoke(
            name,
            args,
            json!({"ok":false,"errors":["unavailable"]}),
            "503 Unavailable",
        );
        assert_eq!(unknown["code"], "outcome_unknown");
        assert!(unknown.get("data").is_none());
    }
}
fn status() -> Value {
    json!({"documentId":7,"sourceSha256":"a".repeat(64),"parserId":"pdfjs-dist","parserVersion":"6.2.108","contractVersion":"document-pdf-text-v1","status":"ok","errorCode":null,"pageCount":1,"itemCount":1,"textLength":9,"resultHash":"b".repeat(64)})
}
#[test]
fn documents_original_pdf_methods_flat_outputs_null_and_tampered_evidence() {
    for (name, args, core, path) in [
        (
            "documents_parse",
            json!({"company":"synthetic","documentId":7,"confirm":true}),
            json!({"ok":true,"parse":{"documentId":7,"status":"ok","errorCode":null,"cached":true,"pageCount":1,"itemCount":1,"textLength":9,"resultHash":"b".repeat(64)}}),
            "POST /api/companies/synthetic/documents/7/parse ",
        ),
        (
            "documents_parse_status",
            json!({"company":"synthetic","documentId":7}),
            json!({"ok":true,"parse":status()}),
            "GET /api/companies/synthetic/documents/7/parse-status ",
        ),
        (
            "documents_parse_status",
            json!({"company":"synthetic","documentId":7}),
            json!({"ok":true,"parse":null}),
            "GET /api/companies/synthetic/documents/7/parse-status ",
        ),
        (
            "documents_parsed_text",
            json!({"company":"synthetic","documentId":7}),
            json!({"ok":true,"parse":null,"pages":[],"offset":0,"limit":10,"nextOffset":null}),
            "GET /api/companies/synthetic/documents/7/parsed-text?offset=0&limit=10 ",
        ),
        (
            "documents_parse_pending",
            json!({"company":"synthetic","confirm":true}),
            json!({"ok":true,"batch":{"requested":1,"parsed":0,"failed":1,"cursor":0,"nextCursor":7,"resume":{"documentIds":[7]}}}),
            "POST /api/companies/synthetic/documents/parse-pending ",
        ),
    ] {
        let (envelope, wire) = invoke(name, args, core.clone(), "200 OK");
        assert!(wire.starts_with(path), "{wire}");
        let mut expected = core;
        expected.as_object_mut().unwrap().remove("ok");
        assert_eq!(envelope, json!({"ok":true,"errors":[],"data":expected}));
    }
    let (envelope, _) = invoke(
        "documents_parse_status",
        json!({"company":"synthetic","documentId":7}),
        json!({"ok":false,"code":"PDF_EVIDENCE_TAMPERED","errors":["PDF_EVIDENCE_TAMPERED"]}),
        "409 Conflict",
    );
    assert_eq!(
        envelope,
        json!({"ok":false,"code":"PDF_EVIDENCE_TAMPERED","errors":["PDF_EVIDENCE_TAMPERED"]})
    );
}
