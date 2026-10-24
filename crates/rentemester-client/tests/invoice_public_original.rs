mod common;
// Same one-request fixture, with discovery bound to the actual additive manifest.
struct TcpListener(std::net::TcpListener);
impl TcpListener {
    fn bind(addr: &str) -> std::io::Result<Self> {
        std::net::TcpListener::bind(addr).map(Self)
    }
    fn local_addr(&self) -> std::io::Result<std::net::SocketAddr> {
        self.0.local_addr()
    }
    fn set_nonblocking(&self, value: bool) -> std::io::Result<()> {
        self.0.set_nonblocking(value)
    }
    fn accept(&self) -> std::io::Result<(std::net::TcpStream, std::net::SocketAddr)> {
        loop {
            let (mut stream, addr) = self.0.accept()?;
            stream.set_nonblocking(false)?;
            stream.set_read_timeout(Some(std::time::Duration::from_secs(5)))?;
            let mut peek = [0; 64];
            let n = stream.peek(&mut peek)?;
            if !peek[..n].starts_with(b"GET /api/protocol ") {
                return Ok((stream, addr));
            }
            let mut bytes = vec![];
            while !bytes.ends_with(b"\r\n\r\n") {
                let mut b = [0];
                stream.read_exact(&mut b)?;
                bytes.push(b[0]);
            }
            let header = String::from_utf8_lossy(&bytes).to_lowercase();
            assert!(!header.contains("cookie:") && !header.contains("authorization:"));
            let body = include_str!("../../../protocol/http-v1.json");
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{body}",
                body.len()
            )?;
        }
    }
}
use rentemester_client::{
    ApiClient, Error, OriginalPublicInvoiceKind as Kind, public_invoice_artifact,
    validate_invoice_public_original,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    thread,
};
fn server(status: u16, value: Value) -> (String, thread::JoinHandle<(String, Value)>) {
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
        let header = String::from_utf8_lossy(&bytes[..end]).to_string();
        let length: usize = header
            .lines()
            .find_map(|l| {
                l.to_lowercase()
                    .strip_prefix("content-length:")
                    .map(|v| v.trim().parse().unwrap())
            })
            .unwrap_or(0);
        while bytes.len() < end + length {
            let mut b = [0; 1024];
            let n = s.read(&mut b).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&b[..n]);
        }
        let body = if length == 0 {
            Value::Null
        } else {
            serde_json::from_slice(&bytes[end..end + length]).unwrap()
        };
        let response = value.to_string();
        write!(
            s,
            "HTTP/1.1 {status} Mock\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{response}",
            response.len()
        )
        .unwrap();
        (header, body)
    });
    (origin, t)
}

