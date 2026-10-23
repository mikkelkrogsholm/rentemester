mod common;
use common::TcpListener;
use rentemester_client::{ApiClient, Error};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    sync::mpsc,
    thread,
    time::Duration,
};
fn server(
    status: &'static str,
    reply: &'static str,
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

fn preview() -> Vec<u8> {
    br#"{ "mileageEntryId":1, "rateVersion":"synthetic-v1", "documentId":2 }"#.to_vec()
}
fn post() -> Vec<u8> {
    serde_json::to_vec(&json!({"mileageEntryId":1,"rateVersion":"synthetic-v1","documentId":2,"expectedPreviewHash":"a".repeat(64),"operationKey":"original-key","confirm":true})).unwrap()
}
fn configure() -> Vec<u8> {
    serde_json::to_vec(&json!({"version":"synthetic-v1","validFrom":"2026-01-01","validTo":"2026-12-31","vehicle":"car","ratePerKm":"2.00","basis":"synthetic","source":"explicit synthetic evidence","expenseAccountNo":"E","liabilityAccountNo":"L","confirm":true})).unwrap()
}
#[tokio::test]
async fn mileage_posting_exact_paths_bytes_and_receipts_once() {
    for op in ["versions", "configure", "preview", "post", "postings"] {
        let reply = r#"{"ok":true,"mileagePosting":{"exact":"server","replayed":true}}"#;
        let (origin, wire, task) = server("200 OK", reply, "application/json");
        let api = ApiClient::new(&origin, true, None).unwrap();
        let bytes = match op {
            "configure" => configure(),
            "post" => post(),
            _ => preview(),
        };
        let result = match op {
            "versions" => api.mileage_rate_versions("synthetic").await,
            "configure" => {
                api.configure_mileage_rates("synthetic", bytes.clone(), true)
                    .await
            }
            "preview" => api.preview_mileage_post("synthetic", bytes.clone()).await,
            "post" => api.post_mileage("synthetic", bytes.clone(), true).await,
            _ => api.mileage_postings("synthetic").await,
        };
        assert_eq!(
            result.unwrap(),
            serde_json::from_str::<Value>(reply).unwrap()
        );
        let request = wire.recv().unwrap();
        let suffix = match op {
            "versions" | "configure" => "rate-versions",
            "preview" => "post-preview",
            "post" => "post",
            _ => "postings",
        };
        let method = if matches!(op, "versions" | "postings") {
            "GET"
        } else {
            "POST"
        };
        assert!(request.starts_with(
            format!("{method} /api/companies/synthetic/mileage/{suffix} HTTP/1.1").as_bytes()
        ));
        if method == "POST" {
            assert!(request.ends_with(&bytes));
        }
        assert_eq!(task.join().unwrap(), 1);
    }
}
#[tokio::test]
async fn mileage_posting_unknown_no_retry_and_exact_server_refusal() {
    for op in ["configure", "preview", "post"] {
        let (origin, wire, task) = server("503 Service Unavailable", "{}", "application/json");
        let api = ApiClient::new(&origin, true, None).unwrap();
        let e = match op {
            "configure" => api.configure_mileage_rates("s", configure(), true).await,
            "preview" => api.preview_mileage_post("s", preview()).await,
            _ => api.post_mileage("s", post(), true).await,
        }
        .unwrap_err();
        assert_eq!(
            e,
            if op == "preview" {
                Error::Transport
            } else {
                Error::OutcomeUnknown
            }
        );
        wire.recv().unwrap();
        assert_eq!(task.join().unwrap(), 1);
    }
    let reply = r#"{"ok":false,"code":"mileage_posting_refused","errors":["exact current source refusal"]}"#;
    let (origin, wire, task) = server("422 Unprocessable Entity", reply, "application/json");
    let api = ApiClient::new(&origin, true, None).unwrap();
    assert_eq!(
        api.post_mileage("s", post(), true).await.unwrap(),
        serde_json::from_str::<Value>(reply).unwrap()
    );
    wire.recv().unwrap();
    assert_eq!(task.join().unwrap(), 1);
}
#[tokio::test]
async fn mileage_posting_local_schema_and_confirmation_no_transport() {
    use rentemester_client::{
        MileagePostingOperation as Op, validate_mileage_posting_body as check,
    };
    let api = ApiClient::new("http://127.0.0.1:1", true, None).unwrap();
    assert!(matches!(
        api.post_mileage("s", post(), false).await,
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        api.configure_mileage_rates("s", configure(), false).await,
        Err(Error::InvalidInput(_))
    ));
    for bad in [
        br#"{"mileageEntryId":1,"rateVersion":"v","documentId":2,"actor":"forged"}"#.to_vec(),
        br#"{"mileageEntryId":1,"mileageEntryId":1,"rateVersion":"v","documentId":2}"#.to_vec(),
        br#"{"mileageEntryId":9007199254740992,"rateVersion":"v","documentId":2}"#.to_vec(),
        vec![b' '; 32769],
    ] {
        assert!(matches!(
            api.preview_mileage_post("s", bad).await,
            Err(Error::InvalidInput(_))
        ));
    }
    for (k, v) in [
        ("validFrom", json!("2026-02-30")),
        ("validTo", json!("2025-01-01")),
        ("ratePerKm", json!(2)),
        ("ratePerKm", json!("0.00")),
        ("ratePerKm", json!("-1")),
        ("source", json!(" ")),
        ("confirm", json!(false)),
    ] {
        let mut b: Value = serde_json::from_slice(&configure()).unwrap();
        b[k] = v;
        assert!(
            check(&serde_json::to_vec(&b).unwrap(), Op::Configure).is_err(),
            "{k}"
        );
    }
    let mut b: Value = serde_json::from_slice(&post()).unwrap();
    b["expectedPreviewHash"] = json!("A".repeat(64));
    assert!(check(&serde_json::to_vec(&b).unwrap(), Op::Post).is_err());
}
