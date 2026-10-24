use rentemester_client::{ApiClient, Error, original_import_body};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::Duration,
};
fn fixture(status: u16, reply: Option<Value>) -> (String, thread::JoinHandle<Vec<u8>>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", l.local_addr().unwrap());
    let t = thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut h = vec![];
        while !h.ends_with(b"\r\n\r\n") {
            let mut b = [0];
            s.read_exact(&mut b).unwrap();
            h.push(b[0]);
        }
        assert!(String::from_utf8_lossy(&h).starts_with("GET /api/protocol "));
        assert!(
            !String::from_utf8_lossy(&h)
                .to_lowercase()
                .contains("cookie:")
        );
        let manifest = include_str!("../../../protocol/http-v1.json");
        write!(s,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection:close\r\n\r\n{manifest}",manifest.len()).unwrap();
        drop(s);
        let (mut s, _) = l.accept().unwrap();
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut b = vec![];
        let end = loop {
            let mut c = [0];
            s.read_exact(&mut c).unwrap();
            b.push(c[0]);
            if b.ends_with(b"\r\n\r\n") {
                break b.len();
            }
        };
        let h = String::from_utf8_lossy(&b).to_lowercase();
        assert!(h.starts_with("post /api/companies/synthetic/import-original http/1.1"));
        assert!(h.contains("cookie: session=synthetic"));
        assert!(h.contains("x-rentemester-manifest:"));
        assert!(!h.contains("idempotency-key:"));
        let n: usize = h
            .lines()
            .find_map(|l| l.strip_prefix("content-length:"))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        b.resize(end + n, 0);
        s.read_exact(&mut b[end..]).unwrap();
        if let Some(r) = reply {
            let r = r.to_string();
            write!(s,"HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection:close\r\n\r\n{r}",r.len()).unwrap();
        }
        drop(s);
        l.set_nonblocking(true).unwrap();
        thread::sleep(Duration::from_millis(50));
        assert!(l.accept().is_err(), "unexpected retry");
        b[end..].to_vec()
    });
    (origin, t)
}
fn receipt(system: &str, dry: bool) -> Value {
    let mut v = json!({"ok":true,"sourceSystem":system,"openingBalanceLineCount":0,"historicalEntriesSkipped":0,"auditTrail":[],"appliedRules":[],"errors":[]});
    if dry {
        v["dryRun"] = json!(true)
    }
    v
}
#[test]
fn strict_builder_and_exact_body_validation() {
    for (s, n, b) in [
        ("unknown", "a.csv", b"x".as_slice()),
        ("synthetic-csv", "../a.csv", b"x"),
        ("synthetic-csv", "a.csv", b""),
        ("synthetic-csv", "a.csv", b"\xff"),
        ("dinero", "arbitrary.json", b"{}"),
    ] {
        assert!(original_import_body(s, n, b, false).is_err());
    }
    assert!(
        original_import_body("synthetic-csv", "a.csv", &vec![b'x'; 48 * 1024 + 1], false).is_err()
    );
}
#[tokio::test]
async fn exact_source_body_full_flat_receipt_and_dry_run() {
    for (system, name, raw, dry) in [
        (
            "synthetic-csv",
            "source.csv",
            b"exact;integer;ore\r\n".as_slice(),
            false,
        ),
        ("synthetic-csv", "source.csv", b"exact", true),
        ("dinero", "source.zip", b"PK\x05\x06", false),
        (
            "dinero",
            "dinero-directory-source.json",
            b"{\"format\":\"source\"}",
            true,
        ),
    ] {
        let mut r = receipt(system, dry);
        r["chart"] = json!({"created":[],"existing":[],"updated":[],"differences":[],"conflicts":[],"unmappedVatCodes":[]});
        let (origin, t) = fixture(200, Some(r.clone()));
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        let body = original_import_body(system, name, raw, dry).unwrap();
        assert_eq!(
            api.import_original("synthetic", body.clone())
                .await
                .unwrap(),
            r
        );
        assert_eq!(t.join().unwrap(), body);
    }
}
#[tokio::test]
async fn source_business_false_is_not_flattened_or_zero_filled() {
    let mut r = receipt("dinero", false);
    r["ok"] = json!(false);
    r["errors"] = json!(["Missing Kontoplan.csv"]);
    r["auditTrail"] = json!(["Source archive validated"]);
    let (origin, t) = fixture(400, Some(r.clone()));
    let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
    assert_eq!(
        api.import_original(
            "synthetic",
            original_import_body("dinero", "source.zip", b"PK", false).unwrap()
        )
        .await
        .unwrap(),
        r
    );
    t.join().unwrap();
}
#[tokio::test]
async fn unknown_receipts_lost_5xx_and_known_authority() {
    let mut wrong_count = receipt("synthetic-csv", false);
    wrong_count["openingBalanceLineCount"] = json!("0");
    for (status, r, expected) in [
        (200, None, Error::OutcomeUnknown),
        (
            503,
            Some(json!({"ok":false,"code":"unavailable"})),
            Error::OutcomeUnknown,
        ),
        (200, Some(receipt("dinero", false)), Error::OutcomeUnknown),
        (200, Some(wrong_count), Error::OutcomeUnknown),
        (
            200,
            Some(receipt("synthetic-csv", true)),
            Error::OutcomeUnknown,
        ),
        (
            200,
            Some(json!({"ok":true,"openingBalance":{}})),
            Error::OutcomeUnknown,
        ),
        (
            403,
            Some(json!({"ok":false,"code":"access_denied"})),
            Error::Refused { status: 403 },
        ),
    ] {
        let (origin, t) = fixture(status, r);
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        assert_eq!(
            api.import_original(
                "synthetic",
                original_import_body("synthetic-csv", "source.csv", b"exact", false).unwrap()
            )
            .await,
            Err(expected)
        );
        t.join().unwrap();
    }
}
#[tokio::test]
async fn reject_unknown_metadata_noncanonical_base64_and_missing_fields_before_transport() {
    let api = ApiClient::new("https://example.invalid", false, None).unwrap();
    let b = original_import_body("synthetic-csv", "a.csv", b"x", false).unwrap();
    let original: Value = serde_json::from_slice(&b).unwrap();
    for v in [
        {
            let mut v = original.clone();
            v["actor"] = json!("user:forged");
            v
        },
        {
            let mut v = original.clone();
            v["contentBase64"] = json!("eB==");
            v
        },
        {
            let mut v = original.clone();
            v.as_object_mut().unwrap().remove("dryRun");
            v
        },
    ] {
        assert!(matches!(
            api.import_original("synthetic", serde_json::to_vec(&v).unwrap())
                .await,
            Err(Error::InvalidInput(_))
        ));
    }
}