fn hash(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}
fn result(kind: Kind) -> Value {
    let mut v =
        json!({"ok":true,"invoiceNumber":"INV & æ","errors":[],"appliedRules":["source-rule"]});
    if kind.is_export() {
        v["xml"] = json!("<source>æ &amp; exact</source>\n");
        v["sha256"] = json!(hash(v["xml"].as_str().unwrap()));
    } else {
        v["envelope"] = json!("<envelope> exact </envelope>");
        v["envelopeSha256"] = json!(hash(v["envelope"].as_str().unwrap()));
        v["oioublSha256"] = json!(hash("oioubl"));
        v["submissionReference"] = json!("native/reference");
        v["idempotencyKey"] = json!("server-key");
        v["status"] = json!("acknowledged");
        v["transmissionId"] = json!("actual/id");
        v["duplicate"] = json!(false);
    }
    v
}
#[tokio::test]
async fn all_four_fixed_routes_preserve_exact_source_payloads_and_flat_results() {
    for kind in Kind::ALL {
        let mut body = json!({"documentId":7,"invoiceNumber":" fallback æ "});
        if !kind.is_export() {
            body["confirm"] = json!(true);
        }
        if kind == Kind::SubmitPublicPeppol {
            body["accessPoint"] = json!({"accessPointId":" raw identity ","endpointUrl":"https://metadata.invalid/never-fetch","senderEndpointId":" raw sender "});
            body["acknowledgement"] =
                json!({"transmissionId":" raw ID ","acknowledgedAt":" raw date "});
        }
        if kind == Kind::TransmitDigisense {
            body["companyKey"] = json!("assert-current");
        }
        let expected = result(kind);
        let (origin, t) = server(200, expected.clone());
        let actual = ApiClient::new(&origin, true, None)
            .unwrap()
            .invoice_public_original("synthetic", kind, body.to_string().into_bytes())
            .await
            .unwrap();
        assert_eq!(actual, expected);
        assert!(public_invoice_artifact(kind, &actual).is_ok());
        let (header, sent) = t.join().unwrap();
        assert!(header.starts_with(&format!(
            "POST /api/companies/synthetic/invoices/original/{} ",
            kind.suffix()
        )));
        assert_eq!(sent, body);
    }
}
#[tokio::test]
async fn strict_selectors_config_refusals_hashes_and_unknown_are_fail_closed() {
    for (kind, body) in [
        (Kind::ExportPublic, json!({})),
        (Kind::ExportPublic, json!({"documentId":0})),
        (
            Kind::ExportPublic,
            json!({"documentId":9007199254740992u64}),
        ),
        (Kind::ExportPublic, json!({"invoiceNumber":" "})),
        (
            Kind::ExportPublic,
            json!({"documentId":7,"outPath":"local"}),
        ),
        (
            Kind::TransmitDigisense,
            json!({"documentId":7,"confirm":false}),
        ),
        (
            Kind::TransmitDigisense,
            json!({"documentId":7,"confirm":true,"endpointUrl":"https://invalid"}),
        ),
        (
            Kind::SubmitPublicPeppol,
            json!({"documentId":7,"confirm":true,"accessPoint":{"accessPointId":"a","endpointUrl":"meta","senderEndpointId":"s","secret":"forbidden"}}),
        ),
        (
            Kind::SubmitPublicPeppol,
            json!({"documentId":7,"confirm":true,"accessPoint":{"accessPointId":"a","endpointUrl":"meta","senderEndpointId":"s"},"acknowledgement":{"transmissionId":" " ,"acknowledgedAt":"date"}}),
        ),
    ] {
        assert!(matches!(
            validate_invoice_public_original(kind, &body.to_string().into_bytes()),
            Err(Error::InvalidInput(_))
        ));
    }
    for kind in Kind::ALL {
        let mut body = json!({"documentId":7});
        if !kind.is_export() {
            body["confirm"] = json!(true);
        }
        if kind == Kind::SubmitPublicPeppol {
            body["accessPoint"] =
                json!({"accessPointId":"a","endpointUrl":"meta","senderEndpointId":"s"});
        }
        let refusal = json!({"ok":false,"errors":["Denied"]});
        let (origin, t) = server(200, refusal.clone());
        assert_eq!(
            ApiClient::new(&origin, true, None)
                .unwrap()
                .invoice_public_original("synthetic", kind, body.to_string().into_bytes())
                .await
                .unwrap(),
            refusal
        );
        t.join().unwrap();
        let mut bad = result(kind);
        bad[if kind.is_export() {
            "sha256"
        } else {
            "envelopeSha256"
        }] = json!("bad");
        let (origin, t) = server(200, bad);
        assert_eq!(
            ApiClient::new(&origin, true, None)
                .unwrap()
                .invoice_public_original("synthetic", kind, body.to_string().into_bytes())
                .await,
            Err(if kind == Kind::ExportPublic {
                Error::Protocol
            } else {
                Error::OutcomeUnknown
            })
        );
        t.join().unwrap();
        let (origin, t) = server(503, json!({}));
        assert_eq!(
            ApiClient::new(&origin, true, None)
                .unwrap()
                .invoice_public_original("synthetic", kind, body.to_string().into_bytes())
                .await,
            Err(if kind == Kind::ExportPublic {
                Error::Refused { status: 503 }
            } else {
                Error::OutcomeUnknown
            })
        );
        t.join().unwrap();
        let (origin, t) = server(403, json!({}));
        assert_eq!(
            ApiClient::new(&origin, true, None)
                .unwrap()
                .invoice_public_original("synthetic", kind, body.to_string().into_bytes())
                .await,
            Err(Error::Refused { status: 403 })
        );
        t.join().unwrap();
    }
}

