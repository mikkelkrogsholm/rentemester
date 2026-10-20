use rentemester_client::{
    ApiClient, Error, MAX_PUBLIC_INVOICE_BODY, PublicInvoiceMutation as Action,
};
use serde_json::json;
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::Duration,
};
fn http(
    method: &'static str,
    path: &'static str,
    body: &[u8],
    status: &'static str,
    reply: &[u8],
) -> (String, thread::JoinHandle<()>) {
    let body = body.to_vec();
    let reply = reply.to_vec();
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", l.local_addr().unwrap());
    let task = thread::spawn(move || {
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
                let length = h
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length: "))
                    .map(|s| s.parse::<usize>().unwrap())
                    .unwrap_or(0);
                if bytes.len() >= end + 4 + length {
                    assert_eq!(&bytes[end + 4..], body);
                    break;
                }
            }
        }
        assert!(bytes.starts_with(format!("{method} {path} HTTP/1.1").as_bytes()));
        write!(s,"HTTP/1.1 {status}\r\nContent-Type:application/json\r\nContent-Length:{}\r\nConnection:close\r\n\r\n",reply.len()).unwrap();
        s.write_all(&reply).unwrap();
        drop(s);
        l.set_nonblocking(true).unwrap();
        thread::sleep(Duration::from_millis(60));
        assert!(l.accept().is_err(), "no automatic resend/read/retry");
    });
    (origin, task)
}
#[tokio::test]
async fn public_invoice_all_five_routes_preserve_exact_wire_and_canonical_projection() {
    for path in [
        "/api/companies/synthetic/invoices/public-config",
        "/api/companies/synthetic/invoices/public-submissions",
    ] {
        let reply = json!({"ok":true,"publicEInvoice":{"configured":false,"environment":"test","registered":false,"outbound":false},"rows":[],"futureEvidence":{"unknown":null}});
        let (origin, t) = http("GET", path, &[], "200 OK", reply.to_string().as_bytes());
        let api = ApiClient::new(&origin, true, None).unwrap();
        let result = if path.ends_with("public-config") {
            api.public_invoice_config("synthetic").await
        } else {
            api.public_invoice_submissions("synthetic").await
        };
        assert_eq!(result.unwrap(), reply);
        t.join().unwrap();
    }
    for (action, path, body) in [
        (
            Action::Register,
            "/api/companies/synthetic/invoices/public-register",
            r#"{ "confirm":true }"#,
        ),
        (
            Action::Send,
            "/api/companies/synthetic/invoices/send-public",
            r#"{ "invoiceDocumentId":7, "confirm":true }"#,
        ),
        (
            Action::RefreshStatus,
            "/api/companies/synthetic/invoices/send-public/status",
            r#"{ "confirm":true, "invoiceDocumentId":7 }"#,
        ),
    ] {
        let reply = json!({"ok":true,"state":"prepared","transmissionId":"synthetic-transmission","redeliveryBlocked":true});
        let (origin, t) = http(
            "POST",
            path,
            body.as_bytes(),
            "200 OK",
            reply.to_string().as_bytes(),
        );
        let api = ApiClient::new(&origin, true, None).unwrap();
        assert_eq!(
            api.mutate_public_invoice("synthetic", action, body.as_bytes().to_vec(), true)
                .await
                .unwrap(),
            reply
        );
        t.join().unwrap();
    }
}
#[tokio::test]
async fn public_invoice_canonical_states_are_not_reinterpreted_as_delivery() {
    for state in ["prepared", "acknowledged", "failed", "uncertain", "legacy"] {
        let reply = json!({"ok":true,"submission":{"state":state,"transmissionId":null,"redeliveryBlocked":true,"events":[{"source":"retained"}]}});
        let body = br#"{"invoiceDocumentId":7,"confirm":true}"#;
        let (origin, t) = http(
            "POST",
            "/api/companies/synthetic/invoices/send-public",
            body,
            "200 OK",
            reply.to_string().as_bytes(),
        );
        let api = ApiClient::new(&origin, true, None).unwrap();
        assert_eq!(
            api.mutate_public_invoice("synthetic", Action::Send, body.to_vec(), true)
                .await
                .unwrap(),
            reply
        );
        t.join().unwrap();
    }
}
#[tokio::test]
async fn public_invoice_confirmation_scope_and_extra_authority_fail_before_network() {
    let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    for (action, value, confirmed) in [
        (Action::Register, json!({"confirm":true}), false),
        (Action::Register, json!({"confirm":false}), true),
        (
            Action::Register,
            json!({"confirm":true,"companyKey":"forbidden"}),
            true,
        ),
        (
            Action::Send,
            json!({"invoiceDocumentId":0,"confirm":true}),
            true,
        ),
        (
            Action::Send,
            json!({"invoiceDocumentId":9007199254740992u64,"confirm":true}),
            true,
        ),
        (
            Action::Send,
            json!({"invoiceDocumentId":1.5,"confirm":true}),
            true,
        ),
        (
            Action::Send,
            json!({"invoiceDocumentId":7,"confirm":true,"buyer":{"eanNumber":"5790000000000"}}),
            true,
        ),
        (
            Action::RefreshStatus,
            json!({"invoiceDocumentId":7,"confirm":true,"actor":"other"}),
            true,
        ),
    ] {
        assert!(matches!(
            api.mutate_public_invoice(
                "synthetic",
                action,
                value.to_string().into_bytes(),
                confirmed
            )
            .await,
            Err(Error::InvalidInput(_))
        ));
    }
    assert!(matches!(
        api.mutate_public_invoice(
            "synthetic",
            Action::Send,
            vec![b'x'; MAX_PUBLIC_INVOICE_BODY + 1],
            true
        )
        .await,
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        api.public_invoice_config("../hidden").await,
        Err(Error::InvalidInput(_))
    ));
}
#[tokio::test]
async fn public_invoice_unknown_writes_never_retry_or_hide_known_refusal() {
    for (status, reply, expected) in [
        (
            "503 Unavailable",
            br#"{"ok":false}"#.as_slice(),
            Error::OutcomeUnknown,
        ),
        (
            "408 Timeout",
            br#"{"ok":false}"#.as_slice(),
            Error::OutcomeUnknown,
        ),
        ("200 OK", b"malformed".as_slice(), Error::OutcomeUnknown),
        (
            "422 Refused",
            br#"{"ok":false,"code":"source_stale"}"#.as_slice(),
            Error::Refused { status: 422 },
        ),
    ] {
        let body = br#"{"invoiceDocumentId":7,"confirm":true}"#;
        let (origin, t) = http(
            "POST",
            "/api/companies/synthetic/invoices/send-public",
            body,
            status,
            reply,
        );
        let api = ApiClient::new(&origin, true, None).unwrap();
        assert_eq!(
            api.mutate_public_invoice("synthetic", Action::Send, body.to_vec(), true)
                .await
                .err(),
            Some(expected)
        );
        t.join().unwrap();
    }
}
