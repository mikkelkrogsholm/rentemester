mod common;
use common::TcpListener;
use rentemester_client::{ApiClient, Error, bank_import_file_body};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    thread,
};
fn server(status: u16, value: Value) -> (String, thread::JoinHandle<Value>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", l.local_addr().unwrap());
    let t = thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut bytes = vec![];
        let end = loop {
            let mut b = [0; 1024];
            let n = s.read(&mut b).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&b[..n]);
            if let Some(i) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                break i + 4;
            }
        };
        let header = String::from_utf8_lossy(&bytes[..end]);
        assert!(header.starts_with("POST /api/companies/synthetic/bank/import-original HTTP/1.1"));
        let length: usize = header
            .lines()
            .find_map(|l| {
                l.to_lowercase()
                    .strip_prefix("content-length:")
                    .map(|s| s.trim().parse().unwrap())
            })
            .unwrap();
        while bytes.len() < end + length {
            let mut b = [0; 1024];
            let n = s.read(&mut b).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&b[..n]);
        }
        let body = value.to_string();
        write!(
            s,
            "HTTP/1.1 {status} Mock\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        serde_json::from_slice(&bytes[end..end + length]).unwrap()
    });
    (origin, t)
}
fn api(origin: &str) -> ApiClient {
    ApiClient::new(origin, true, None).unwrap()
}
#[tokio::test]
async fn exact_receipt_business_errors_and_typed_uncertainty_are_preserved() {
    let body = serde_json::to_vec(
        &json!({"confirm":true,"csvContent":"date;text;amount\n2026-01-01;Synthetic;1"}),
    )
    .unwrap();
    for value in [
        json!({"ok":true,"data":{"importBatchId":"BANK-source","sourceFileHash":"exact","imported":2,"skippedDuplicates":1,"exceptionsCreated":0,"firstBalance":-12.34},"errors":[]}),
        json!({"ok":false,"code":"bank_refused","errors":["server-owned refusal"]}),
    ] {
        let (origin, t) = server(200, value.clone());
        assert_eq!(
            api(&origin)
                .bank_import_original("synthetic", body.clone())
                .await
                .unwrap(),
            value
        );
        assert_eq!(t.join().unwrap()["confirm"], true);
    }
    for (status, unknown) in [(403, false), (503, true)] {
        let (origin, t) = server(status, json!({"ok":false}));
        let error = api(&origin)
            .bank_import_original("synthetic", body.clone())
            .await
            .unwrap_err();
        assert!(if unknown {
            matches!(error, Error::OutcomeUnknown)
        } else {
            matches!(error, Error::Refused { status: 403 })
        });
        t.join().unwrap();
    }
    let (origin, t) = server(200, json!({"ok":false,"data":{},"errors":["bad"]}));
    assert!(matches!(
        api(&origin).bank_import_original("synthetic", body).await,
        Err(Error::OutcomeUnknown)
    ));
    t.join().unwrap();
}
#[tokio::test]
async fn file_bytes_and_native_union_confirmation_are_not_changed() {
    let path = std::env::temp_dir().join(format!("rm142-sdk-{}", std::process::id()));
    std::fs::write(&path, [0xef, 0xbb, 0xbf, 0xf8, 13, 10]).unwrap();
    let body = bank_import_file_body(&path, Some(" synthetic "), Some("danske-bank")).unwrap();
    std::fs::remove_file(&path).unwrap();
    let value: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value["csvBase64"], "77u/+A0K");
    assert_eq!(value["account"], " synthetic ");
    assert_eq!(value["profile"], "danske-bank");
    for body in [
        json!({"confirm":false,"csvContent":"x"}),
        json!({"confirm":true,"csvContent":"x","csvBase64":"eA=="}),
        json!({"confirm":true,"csvBase64":"YR=="}),
    ] {
        assert!(matches!(
            api("http://127.0.0.1:9")
                .bank_import_original("synthetic", body.to_string().into_bytes())
                .await,
            Err(Error::InvalidInput(_))
        ));
    }
}
