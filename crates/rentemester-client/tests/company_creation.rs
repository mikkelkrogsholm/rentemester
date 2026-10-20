use rentemester_client::{ApiClient, Error, MAX_COMPANY_CREATION_BODY};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::Duration,
};
#[tokio::test]
async fn new_company_is_workspace_scoped_exact_and_never_retried() {
    let body = "{ \"name\":\" Synthetic Company \",\"slug\":\"chosen-company\",\"fiscalYearStartMonth\":\"7\",\"fiscalYearLabelStrategy\":\"span\",\"vatPeriodType\":null,\"payment\":null }\n";
    for (status, expected) in [
        ("201 Created", None),
        ("200 OK", Some(Error::OutcomeUnknown)),
        ("409 Conflict", Some(Error::Refused { status: 409 })),
        ("503 Unavailable", Some(Error::OutcomeUnknown)),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let task = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut wire = vec![];
            loop {
                let mut buffer = [0; 4096];
                let n = stream.read(&mut buffer).unwrap();
                assert!(n > 0);
                wire.extend_from_slice(&buffer[..n]);
                if wire.ends_with(body.as_bytes()) {
                    break;
                }
            }
            let wire = String::from_utf8(wire).unwrap();
            assert!(wire.starts_with("POST /api/companies HTTP/1.1\r\n"));
            assert!(
                wire.to_ascii_lowercase()
                    .contains("cookie: session=synthetic")
            );
            assert!(wire.ends_with(body));
            let reply = r#"{"ok":true,"company":{"slug":"chosen-company","name":"Synthetic Company","futureCanonicalField":null}}"#;
            write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{reply}",
                reply.len()
            )
            .unwrap();
            drop(stream);
            listener.set_nonblocking(true).unwrap();
            thread::sleep(Duration::from_millis(50));
            assert!(listener.accept().is_err(), "no automatic creation replay");
        });
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        let result = api.create_company(body.as_bytes().to_vec(), true).await;
        if let Some(error) = expected {
            assert_eq!(result.unwrap_err(), error);
        } else {
            let value = result.unwrap();
            assert_eq!(value["company"]["slug"], "chosen-company");
            assert!(
                value["company"]
                    .get("futureCanonicalField")
                    .unwrap()
                    .is_null()
            );
        }
        task.join().unwrap();
    }
}
#[tokio::test]
async fn creation_rejects_inner_confirmation_keys_and_company_selectors() {
    let api = ApiClient::new("http://127.0.0.1:9", true, Some("session=synthetic".into())).unwrap();
    for bytes in [
        br#"{"name":"Synthetic","confirm":true}"#.to_vec(),
        br#"{"name":"Synthetic","company":"old"}"#.to_vec(),
        br#"{"name":"Synthetic","idempotencyKey":"invented"}"#.to_vec(),
        br#"{"name":"Synthetic","fiscalYearStartMonth":7}"#.to_vec(),
        br#"{"name":"Synthetic","payment":{"secret":"excluded"}}"#.to_vec(),
        vec![b' '; MAX_COMPANY_CREATION_BODY + 1],
    ] {
        assert!(matches!(
            api.create_company(bytes, true).await,
            Err(Error::InvalidInput(_))
        ));
    }
    assert!(matches!(
        api.create_company(br#"{"name":"Synthetic"}"#.to_vec(), false)
            .await,
        Err(Error::InvalidInput(_))
    ));
}
