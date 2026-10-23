mod common;
use common::TcpListener;
use rentemester_client::{ApiClient, Error, ReceivablesMutation};
use serde_json::json;
use std::{
    io::{Read, Write},
    thread,
    time::Duration,
};
fn server(response: &'static str) -> (String, thread::JoinHandle<Vec<u8>>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", l.local_addr().unwrap());
    let task = thread::spawn(move || {
        let (mut stream, _) = l.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut bytes = vec![];
        loop {
            let mut buf = [0; 4096];
            let n = stream.read(&mut buf).unwrap();
            if n == 0 {
                break;
            }
            bytes.extend_from_slice(&buf[..n]);
            if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let len = String::from_utf8_lossy(&bytes[..end])
                    .lines()
                    .find_map(|s| {
                        s.to_lowercase()
                            .strip_prefix("content-length: ")
                            .map(str::to_owned)
                    })
                    .unwrap_or("0".into())
                    .parse::<usize>()
                    .unwrap();
                if bytes.len() >= end + 4 + len {
                    break;
                }
            }
        }
        stream.write_all(response.as_bytes()).unwrap();
        drop(stream);
        l.set_nonblocking(true).unwrap();
        thread::sleep(Duration::from_millis(120));
        assert!(l.accept().is_err(), "write was retried");
        bytes
    });
    (url, task)
}
#[tokio::test]
async fn all_three_exact_authenticated_routes_preserve_native_body() {
    for(op,route,body)in[(ReceivablesMutation::Settle,"settle",br#"{ "invoiceDocumentId":1,"bankTransactionId":7,"amount":100.01,"confirm":true }"#.to_vec()),(ReceivablesMutation::Credit,"credit-note",br#"{"invoiceDocumentId":1,"issueDate":"2026-10-19","reason":"Synthetic correction","grossAmount":25.01,"confirm":true}"#.to_vec()),(ReceivablesMutation::Refund,"refund",br#"{"invoiceDocumentId":1,"bankTransactionReference":"synthetic outgoing","confirm":true}"#.to_vec())]{let(url,task)=server("HTTP/1.1 200 OK\r\nContent-Length:11\r\nConnection:close\r\n\r\n{\"ok\":true}");let api=ApiClient::new(&url,true,Some("session=private".into())).unwrap();api.receivables_mutation("synthetic",op,body.clone(),true).await.unwrap();let wire=task.join().unwrap();let end=wire.windows(4).position(|w|w==b"\r\n\r\n").unwrap();assert_eq!(&wire[end+4..],body);let h=String::from_utf8_lossy(&wire[..end]).to_lowercase();assert!(h.starts_with(&format!("post /api/companies/synthetic/invoices/{route} http/1.1")));assert!(h.contains("cookie: session=private"));assert!(h.contains(&format!("origin: {url}")));}
}
#[tokio::test]
async fn selector_confirmation_and_unknown_fields_refuse_before_transport() {
    let api = ApiClient::new("http://127.0.0.1:9", true, Some("s=x".into())).unwrap();
    for value in [
        json!({"invoiceDocumentId":1,"confirm":true}),
        json!({"invoiceDocumentId":1,"bankTransactionId":2,"bankTransactionReference":"both","confirm":true}),
        json!({"invoiceDocumentId":1,"bankTransactionId":2,"bankTransactionReference":" ","confirm":true}),
        json!({"invoiceDocumentId":1,"bankTransactionId":2,"confirm":false}),
        json!({"invoiceDocumentId":1,"bankTransactionId":2,"confirm":true,"idempotencyKey":"invented"}),
    ] {
        assert!(matches!(
            api.receivables_mutation(
                "synthetic",
                ReceivablesMutation::Settle,
                serde_json::to_vec(&value).unwrap(),
                true
            )
            .await,
            Err(Error::InvalidInput(_))
        ));
    }
    assert!(matches!(api.receivables_mutation("synthetic",ReceivablesMutation::Credit,br#"{"invoiceDocumentId":1,"issueDate":"2026-10-19","reason":"Synthetic","confirm":true}"#.to_vec(),false).await,Err(Error::InvalidInput(_))));
}
#[tokio::test]
async fn rejection_and_ambiguous_mutation_remain_typed_and_never_retry() {
    for (response, expected) in [
        (
            "HTTP/1.1 422 Unprocessable Entity\r\nContent-Length:12\r\nConnection:close\r\n\r\n{\"ok\":false}",
            Error::Refused { status: 422 },
        ),
        (
            "HTTP/1.1 503 Unavailable\r\nContent-Length:12\r\nConnection:close\r\n\r\n{\"ok\":false}",
            Error::OutcomeUnknown,
        ),
        (
            "HTTP/1.1 200 OK\r\nContent-Length:100\r\nConnection:close\r\n\r\n{",
            Error::OutcomeUnknown,
        ),
    ] {
        let (url, task) = server(response);
        let api = ApiClient::new(&url, true, Some("s=x".into())).unwrap();
        assert_eq!(api.receivables_mutation("synthetic",ReceivablesMutation::Credit,br#"{"invoiceDocumentId":1,"issueDate":"2026-10-19","reason":"Synthetic","grossAmount":25.01,"confirm":true}"#.to_vec(),true).await,Err(expected));
        task.join().unwrap();
    }
}
