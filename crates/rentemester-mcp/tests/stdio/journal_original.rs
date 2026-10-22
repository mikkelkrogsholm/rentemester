use super::*;

fn payload() -> Value {
    json!({"transactionDate":"2026-10-22","text":"Synthetic original journal","lines":[{"accountNo":"1000","debitAmount":10.5},{"accountNo":"2000","creditAmount":10.5}]})
}
fn posted() -> Value {
    json!({"ok":true,"entryId":17,"entryNo":"JE-000017","entryHash":"a".repeat(64),"errors":[],"appliedRules":["DK-BOOKKEEPING-APPEND-ONLY-001"]})
}
fn invoke(name: &str, args: Value, core: Value, status: &str) -> (Value, Value, String) {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{core}",
        core.to_string().len()
    );
    let (origin, requests, task) = http(Box::leak(response.into_boxed_str()));
    let rows = parsed(&run(
        &origin,
        &[init("2025-11-25"), ready(), call(2, name, args)],
    ));
    let wire = String::from_utf8(
        requests
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap(),
    )
    .unwrap();
    task.join().unwrap();
    let result = rows[1]["result"].clone();
    let envelope = result["structuredContent"].clone();
    assert_eq!(
        serde_json::from_str::<Value>(result["content"][0]["text"].as_str().unwrap()).unwrap(),
        envelope
    );
    assert_eq!(result["isError"], envelope["ok"] != true);
    let body = serde_json::from_str(wire.split("\r\n\r\n").nth(1).unwrap()).unwrap();
    (envelope, body, wire)
}
#[test]
fn journal_original_catalogue_annotations_and_structured_numeric_schema() {
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
    for name in ["journal_dry_run", "journal_post", "journal_reverse"] {
        let t = tools.iter().find(|t| t["name"] == name).unwrap();
        let read = name == "journal_dry_run";
        assert_eq!(
            t["annotations"],
            json!({"readOnlyHint":read,"destructiveHint":false,"idempotentHint":read,"openWorldHint":false})
        );
        assert_eq!(t["inputSchema"]["additionalProperties"], false);
        assert!(t["outputSchema"].is_object());
    }
    for name in ["post_journal", "reverse_journal", "journal_list"] {
        assert!(tools.iter().any(|t| t["name"] == name));
    }
}
#[test]
fn journal_original_confirmation_and_malformed_schema_never_http() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let mut messages = vec![init("2025-11-25"), ready()];
    for (i, args) in [
        json!({"company":"synthetic","payload":payload()}),
        json!({"company":"synthetic","payload":payload(),"confirm":false}),
    ]
    .into_iter()
    .enumerate()
    {
        messages.push(call(i as i32 + 2, "journal_post", args));
    }
    let rows = parsed(&run(&origin, &messages));
    for row in &rows[1..] {
        assert_eq!(
            row["result"]["structuredContent"],
            json!({"ok":false,"code":"CONFIRM_REQUIRED","errors":["confirm: true required for write tool journal_post"]})
        );
    }
    for args in [
        json!({"company":"synthetic","payload":payload(),"confirm":"true"}),
        json!({"company":"synthetic","payload":payload(),"confirm":null}),
        json!({"company":"synthetic","payload":payload(),"confirm":true,"actor":"forged"}),
        json!({"company":"synthetic","payload":payload(),"confirm":true,"idempotencyKey":"😀".repeat(65)}),
        json!({"company":"synthetic","payload":{"transactionDate":"2026-10-22","text":"Synthetic","lines":[{"accountNo":"1000","debitAmount":"10.5"},{"accountNo":"2000","creditAmount":10.5}]},"confirm":true}),
        json!({"company":"synthetic","payload":payload(),"confirm":true,"idempotencyKey":null}),
    ] {
        let rows = parsed(&run(
            &origin,
            &[init("2025-11-25"), ready(), call(2, "journal_post", args)],
        ));
        assert_eq!(rows[1]["error"]["code"], -32602);
    }
    listener.set_nonblocking(true).unwrap();
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}
#[test]
fn journal_original_unkeyed_and_keyed_routes_preserve_numeric_core_and_receipt() {
    for keyed in [false, true] {
        for replayed in [false, true] {
            let mut core = posted();
            let mut args = json!({"company":"synthetic","payload":payload(),"confirm":true});
            if keyed {
                args["idempotencyKey"] = json!("caller-key");
                core["idempotency"] = json!({"replayed":replayed,"receiptId":81,"createdAt":"2026-10-22T14:00:00Z","expiresAt":"2026-10-29T14:00:00Z"});
            }
            let (envelope, body, wire) = invoke("journal_post", args, core.clone(), "200 OK");
            assert!(wire.starts_with(if keyed {
                "POST /api/companies/synthetic/journal/original/keyed-post HTTP/1.1"
            } else {
                "POST /api/companies/synthetic/journal/original/post HTTP/1.1"
            }));
            assert_eq!(body["input"], payload());
            assert_eq!(body.get("idempotencyKey").is_some(), keyed);
            assert_eq!(envelope["data"]["entryId"], 17);
            assert!(envelope["data"].get("ok").is_none());
            assert_eq!(envelope["appliedRules"], core["appliedRules"]);
            assert_eq!(envelope["data"].get("idempotency"), core.get("idempotency"));
        }
    }
}
#[test]
fn journal_original_dryrun_and_reverse_full_source_envelopes() {
    let mut core = posted();
    core["previousHash"] = json!("GENESIS");
    core["accountEffects"] = json!([{"accountNo":"1000","accountName":"Synthetic asset","balanceBefore":0,"balanceAfter":10.5,"delta":10.5},{"accountNo":"2000","accountName":"Synthetic bank","balanceBefore":0,"balanceAfter":-10.5,"delta":-10.5}]);
    let (envelope, body, wire) = invoke(
        "journal_dry_run",
        json!({"company":"synthetic","payload":payload()}),
        core.clone(),
        "200 OK",
    );
    assert!(wire.starts_with("POST /api/companies/synthetic/journal/original/dry-run HTTP/1.1"));
    assert_eq!(body, json!({"input":payload()}));
    assert_eq!(envelope["data"]["accountEffects"], core["accountEffects"]);
    for keyed in [false, true] {
        let mut core = posted();
        core["originalEntryId"] = json!(16);
        let mut args = json!({"company":"synthetic","date":"2026-10-22","reason":"Synthetic correction","entryId":16,"entryNo":"  JE-000016  ","matchText":"Synthetic","matchDate":"2026-10-21","matchDocumentId":9,"confirm":true});
        if keyed {
            args["idempotencyKey"] = json!("reverse-key");
            core["idempotency"] = json!({"replayed":false,"receiptId":82,"createdAt":"2026-10-22T14:00:00Z","expiresAt":"2026-10-29T14:00:00Z"});
        }
        let (envelope, body, wire) = invoke("journal_reverse", args, core, "200 OK");
        assert!(wire.contains(if keyed {
            "/original/keyed-reverse "
        } else {
            "/original/reverse "
        }));
        assert_eq!(body["entryNo"], "  JE-000016  ");
        assert_eq!(body["matchDocumentId"], 9);
        assert!(body.get("date").is_none());
        assert_eq!(body["transactionDate"], "2026-10-22");
        assert_eq!(envelope["data"]["originalEntryId"], 16);
    }
}
#[test]
fn journal_original_selection_refusals_and_native_codes_rules_preserved() {
    for core in [
        json!({"ok":false,"code":"journal_selection_usage","errors":["Multiple journal entries matched"],"appliedRules":[]}),
        json!({"ok":false,"code":"RESTORED_KEY_RECONCILIATION_REQUIRED","errors":["Reconciliation required"],"appliedRules":["restore-guard"]}),
        json!({"ok":false,"code":"IDEMPOTENCY_CONFLICT","errors":["Different intent"],"appliedRules":[]}),
        json!({"ok":false,"errors":["date falls in closed period"],"appliedRules":["period-guard"]}),
    ] {
        let (envelope, _, _) = invoke(
            "journal_reverse",
            json!({"company":"synthetic","date":"2026-10-22","reason":"Synthetic correction","entryNo":"missing","matchText":"do not fallback","confirm":true,"idempotencyKey":"key"}),
            core.clone(),
            "422 Unprocessable Entity",
        );
        assert_eq!(envelope["errors"], core["errors"]);
        assert_eq!(envelope["appliedRules"], core["appliedRules"]);
        assert!(envelope.get("data").is_none());
        assert_eq!(
            envelope["code"],
            core.get("code").cloned().unwrap_or(json!("PERIOD_CLOSED"))
        );
    }
}
#[test]
fn journal_original_malformed_and_authority_responses_redacted_unknown_no_retry() {
    for (status, core, code) in [
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
        // Accepted sockets stay blocking. Normal process startup precedes this short
        // after-response observation; no financial/provider effects or retry.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut bytes = Vec::new();
            loop {
                let mut b = [0; 4096];
                let n = socket.read(&mut b).unwrap();
                bytes.extend_from_slice(&b[..n]);
                if let Some(i) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&bytes[..i]).to_lowercase();
                    let len = headers
                        .lines()
                        .find_map(|s| s.strip_prefix("content-length: "))
                        .unwrap()
                        .parse::<usize>()
                        .unwrap();
                    if bytes.len() >= i + 4 + len {
                        break;
                    }
                }
                assert!(n > 0);
            }
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{core}",
                core.to_string().len()
            );
            socket.write_all(response.as_bytes()).unwrap();
            drop(socket);
            listener.set_nonblocking(true).unwrap();
            let until = std::time::Instant::now() + std::time::Duration::from_millis(150);
            while std::time::Instant::now() < until {
                assert!(
                    matches!(listener.accept(),Err(e) if e.kind()==std::io::ErrorKind::WouldBlock)
                );
                thread::sleep(std::time::Duration::from_millis(5));
            }
        });
        let rows = parsed(&run(
            &origin,
            &[
                init("2025-11-25"),
                ready(),
                call(
                    2,
                    "journal_post",
                    json!({"company":"synthetic","payload":payload(),"confirm":true,"idempotencyKey":"caller-key"}),
                ),
            ],
        ));
        assert_eq!(rows[1]["result"]["structuredContent"]["code"], code);
        server.join().unwrap();
    }
}
