use rentemester_client::{ApiClient, Error, MAX_CONTACT_IMPORT_BODY};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
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

fn body() -> Vec<u8> {
    serde_json::to_vec(&json!({"fileName":"Kontakter.csv","content":"Kontaktnavn;CVR-nummer;Kontakttype\nSynthetic;;\n","enrichCvr":true,"confirm":true})).unwrap()
}
#[tokio::test]
async fn import_preserves_large_exact_csv_bytes_and_full_partial_result() {
    let reply = r#"{"ok":true,"import":{"detected":{"id":"dinero-contacts"},"summary":{"parsed":2,"customersCreated":1,"vendorsCreated":1,"skipped":0,"enriched":1,"enrichmentFailures":1},"errors":["known row warning"],"future":{"retained":true}}}"#;
    let (origin, wire, task) = server("200 OK", reply, "application/json");
    let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
    let mut value: Value = serde_json::from_slice(&body()).unwrap();
    value["content"] = json!("x".repeat(400_000));
    let bytes = serde_json::to_vec(&value).unwrap();
    assert_eq!(
        api.import_contacts("synthetic", bytes.clone(), true)
            .await
            .unwrap(),
        serde_json::from_str::<Value>(reply).unwrap()
    );
    let request = wire.recv().unwrap();
    assert!(request.starts_with(b"POST /api/companies/synthetic/import HTTP/1.1"));
    assert!(request.ends_with(&bytes));
    assert_eq!(task.join().unwrap(), 1);
}
#[tokio::test]
async fn import_known_refusal_is_canonical_and_unknown_write_is_never_retried() {
    for (status, reply, unknown) in [
        (
            "422 Unprocessable Entity",
            r#"{"ok":false,"code":"contact_import_refused","errors":["source refused"],"future":true}"#,
            false,
        ),
        ("503 Service Unavailable", "{}", true),
    ] {
        let (origin, wire, task) = server(status, reply, "application/json");
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        let result = api.import_contacts("synthetic", body(), true).await;
        if unknown {
            assert_eq!(result, Err(Error::OutcomeUnknown));
        } else {
            assert_eq!(
                result.unwrap(),
                serde_json::from_str::<Value>(reply).unwrap()
            );
        }
        wire.recv().unwrap();
        assert_eq!(task.join().unwrap(), 1);
    }
}
#[tokio::test]
async fn import_confirmation_size_and_provider_authority_fail_before_transport() {
    let api = ApiClient::new("http://127.0.0.1:9", true, Some("session=synthetic".into())).unwrap();
    assert!(matches!(
        api.import_contacts("synthetic", body(), false).await,
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        api.import_contacts("synthetic", vec![b' '; MAX_CONTACT_IMPORT_BODY + 1], true)
            .await,
        Err(Error::InvalidInput(_))
    ));
    let mut v: Value = serde_json::from_slice(&body()).unwrap();
    v["endpoint"] = json!("https://untrusted.invalid");
    assert!(matches!(
        api.import_contacts("synthetic", serde_json::to_vec(&v).unwrap(), true)
            .await,
        Err(Error::InvalidInput(_))
    ));
}
