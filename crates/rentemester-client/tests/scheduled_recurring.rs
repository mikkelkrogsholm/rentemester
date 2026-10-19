use rentemester_client::{
    ApiClient, Error, MAX_SCHEDULED_RECURRING_BODY, ScheduledRecurringMutation as Op,
};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::mpsc,
    thread,
    time::Duration,
};
fn server(status: &'static str) -> (String, mpsc::Receiver<Vec<u8>>, thread::JoinHandle<usize>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", l.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();
    let t = thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        let mut bytes = vec![];
        loop {
            let mut b = [0; 4096];
            let n = s.read(&mut b).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&b[..n]);
            if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let h = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                let len = h
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length: "))
                    .map(|n| n.parse::<usize>().unwrap())
                    .unwrap_or(0);
                if bytes.len() >= end + 4 + len {
                    break;
                }
            }
        }
        tx.send(bytes).unwrap();
        let reply = r#"{"ok":true,"scheduledRecurring":{"templateId":7,"state":"enabled","stateHash":"canonical","localService":false,"queued":[{"periodIndex":0,"issueDate":"2026-01-01"}],"futureEvidence":{"retained":true},"restoreConsent":{"required":true,"boundaryHash":"original-boundary","historicalOwnerUserId":"historical-user","historicalQueued":[{"periodIndex":2,"issueDate":"2025-01-01"}]}},"generation":{"amount":"90071992547409.93","posted":false,"sent":false},"plan":{"truncated":true,"missedCount":101,"restoreConsent":null}}"#;
        write!(
            s,
            "HTTP/1.1 {status}\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{reply}",
            reply.len()
        )
        .unwrap();
        drop(s);
        l.set_nonblocking(true).unwrap();
        thread::sleep(Duration::from_millis(60));
        1 + l.incoming().take_while(Result::is_ok).count()
    });
    (origin, rx, t)
}
fn body(op: Op) -> Vec<u8> {
    let mut b = json!({"templateId":7,"confirm":true});
    let key = match op {
        Op::Enable => "templateHash",
        Op::Pause | Op::Resume | Op::Cancel => "expectedStateHash",
        _ => "planHash",
    };
    b[key] = json!("a".repeat(64));
    if matches!(
        op,
        Op::Pause | Op::Resume | Op::Cancel | Op::Skip | Op::Reconsent
    ) {
        b["reason"] = json!("  Synthetic reviewed reason café  ");
    }
    if matches!(op, Op::CatchUp | Op::Skip) {
        b["periodIndexes"] = json!([0, 2]);
    }
    if op == Op::Dispatch {
        b["periodIndex"] = json!(0);
    }
    format!(" {b}\n").into_bytes()
}
#[tokio::test]
async fn scheduled_mutations_preserve_all_exact_bytes_and_uncertain_outcomes_without_retry() {
    for op in [
        Op::Enable,
        Op::Pause,
        Op::Resume,
        Op::Cancel,
        Op::CatchUp,
        Op::Skip,
        Op::Dispatch,
        Op::Reconsent,
    ] {
        for status in ["200 OK", "422 Unprocessable Entity", "503 Unavailable"] {
            let (origin, rx, t) = server(status);
            let api =
                ApiClient::new(&origin, true, Some("session=synthetic-private".into())).unwrap();
            let bytes = body(op);
            let out = api
                .mutate_scheduled_recurring("synthetic", op, bytes.clone(), true)
                .await;
            match status {
                "200 OK" => {
                    let v = out.unwrap();
                    assert_eq!(v["scheduledRecurring"]["futureEvidence"]["retained"], true);
                    assert_eq!(v["generation"]["amount"], "90071992547409.93");
                    assert_eq!(
                        v["scheduledRecurring"]["restoreConsent"]["historicalOwnerUserId"],
                        "historical-user"
                    );
                    assert!(v["plan"]["restoreConsent"].is_null());
                }
                "422 Unprocessable Entity" => assert_eq!(out, Err(Error::Refused { status: 422 })),
                _ => assert_eq!(out, Err(Error::OutcomeUnknown)),
            }
            let wire = rx.recv().unwrap();
            assert!(
                wire.starts_with(
                    format!(
                        "POST /api/companies/synthetic/scheduled-recurring/{} HTTP/1.1",
                        op.suffix()
                    )
                    .as_bytes()
                )
            );
            assert!(wire.ends_with(&bytes));
            let text = String::from_utf8(wire).unwrap().to_lowercase();
            assert!(text.contains(&format!("origin: {origin}")));
            assert!(text.contains("cookie: session=synthetic-private"));
            assert_eq!(t.join().unwrap(), 1);
        }
    }
}
#[tokio::test]
async fn status_and_plan_gets_have_no_body_query_confirmation_or_dispatch() {
    for plan in [false, true] {
        for status in ["200 OK", "503 Unavailable"] {
            let (origin, rx, t) = server(status);
            let api =
                ApiClient::new(&origin, true, Some("session=synthetic-private".into())).unwrap();
            let out = if plan {
                api.scheduled_recurring_plan("synthetic", 7).await
            } else {
                api.scheduled_recurring("synthetic").await
            };
            if status == "200 OK" {
                assert_eq!(out.unwrap()["plan"]["truncated"], true);
            } else {
                assert_eq!(out, Err(Error::Refused { status: 503 }));
            }
            let wire = String::from_utf8(rx.recv().unwrap()).unwrap();
            let path = if plan {
                "/api/companies/synthetic/scheduled-recurring/7/plan"
            } else {
                "/api/companies/synthetic/scheduled-recurring"
            };
            assert!(wire.starts_with(&format!("GET {path} HTTP/1.1")));
            assert!(!wire.contains("confirm"));
            assert_eq!(t.join().unwrap(), 1);
        }
    }
}
#[tokio::test]
async fn unknown_actor_keys_missing_confirmation_duplicates_overflow_and_body_bounds_refuse_before_http()
 {
    let api = ApiClient::new("http://127.0.0.1:9", true, Some("session=synthetic".into())).unwrap();
    for (op, change, confirmed) in [
        (Op::Enable, json!({"actor":"user:forged"}), true),
        (Op::Reconsent, json!({"reason":" \u{feff} \t"}), true),
        (Op::Reconsent, json!({"reason":"🧾".repeat(501)}), true),
        (Op::Reconsent, json!({"periodIndex":0}), true),
        (Op::Reconsent, json!({"confirm":false}), true),
        (Op::Reconsent, json!({"planHash":"A".repeat(64)}), true),
        (Op::Reconsent, json!({"templateId":0}), true),
        (Op::Dispatch, json!({"idempotencyKey":"invented"}), true),
        (Op::Dispatch, json!({"confirm":false}), true),
        (Op::Dispatch, json!({}), false),
        (Op::CatchUp, json!({"periodIndexes":[0,0]}), true),
        (Op::CatchUp, json!({"periodIndexes":[]}), true),
        (Op::CatchUp, json!({"periodIndexes":vec![0;101]}), true),
        (Op::Dispatch, json!({"periodIndex":-1}), true),
        (Op::Dispatch, json!({"periodIndex":0.0}), true),
        (Op::Enable, json!({"templateId":9007199254740992u64}), true),
        (Op::Enable, json!({"templateHash":"g".repeat(64)}), true),
    ] {
        let mut value: Value = serde_json::from_slice(&body(op)).unwrap();
        for (k, v) in change.as_object().unwrap() {
            value[k] = v.clone();
        }
        assert!(matches!(
            api.mutate_scheduled_recurring(
                "synthetic",
                op,
                serde_json::to_vec(&value).unwrap(),
                confirmed
            )
            .await,
            Err(Error::InvalidInput(_))
        ));
    }
    assert!(matches!(
        api.mutate_scheduled_recurring(
            "synthetic",
            Op::Enable,
            vec![b' '; MAX_SCHEDULED_RECURRING_BODY + 1],
            true
        )
        .await,
        Err(Error::InvalidInput(_))
    ));
    for id in [0, -1, 9007199254740992] {
        assert!(matches!(
            api.scheduled_recurring_plan("synthetic", id).await,
            Err(Error::InvalidInput(_))
        ));
    }
}

#[tokio::test]
async fn reconsent_accepts_exact_native_utf16_reason_boundary_without_trimming_wire_bytes() {
    for reason in [
        format!(" \u{feff}{} ", "🧾".repeat(500)),
        "\u{0085}".to_owned(),
    ] {
        let (origin, rx, t) = server("200 OK");
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        let bytes = serde_json::to_vec(
            &json!({"templateId":7,"planHash":"a".repeat(64),"reason":reason,"confirm":true}),
        )
        .unwrap();
        api.mutate_scheduled_recurring("synthetic", Op::Reconsent, bytes.clone(), true)
            .await
            .unwrap();
        assert!(rx.recv().unwrap().ends_with(&bytes));
        assert_eq!(t.join().unwrap(), 1);
    }
}