fn request(kind: Kind) -> Vec<u8> {
    let mut body = json!({"documentId":7});
    if !kind.is_export() {
        body["confirm"] = json!(true);
    }
    if kind == Kind::SubmitPublicPeppol {
        body["accessPoint"] =
            json!({"accessPointId":"a","endpointUrl":"meta","senderEndpointId":"s"});
    }
    body.to_string().into_bytes()
}

#[tokio::test]
async fn bounded_422_receipts_preserve_exact_code_errors_for_all_four_operations() {
    for kind in Kind::ALL {
        for rules in [None, Some(json!([])), Some(json!(["source-rule"]))] {
            let mut expected = json!({"ok":false,"code":" physical_source_tampered ","errors":["Denied æ", "Exact second error"]});
            if let Some(rules) = rules {
                expected["appliedRules"] = rules;
            }
            let (origin, t) = server(422, expected.clone());
            assert_eq!(
                ApiClient::new(&origin, true, None)
                    .unwrap()
                    .invoice_public_original("synthetic", kind, request(kind))
                    .await
                    .unwrap(),
                expected
            );
            t.join().unwrap();
        }
        let base = json!({"ok":false,"code":"denied","errors":["REMOTE_SENTINEL"]});
        let mut invalid = vec![json!({}), json!("REMOTE_SENTINEL"), result(kind)];
        for (key, bad) in [
            ("ok", json!(true)),
            ("code", Value::Null),
            ("code", json!(42)),
            ("code", json!(" \t")),
            ("errors", json!([])),
            ("errors", json!([42])),
            ("appliedRules", Value::Null),
            ("appliedRules", json!([42])),
            ("data", json!({})),
            ("xml", json!("artifact")),
            ("envelope", json!("artifact")),
        ] {
            let mut v = base.clone();
            v[key] = bad;
            invalid.push(v);
        }
        let mut missing = base.clone();
        missing.as_object_mut().unwrap().remove("code");
        invalid.push(missing);
        for value in invalid {
            let (origin, t) = server(422, value);
            assert_eq!(
                ApiClient::new(&origin, true, None)
                    .unwrap()
                    .invoice_public_original("synthetic", kind, request(kind))
                    .await,
                Err(Error::Refused { status: 422 })
            );
            t.join().unwrap();
        }
        let (origin, t) = server(403, base);
        assert_eq!(
            ApiClient::new(&origin, true, None)
                .unwrap()
                .invoice_public_original("synthetic", kind, request(kind))
                .await,
            Err(Error::Refused { status: 403 })
        );
        t.join().unwrap();
    }
}

// A raw responder also observes the listener after completion: no automatic replay is allowed.
fn raw_server(status: u16, body: Vec<u8>, extra_length: usize) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let t = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut request = vec![];
        loop {
            let mut buf = [0; 1024];
            let n = stream.read(&mut buf).unwrap();
            assert!(n > 0);
            request.extend_from_slice(&buf[..n]);
            if let Some(end) = request.windows(4).position(|v| v == b"\r\n\r\n") {
                let header = String::from_utf8_lossy(&request[..end]);
                let length: usize = header
                    .lines()
                    .find_map(|line| {
                        line.to_lowercase()
                            .strip_prefix("content-length:")
                            .map(|v| v.trim().parse().unwrap())
                    })
                    .unwrap_or(0);
                if request.len() >= end + 4 + length {
                    break;
                }
            }
        }
        if status != 0 {
            let header = format!(
                "HTTP/1.1 {status} Mock\r\nContent-Length:{}\r\nConnection:close\r\n\r\n",
                body.len() + extra_length
            );
            let _ = stream.write_all(header.as_bytes());
            let _ = stream.write_all(&body);
        }
        drop(stream);
        listener.set_nonblocking(true).unwrap();
        for _ in 0..20 {
            assert!(
                matches!(listener.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock),
                "unexpected retry"
            );
            thread::sleep(std::time::Duration::from_millis(5));
        }
    });
    (origin, t)
}

