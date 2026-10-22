use rentemester_client::ApiClient;
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

#[tokio::test]
async fn original_bad_debt_is_exact_and_optional_import_date_never_defaults_locally() {
    let reply =
        r#"{"ok":true,"writeOffId":7,"invoiceNumber":"SYN-7","grossAmount":25,"errors":[]}"#;
    let (origin, wire, task) = server("200 OK", reply, "application/json");
    let api = ApiClient::new(&origin, true, None).unwrap();
    let bytes=br#"{"payload":{"invoiceNumber":"SYN-7","writeOffDate":"2026-10-20","expenseAccountNo":"loss","receivableAccountNo":"debtor","vatAccountNo":"output-vat"},"confirm":true}"#.to_vec();
    assert_eq!(
        api.invoice_bad_debt_original("synthetic", bytes.clone())
            .await
            .unwrap(),
        serde_json::from_str::<Value>(reply).unwrap()
    );
    let request = wire.recv().unwrap();
    let end = request.windows(4).position(|b| b == b"\r\n\r\n").unwrap() + 4;
    assert!(request.starts_with(
        b"POST /api/companies/synthetic/invoices/original/write-off-bad-debt HTTP/1.1"
    ));
    assert_eq!(request[end..], bytes);
    assert_eq!(task.join().unwrap(), 1);
    for date in [None, Some("2026-10-20")] {
        let (origin, wire, task) = server(
            "200 OK",
            r#"{"ok":true,"importedReceivables":{"count":0}}"#,
            "application/json",
        );
        let api = ApiClient::new(&origin, true, None).unwrap();
        api.imported_receivables_optional("synthetic", date)
            .await
            .unwrap();
        let request = String::from_utf8(wire.recv().unwrap()).unwrap();
        let suffix = if date.is_some() {
            "?asOf=2026-10-20"
        } else {
            ""
        };
        assert!(request.starts_with(&format!(
            "GET /api/companies/synthetic/invoices/imported-receivables{suffix} HTTP/1.1"
        )));
        assert_eq!(task.join().unwrap(), 1);
    }
}
#[test]
fn original_selector_confirm_and_strict_payload_are_required() {
    use rentemester_client::validate_invoice_bad_debt_original as validate;
    let base = json!({"payload":{"invoiceNumber":"SYN-7","writeOffDate":"2026-10-20","expenseAccountNo":"loss"},"confirm":true});
    assert!(validate(&serde_json::to_vec(&base).unwrap()).is_ok());
    for (key, value) in [
        ("invoiceDocumentId", json!(0)),
        ("actor", json!("forged")),
        ("expenseAccountNo", json!(" ")),
        ("writeOffDate", json!("2026-02-30")),
    ] {
        let mut input = base.clone();
        input["payload"][key] = value;
        assert!(validate(&serde_json::to_vec(&input).unwrap()).is_err());
    }
    let mut both = base.clone();
    both["payload"]["invoiceDocumentId"] = json!(7);
    assert!(validate(&serde_json::to_vec(&both).unwrap()).is_ok());
    let mut input = base;
    input["confirm"] = json!(false);
    assert!(validate(&serde_json::to_vec(&input).unwrap()).is_err());
}
