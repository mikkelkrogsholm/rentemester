mod common;
use common::TcpListener;
use rentemester_client::{ApiClient, BankAccountOriginalMutation, Error};
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
        assert!(header.starts_with("POST /api/companies/synthetic/bank/accounts-original/"));
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
async fn canonical_create_update_and_business_receipts_preserve_exact_server_data() {
    for (operation, input, value) in [
        (
            BankAccountOriginalMutation::Create,
            json!({"confirm":true,"name":" Syntétic ","slug":"","currency":""}),
            json!({"ok":true,"data":{"account":{"id":7,"slug":"server-owned","name":"Syntétic","active":true,"iban":null,"createdAt":"server-clock"}},"errors":[]}),
        ),
        (
            BankAccountOriginalMutation::Update,
            json!({"confirm":true,"account":" 7 ","iban":"","active":false}),
            json!({"ok":true,"data":{"account":{"id":7,"slug":"server-owned","active":false,"iban":null,"createdAt":"server-clock"}},"errors":[]}),
        ),
        (
            BankAccountOriginalMutation::Update,
            json!({"confirm":true,"account":"unknown"}),
            json!({"ok":false,"errors":["server-owned unknown account"],"code":"not_found"}),
        ),
    ] {
        let (origin, t) = server(200, value.clone());
        assert_eq!(
            api(&origin)
                .bank_account_original("synthetic", operation, input.to_string().into_bytes())
                .await
                .unwrap(),
            value
        );
        assert_eq!(t.join().unwrap(), input);
    }
}
#[tokio::test]
async fn strict_input_and_uncertain_mutations_are_not_reclassified_as_business() {
    for input in [
        json!({"confirm":false,"account":"7"}),
        json!({"confirm":true,"account":"7","slug":"changed"}),
        json!({"confirm":true,"account":"7","iban":null}),
        json!({"confirm":true,"account":"7","active":"false"}),
        json!({"confirm":true,"account":"7","actor":"spoof"}),
    ] {
        assert!(matches!(
            api("http://127.0.0.1:9")
                .bank_account_original(
                    "synthetic",
                    BankAccountOriginalMutation::Update,
                    input.to_string().into_bytes()
                )
                .await,
            Err(Error::InvalidInput(_))
        ));
    }
    let input = json!({"confirm":true,"account":"7"});
    for (status, value) in [
        (403, json!({"ok":false})),
        (503, json!({"ok":false})),
        (200, json!({"ok":false,"errors":["bad"],"data":{}})),
    ] {
        let (origin, t) = server(status, value);
        let error = api(&origin)
            .bank_account_original(
                "synthetic",
                BankAccountOriginalMutation::Update,
                input.to_string().into_bytes(),
            )
            .await
            .unwrap_err();
        assert!(if status == 403 {
            matches!(error, Error::Refused { status: 403 })
        } else {
            matches!(error, Error::OutcomeUnknown)
        });
        t.join().unwrap();
    }
    let too_large = json!({"confirm":true,"name":"x".repeat(262144)});
    assert!(matches!(
        api("http://127.0.0.1:9")
            .bank_account_original(
                "synthetic",
                BankAccountOriginalMutation::Create,
                too_large.to_string().into_bytes()
            )
            .await,
        Err(Error::InvalidInput(_))
    ));
}