#[tokio::test]
async fn incomplete_oversized_and_unknown_responses_never_retry_or_relabel_writes() {
    for kind in Kind::ALL {
        for (status, bytes, extra, expected) in [
            (
                422,
                b"not json REMOTE_SENTINEL".to_vec(),
                0,
                Error::Refused { status: 422 },
            ),
            (
                422,
                vec![b'x'; 16 * 1024 * 1024 + 1],
                0,
                Error::Refused { status: 422 },
            ),
            (
                422,
                b"{\"ok\":false}".to_vec(),
                20,
                Error::Refused { status: 422 },
            ),
            (
                200,
                result(kind).to_string().into_bytes(),
                20,
                if kind == Kind::ExportPublic {
                    Error::Transport
                } else {
                    Error::OutcomeUnknown
                },
            ),
            (
                408,
                vec![],
                0,
                if kind == Kind::ExportPublic {
                    Error::Refused { status: 408 }
                } else {
                    Error::OutcomeUnknown
                },
            ),
            (
                503,
                vec![],
                0,
                if kind == Kind::ExportPublic {
                    Error::Refused { status: 503 }
                } else {
                    Error::OutcomeUnknown
                },
            ),
            (
                0,
                vec![],
                0,
                if kind == Kind::ExportPublic {
                    Error::Transport
                } else {
                    Error::OutcomeUnknown
                },
            ),
        ] {
            let (origin, t) = raw_server(status, bytes, extra);
            assert_eq!(
                ApiClient::new(&origin, true, None)
                    .unwrap()
                    .invoice_public_original("synthetic", kind, request(kind))
                    .await,
                Err(expected)
            );
            t.join().unwrap();
        }
    }
}

#[tokio::test]
async fn original_status_distinct_route_assertion_and_response_policy() {
    let body = json!({"documentId":7,"confirm":true,"companyKey":" \t "});
    for (status, value, expected_error) in [
        (200, result(Kind::TransmitDigisense), None),
        (
            200,
            json!({"ok":false,"errors":["Still pending"],"status":"prepared"}),
            None,
        ),
        (
            422,
            json!({"ok":false,"code":"exact_denial","errors":["Denied"]}),
            None,
        ),
        (
            422,
            json!({"ok":false,"errors":[]}),
            Some(Error::Refused { status: 422 }),
        ),
        (200, json!({"ok":true}), Some(Error::OutcomeUnknown)),
        (503, json!({}), Some(Error::OutcomeUnknown)),
    ] {
        let (origin, t) = server(status, value.clone());
        let actual = ApiClient::new(&origin, true, None)
            .unwrap()
            .invoice_public_original_status("synthetic", body.to_string().into_bytes())
            .await;
        if let Some(e) = expected_error {
            assert_eq!(actual, Err(e));
        } else {
            assert_eq!(actual.unwrap(), value);
        }
        let (header, sent) = t.join().unwrap();
        assert!(
            header.starts_with("POST /api/companies/synthetic/invoices/original/status-digisense ")
        );
        assert_eq!(sent, body);
    }
    let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    for bad in [
        json!({"confirm":true}),
        json!({"documentId":7,"confirm":true,"invoiceNumber":"n"}),
        json!({"documentId":7,"confirm":true,"companyKey":null}),
        json!({"documentId":7,"confirm":false}),
    ] {
        assert!(matches!(
            api.invoice_public_original_status("synthetic", bad.to_string().into_bytes())
                .await,
            Err(Error::InvalidInput(_))
        ));
    }
    for (status, bytes, extra, expected) in [
        (200, b"not json".to_vec(), 0, Error::OutcomeUnknown),
        (
            200,
            result(Kind::TransmitDigisense).to_string().into_bytes(),
            20,
            Error::OutcomeUnknown,
        ),
        (503, vec![], 0, Error::OutcomeUnknown),
        (422, b"bad".to_vec(), 0, Error::Refused { status: 422 }),
    ] {
        let (origin, t) = raw_server(status, bytes, extra);
        assert_eq!(
            ApiClient::new(&origin, true, None)
                .unwrap()
                .invoice_public_original_status("synthetic", body.to_string().into_bytes())
                .await,
            Err(expected)
        );
        t.join().unwrap();
    }
}
