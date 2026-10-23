mod common;
use common::TcpListener;
use rentemester_client::{
    ApiClient, Error, OriginalBatchOperation as Op, bookkeeping_batch_original_schema,
    validate_bookkeeping_batch_original,
};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    sync::mpsc,
    thread,
    time::Duration,
};
fn server(
    status: &'static str,
    reply: String,
    mime: &'static str,
) -> (String, mpsc::Receiver<Vec<u8>>, thread::JoinHandle<usize>) {
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
        write!(
            s,
            "HTTP/1.1 {status}\r\nContent-Type:{mime}\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{reply}",
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

fn scope() -> Value {
    json!({"accountingFrom":"2026-01-01","accountingTo":"2026-12-31","bankFrom":"2026-01-01","bankTo":"2026-12-31"})
}
fn input(op: Op) -> Value {
    match op {
        Op::Plan => scope(),
        Op::Persist | Op::DryRun => {
            let mut v = scope();
            v["runKey"] = json!(" exact key=one ");
            v["expectedPlanHash"] = json!("a".repeat(64));
            v["confirm"] = json!(true);
            v
        }
        Op::Status => json!({"runId":7}),
        _ => json!({"runId":7,"planHash":"a".repeat(64),"confirm":true}),
    }
}
fn plan() -> Value {
    json!({"executionPolicy":"atomic_v1","planHash":"a".repeat(64),"candidateSetHash":"b".repeat(64),"items":[{"actionKey":"bank:9","partition":"ready"}]})
}
fn state() -> Value {
    json!({"run":{"runId":7,"runKey":" exact key=one ","planHash":"a".repeat(64),"plan":plan().to_string()},"revisions":[{"plannerActor":"agent:historical","approverActor":"user:reviewer"}],"attempts":[{"attemptId":12,"actor":"user:owner"}],"receipts":[{"actionKey":"bank:9","receipt":json!({"journalEntryId":31,"journalEntryHash":"c".repeat(64)}).to_string()}],"finalChecks":[{"name":"trialBalance","ok":1}],"legacyEvents":[{"actor":"system:old"}],"legacyFinalChecks":[]})
}
fn reply(op: Op) -> Value {
    match op {
        Op::Plan => json!({"ok":true,"dryRun":true,"plan":plan()}),
        Op::Persist | Op::DryRun => {
            json!({"ok":true,"dryRun":true,"runId":7,"duplicate":true,"plan":plan(),"state":state()})
        }
        Op::Apply => {
            json!({"ok":true,"runId":7,"planHash":"a".repeat(64),"results":[{"actionKey":"bank:9","receipt":{"journalEntryId":31,"outcome":"applied"}}],"checks":[{"ok":true,"name":"trialBalance"}],"state":state(),"applyAttemptId":12,"duplicate":true})
        }
        _ => json!({"ok":true,"state":state()}),
    }
}
#[tokio::test]
async fn six_operations_exact_native_routes_and_authoritative_terminal_history() {
    for op in [
        Op::Plan,
        Op::Persist,
        Op::DryRun,
        Op::Approve,
        Op::Apply,
        Op::Status,
    ] {
        let native = reply(op);
        let (origin, rx, t) = server("200 OK", native.to_string(), "application/json");
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        let got = api
            .bookkeeping_batch_original("synthetic", op, input(op))
            .await
            .unwrap();
        let mut expected = native;
        if op == Op::Persist {
            expected.as_object_mut().unwrap().remove("dryRun");
        }
        assert_eq!(got, expected);
        let wire = rx.recv_timeout(Duration::from_secs(3)).unwrap();
        let text = String::from_utf8_lossy(&wire);
        let suffix = match op {
            Op::Plan => "bookkeeping-batch?",
            Op::Persist | Op::DryRun => "bookkeeping-batch/persist ",
            Op::Approve => "bookkeeping-batch/approve ",
            Op::Apply => "bookkeeping-batch/apply ",
            Op::Status => "bookkeeping-batch/runs/7 ",
        };
        assert!(text.lines().next().unwrap().contains(suffix));
        assert!(text.to_lowercase().contains("cookie: session=synthetic"));
        let end = wire.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
        if op.write() {
            assert_eq!(
                serde_json::from_slice::<Value>(&wire[end..]).unwrap(),
                input(op)
            );
        } else {
            assert!(wire[end..].is_empty());
        }
        assert_eq!(t.join().unwrap(), 1);
    }
}
#[tokio::test]
async fn invalid_source_inputs_fail_offline() {
    let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    for op in [
        Op::Plan,
        Op::Persist,
        Op::DryRun,
        Op::Approve,
        Op::Apply,
        Op::Status,
    ] {
        assert_eq!(
            bookkeeping_batch_original_schema(op)["additionalProperties"],
            false
        );
        let mut bad = input(op);
        bad["actor"] = json!("user:forged");
        assert!(matches!(
            api.bookkeeping_batch_original("synthetic", op, bad).await,
            Err(Error::InvalidInput(_))
        ));
    }
    for (key, value) in [
        ("accountingFrom", json!("2026-02-30")),
        ("bankTo", json!("2025-01-01")),
        ("runKey", json!("x".repeat(1025))),
        ("expectedPlanHash", json!("A".repeat(64))),
        ("confirm", json!(false)),
        ("runKey", json!("\n")),
    ] {
        let mut v = input(Op::Persist);
        v[key] = value;
        assert!(validate_bookkeeping_batch_original(Op::Persist, &v).is_err());
    }
    for id in [
        json!(0),
        json!(-1),
        json!(9007199254740992u64),
        json!(1.5),
        json!("7"),
    ] {
        assert!(validate_bookkeeping_batch_original(Op::Status, &json!({"runId":id})).is_err());
    }
}
#[tokio::test]
async fn refusal_status_first_unknown_and_malformed_success_never_retry() {
    for (status, v, unknown) in [
        (
            "409 Conflict",
            json!({"ok":false,"code":"bookkeeping_batch_stale","checks":[{"ok":false}]}),
            false,
        ),
        (
            "422 Unprocessable Entity",
            json!({"ok":false,"code":"approval_required"}),
            false,
        ),
        (
            "503 Service Unavailable",
            json!({"ok":false,"code":"batch_stale"}),
            true,
        ),
        ("200 OK", json!({"ok":true,"state":{}}), true),
        (
            "200 OK",
            {
                let mut v = reply(Op::Apply);
                v["state"]["run"]["plan"] = json!("{bad");
                v
            },
            true,
        ),
    ] {
        let (origin, rx, t) = server(status, v.to_string(), "application/json");
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        let got = api
            .bookkeeping_batch_original("synthetic", Op::Apply, input(Op::Apply))
            .await;
        if unknown {
            assert!(matches!(got, Err(Error::OutcomeUnknown)));
        } else {
            assert_eq!(got.unwrap(), v);
        }
        rx.recv_timeout(Duration::from_secs(3)).unwrap();
        assert_eq!(t.join().unwrap(), 1);
    }
}
