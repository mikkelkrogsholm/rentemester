use rentemester_client::{
    ApiClient, Error, PostingRulesOriginalOperation as Op, original_posting_rules_schema,
    validate_original_posting_rules,
};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::{Duration, Instant},
};
fn input(op: Op) -> Value {
    match op {
        Op::Propose => {
            json!({"proposal":{"ruleId":"synthetic","effectiveFrom":"2026-01-01","conditions":{"currency":"DKK","amount":{"min":100,"max":150},"attributes":{"bankText":"synthetic"}},"outcome":{"account":"6000","vatTreatment":"standard","textTemplate":"Synthetic"},"provenance":"review","rationale":"reviewed"},"confirm":true})
        }
        Op::Explain | Op::Test => json!({"context":{"currency":"DKK","amount":125}}),
        _ => {
            json!({"ruleId":"synthetic","version":1,"expectedPayloadHash":"a".repeat(64),"rationale":"reviewed","provenance":"review","confirm":true})
        }
    }
}
#[test]
fn all_six_schemas_and_strict_inputs() {
    for op in [
        Op::Propose,
        Op::Approve,
        Op::Disable,
        Op::Supersede,
        Op::Explain,
        Op::Test,
    ] {
        assert!(original_posting_rules_schema(op)["required"].is_array());
        assert!(validate_original_posting_rules(op, &input(op)).is_ok());
        for key in [
            "company",
            "companyId",
            "actor",
            "principal",
            "createdAt",
            "unknown",
        ] {
            let mut v = input(op);
            v[key] = json!("forged");
            assert!(validate_original_posting_rules(op, &v).is_err());
        }
    }
    for (key, value) in [
        ("createdAt", json!("2026-01-01")),
        ("effectiveFrom", json!("2026-02-30")),
    ] {
        let mut v = input(Op::Propose);
        v["proposal"][key] = value;
        assert!(validate_original_posting_rules(Op::Propose, &v).is_err());
    }
    for amount in [json!(1.001), json!(90071992547410f64)] {
        let mut v = input(Op::Test);
        v["context"]["amount"] = amount;
        assert!(validate_original_posting_rules(Op::Test, &v).is_err());
    }
}
async fn invoke(op: Op, status: u16, body: &str) -> (Result<Value, Error>, String) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let body = body.to_owned();
    let task = thread::spawn(move || {
        let start = Instant::now();
        let mut stream = loop {
            match listener.accept() {
                Ok((s, _)) => break s,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(start.elapsed() < Duration::from_secs(5));
                    thread::sleep(Duration::from_millis(5));
                }
                Err(e) => panic!("{e}"),
            }
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut bytes = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            let n = stream.read(&mut buf).unwrap();
            if n == 0 {
                break;
            }
            bytes.extend_from_slice(&buf[..n]);
            if let Some(p) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let header = String::from_utf8_lossy(&bytes[..p]).to_lowercase();
                let len = header
                    .lines()
                    .find_map(|s| {
                        s.strip_prefix("content-length:")
                            .and_then(|s| s.trim().parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                if bytes.len() >= p + 4 + len {
                    break;
                }
            }
        }
        write!(
            stream,
            "HTTP/1.1 {status} Result\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        String::from_utf8(bytes).unwrap()
    });
    let api = ApiClient::new(&origin, true, None).unwrap();
    let result = api
        .original_posting_rules("synthetic", op, &input(op), None, None)
        .await;
    (result, task.join().unwrap())
}
#[tokio::test]
async fn all_fixed_routes_flat_reads_and_status_first_single_shot() {
    for op in [
        Op::Propose,
        Op::Approve,
        Op::Disable,
        Op::Supersede,
        Op::Explain,
        Op::Test,
    ] {
        let body = if op == Op::Propose {
            json!({"ok":true,"duplicate":false,"version":1,"payloadHash":"a".repeat(64),"errors":[]})
        } else if op.write() {
            json!({"ok":true,"errors":[]})
        } else {
            json!({"decision":"human_decision","matched":[],"explanations":[],"reasons":["no approved rule matched"],"dryRun":true})
        };
        let (result, wire) = invoke(op, 200, &body.to_string()).await;
        assert_eq!(result.unwrap(), body);
        assert!(wire.starts_with(&format!(
            "POST /api/companies/synthetic/original/posting-rules/{} ",
            op.name()
        )));
        let (result, _) = invoke(op, 200, "{}").await;
        assert_eq!(
            result,
            Err(if op.write() {
                Error::OutcomeUnknown
            } else {
                Error::Protocol
            })
        );
        let (result, _) = invoke(op, 401, "not JSON").await;
        assert_eq!(result, Err(Error::Refused { status: 401 }));
        let (result, _) = invoke(
            op,
            422,
            r#"{"ok":false,"errors":["exact current refusal"]}"#,
        )
        .await;
        assert_eq!(result.unwrap()["errors"], json!(["exact current refusal"]));
    }
}

#[tokio::test]
async fn historical_preproposal_and_rejected_explanations_are_valid_reads() {
    for state in [Value::Null, json!("rejected")] {
        let item = json!({"ruleId":"synthetic","version":1,"payloadHash":"a".repeat(64),
            "state":state,"matched":false,"reasons":["not approved at cutoff"]});
        let core = json!({"decision":"human_decision","matched":[],"explanations":[item],
            "reasons":["no approved rule matched"],"dryRun":true});
        assert_eq!(
            invoke(Op::Explain, 200, &core.to_string()).await.0.unwrap(),
            core
        );
    }
}
#[test]
fn exact_server_body_text_date_and_empty_range_bounds() {
    let mut p = input(Op::Propose);
    p["proposal"]["conditions"]["amount"] = json!({});
    assert!(validate_original_posting_rules(Op::Propose, &p).is_ok());
    p["proposal"]["provenance"] = json!("x".repeat(257));
    assert!(validate_original_posting_rules(Op::Propose, &p).is_err());
    for at in ["2026-01-01T12:00:00+01:00", "2026-01-01T12:00:00.000123Z"] {
        assert!(validate_original_posting_rules(Op::Test, &json!({"context":{},"at":at})).is_err());
    }
    let mut v = input(Op::Test);
    let mut attributes = json!({});
    for i in 0..12 {
        attributes[format!("field{i}")] = json!("x".repeat(256));
    }
    v["context"]["attributes"] = attributes;
    assert!(validate_original_posting_rules(Op::Test, &v).is_ok());
}
