use rentemester_client::{
    ApiClient, Error, OriginalPublicInvoiceKind as Kind, public_invoice_artifact,
    validate_invoice_public_original,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    net::TcpListener,
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
        let refusal = json!({"ok":false,"code":"physical_source_tampered","errors":["Denied"]});
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
