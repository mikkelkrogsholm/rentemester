use super::*;
fn envelope(row: &Value) -> &Value {
    let result = &row["result"];
    let envelope = &result["structuredContent"];
    assert_eq!(
        serde_json::from_str::<Value>(result["content"][0]["text"].as_str().unwrap()).unwrap(),
        *envelope
    );
    assert_eq!(result["isError"], envelope["ok"] != true);
    envelope
}
#[test]
fn original_diagnostics118_catalogue_exact_flat_grammar_titles_and_hints() {
    let rows = parsed(&run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        ],
    ));
    let tools = rows[1]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 436);
    for (name, title) in [
        ("retention_status", "Retention status report"),
        ("system_healthcheck", "Company directory healthcheck"),
    ] {
        let t = tools.iter().find(|t| t["name"] == name).unwrap();
        assert_eq!(t["title"], title);
        assert_eq!(
            t["annotations"],
            json!({"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false})
        );
        assert_eq!(t["inputSchema"]["required"], json!(["company"]));
        assert_eq!(t["inputSchema"]["additionalProperties"], false);
        assert_eq!(
            t["inputSchema"]["properties"]["company"],
            json!({"type":"string","minLength":1,"description":"Absolute path to the company directory, or a workspace slug."})
        );
        assert_eq!(t["outputSchema"]["required"], json!(["ok", "errors"]));
        assert_eq!(
            t["inputSchema"]["properties"].as_object().unwrap().len(),
            if name == "retention_status" { 2 } else { 1 }
        );
        if name == "retention_status" {
            assert_eq!(
                t["inputSchema"]["properties"]["asOf"],
                json!({"type":"string"})
            );
        }
    }
}
#[test]
fn original_diagnostics118_flat_arguments_text_only_and_global_object_guard_without_network() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    for name in ["retention_status", "system_healthcheck"] {
        for args in [
            json!(null),
            json!([]),
            json!({}),
            json!({"company":""}),
            json!({"company":7}),
            json!({"company":"synthetic","actor":"forged"}),
            json!({"company":"synthetic","confirm":true}),
            json!({"company":"synthetic","asOf":null}),
            json!({"company":"synthetic","body":"{}"}),
        ] {
            let rows = parsed(&run(
                &origin,
                &[init("2025-11-25"), ready(), call(2, name, args.clone())],
            ));
            if !args.is_object() {
                assert_eq!(rows[1]["error"]["code"], -32602);
                assert!(rows[1].get("result").is_none());
                continue;
            }
            assert_eq!(rows[1]["result"]["isError"], true);
            assert!(rows[1]["result"].get("structuredContent").is_none());
            assert!(
                rows[1]["result"]["content"][0]["text"]
                    .as_str()
                    .unwrap()
                    .contains("Input validation error")
            );
        }
    }
    listener.set_nonblocking(true).unwrap();
    assert!(listener.accept().is_err());
}
#[test]
fn original_diagnostics118_retention_exact_success_and_invalid_date_envelopes() {
    for date in [None, Some("2026-10-25"), Some(""), Some("2026-02-30")] {
        let ok = date.is_none_or(|d| d == "2026-10-25");
        let core = json!({"ok":ok,"asOf":date.unwrap_or("2026-10-25"),"appliedRules":["DK-BOOKKEEPING-RETENTION-001"],"rows":if ok {json!([{"table":"documents","total":0,"expired":0,"nextExpiry":null,"oldestExpired":null},{"table":"journal_entries","total":0,"expired":0,"nextExpiry":null,"oldestExpired":null},{"table":"bank_transactions","total":0,"expired":0,"nextExpiry":null,"oldestExpired":null}])}else{json!([])},"errors":if ok {json!([])} else {json!(["asOf must be YYYY-MM-DD"])}});
        let body = json!({"ok":true,"retentionStatus":core}).to_string();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{body}",
            body.len()
        );
        let (origin, request, task) = http(Box::leak(response.into_boxed_str()));
        let mut args = json!({"company":"synthetic"});
        if let Some(d) = date {
            args["asOf"] = json!(d);
        }
        let rows = parsed(&run(
            &origin,
            &[
                init("2025-11-25"),
                ready(),
                call(2, "retention_status", args),
            ],
        ));
        let e = envelope(&rows[1]);
        let mut expected =
            json!({"ok":ok,"errors":core["errors"],"appliedRules":core["appliedRules"]});
        if ok {
            expected["data"] = json!({"asOf":core["asOf"],"rows":core["rows"]});
        }
        assert_eq!(*e, expected);
        let wire = String::from_utf8(request.recv().unwrap()).unwrap();
        assert!(wire.starts_with("GET /api/companies/synthetic/retention-status"));
        if let Some(d) = date {
            assert!(wire.starts_with(&format!(
                "GET /api/companies/synthetic/retention-status?asOf={d} HTTP/1.1"
            )));
        }
        task.join().unwrap();
    }
}
#[test]
fn original_diagnostics118_health_false_preserves_schema_and_checks() {
    for ok in [true, false] {
        let core = json!({"ok":ok,"checks":[{"name":"company_root","ok":true},{"name":"data_dir","ok":true},{"name":"ledger","ok":true},{"name":"documents","ok":true},{"name":"config","ok":true}],"missing":if ok {json!([])} else {json!(["schema"])},"schema_outdated":false,"schema":{"status":if ok {"current"} else {"corrupt"},"currentVersion":1,"requiredVersion":1,"pending":[]},"errors":if ok {json!([])} else {json!(["schema_corrupt: current=1 required=1"])}});
        let body = json!({"ok":true,"healthcheck":core}).to_string();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{body}",
            body.len()
        );
        let (origin, request, task) = http(Box::leak(response.into_boxed_str()));
        let rows = parsed(&run(
            &origin,
            &[
                init("2025-11-25"),
                ready(),
                call(2, "system_healthcheck", json!({"company":"synthetic"})),
            ],
        ));
        let mut data = core.clone();
        data.as_object_mut().unwrap().remove("errors");
        assert_eq!(
            *envelope(&rows[1]),
            json!({"ok":ok,"errors":core["errors"],"data":data})
        );
        assert!(
            String::from_utf8(request.recv().unwrap())
                .unwrap()
                .starts_with("GET /api/companies/synthetic/system-healthcheck HTTP/1.1")
        );
        task.join().unwrap();
    }
}
#[test]
fn original_diagnostics118_refusals_malformed_responses_and_path_selectors_fail_closed() {
    for name in ["retention_status", "system_healthcheck"] {
        for (status, body, code) in [
            (
                "403 Forbidden",
                r#"{"ok":false,"errors":["revoked"]}"#,
                "server_refused",
            ),
            ("429 Limited", "{}", "server_refused"),
            ("200 OK", "{}", "protocol"),
            ("200 OK", "{", "protocol"),
        ] {
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{body}",
                body.len()
            );
            let (origin, request, task) = http(Box::leak(response.into_boxed_str()));
            let rows = parsed(&run(
                &origin,
                &[
                    init("2025-11-25"),
                    ready(),
                    call(2, name, json!({"company":"synthetic"})),
                ],
            ));
            assert_eq!(envelope(&rows[1])["code"], code);
            request.recv().unwrap();
            task.join().unwrap();
        }
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        for company in ["../synthetic", "/unverified/synthetic", "synthetic/nested"] {
            let rows = parsed(&run(
                &origin,
                &[
                    init("2025-11-25"),
                    ready(),
                    call(2, name, json!({"company":company})),
                ],
            ));
            assert_eq!(envelope(&rows[1])["code"], "invalid_input");
        }
        listener.set_nonblocking(true).unwrap();
        assert!(listener.accept().is_err());
    }
}
