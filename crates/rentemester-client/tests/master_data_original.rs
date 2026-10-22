use rentemester_client::{
    ApiClient, Error, MasterDataOriginalKind as Kind, master_data_original_create_schema,
};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
fn server(
    status: u16,
    response: String,
    target: &'static str,
    body: Option<Vec<u8>>,
) -> (ApiClient, Arc<AtomicUsize>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let seen = count.clone();
    let handle = thread::spawn(move || {
        let start = Instant::now();
        let mut last: Option<Instant> = None;
        loop {
            if let Some(t) = last {
                if t.elapsed() >= Duration::from_millis(150) {
                    break;
                }
            } else {
                assert!(
                    start.elapsed() < Duration::from_secs(5),
                    "startup deadline exceeded"
                );
            }
            let (mut s, _) = match listener.accept() {
                Ok(v) => v,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                    continue;
                }
                Err(e) => panic!("{e}"),
            };
            seen.fetch_add(1, Ordering::SeqCst);
            s.set_nonblocking(false).unwrap();
            s.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
            let mut bytes = Vec::new();
            let end = loop {
                let mut chunk = [0; 4096];
                let n = s.read(&mut chunk).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&chunk[..n]);
                if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    break i + 4;
                }
            };
            let headers = String::from_utf8(bytes[..end].to_vec()).unwrap();
            assert!(headers.starts_with(target), "{headers}");
            assert!(headers.to_lowercase().contains("cookie: session=synthetic"));
            let length = headers
                .lines()
                .find_map(|l| {
                    l.to_lowercase()
                        .strip_prefix("content-length:")
                        .map(|s| s.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            while bytes.len() < end + length {
                let mut chunk = [0; 4096];
                let n = s.read(&mut chunk).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&chunk[..n]);
            }
            assert_eq!(&bytes[end..], body.as_deref().unwrap_or_default());
            write!(s,"HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()).unwrap();
            last = Some(Instant::now());
        }
    });
    (
        ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap(),
        count,
        handle,
    )
}
fn once(count: Arc<AtomicUsize>, handle: thread::JoinHandle<()>) {
    handle.join().unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 1);
}

fn customer() -> Value {
    json!({"id":1,"name":"Alpha","address":null,"vatOrCvr":null,"email":null,"phone":null,"website":null,"eanNumber":null,"paymentTermsDays":null,"defaultCurrency":"DKK","notes":null,"archived":false,"createdAt":"2026-01-01"})
}
#[tokio::test]
async fn master_data_original_flat_list_full_nulls_and_strict_shapes() {
    for (value, good) in [
        (
            json!({"ok":true,"count":1,"rows":[customer()],"errors":[]}),
            true,
        ),
        (json!({"ok":true,"count":0,"rows":[],"errors":[]}), true),
        (
            json!({"ok":true,"count":1,"rows":[{"id":1,"name":"partial"}],"errors":[]}),
            false,
        ),
        (
            json!({"ok":true,"count":0,"rows":[customer()],"errors":[]}),
            false,
        ),
        (json!({"ok":true,"data":{"rows":[]}}), false),
    ] {
        let (api, count, t) = server(
            200,
            value.to_string(),
            "GET /api/companies/synthetic/master-data/original/customers?archived=false HTTP/1.1",
            None,
        );
        let result = api
            .master_data_original_list("synthetic", Kind::Customer, false)
            .await;
        if good {
            assert_eq!(result.unwrap(), value);
        } else {
            assert_eq!(result.unwrap_err(), Error::Protocol);
        }
        once(count, t);
    }
}
#[tokio::test]
async fn master_data_original_create_actual_errors_auth_redacted_unknown_once() {
    let body = json!({"input":{"name":"Synthetic"},"confirm":true})
        .to_string()
        .into_bytes();
    for (status, value, error) in [
        (
            200,
            json!({"ok":true,"customerId":7,"appliedRules":["DK-MASTER-DATA-CUSTOMER-001"],"errors":[]}),
            None,
        ),
        (
            200,
            json!({"ok":false,"errors":["actual provider refusal"]}),
            None,
        ),
        (
            422,
            json!({"ok":false,"code":"BUSINESS","errors":["actual account error"],"status":"human_resolution_required"}),
            None,
        ),
        (
            422,
            json!({"ok":false,"code":" ","errors":["private"]}),
            Some(Error::Refused { status: 422 }),
        ),
        (
            401,
            json!({"secret":"credential"}),
            Some(Error::Refused { status: 401 }),
        ),
        (
            403,
            json!({"secret":"credential"}),
            Some(Error::Refused { status: 403 }),
        ),
        (
            500,
            json!({"secret":"credential"}),
            Some(Error::OutcomeUnknown),
        ),
        (
            200,
            json!({"ok":true,"vendorId":7,"appliedRules":["DK-MASTER-DATA-VENDOR-001"],"errors":[]}),
            Some(Error::OutcomeUnknown),
        ),
    ] {
        let (api, count, t) = server(
            status,
            value.to_string(),
            "POST /api/companies/synthetic/master-data/original/customers HTTP/1.1",
            Some(body.clone()),
        );
        let result = api
            .master_data_original_create("synthetic", Kind::Customer, body.clone(), true)
            .await;
        if let Some(e) = error {
            assert_eq!(result.unwrap_err(), e);
        } else {
            assert_eq!(result.unwrap(), value);
        }
        once(count, t);
    }
}
#[tokio::test]
async fn master_data_original_vat_authoritative_invalid_is_success() {
    let value = json!({"ok":true,"validation":{"countryCode":"DK","vatNumber":"12345678","normalized":"DK12345678","valid":false,"name":null,"address":null,"validatedAt":"2026-01-01","expiresAt":"2026-02-01","rawResponse":null},"appliedRules":["DK-VAT-REVERSE-CHARGE-001"],"errors":[]});
    let body = json!({"cvr":"DK12345678","confirm":true})
        .to_string()
        .into_bytes();
    let (api, count, t) = server(
        200,
        value.to_string(),
        "POST /api/companies/synthetic/master-data/original/customer-validate-vat HTTP/1.1",
        Some(body),
    );
    assert_eq!(
        api.customer_original_validate_vat("synthetic", "DK12345678", true)
            .await
            .unwrap(),
        value
    );
    once(count, t);
}
#[tokio::test]
async fn master_data_original_contact_input_schema_and_prehttp_fences() {
    let schema = master_data_original_create_schema(Kind::Customer);
    assert_eq!(schema["additionalProperties"], false);
    assert!(schema["properties"].get("input").is_none());
    assert_eq!(schema["properties"]["paymentTermsDays"]["minimum"], 1);
    let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    for input in [
        json!({"name":0}),
        json!({"note":"no"}),
        json!({"actor":"caller"}),
        json!({"paymentTermsDays":0}),
        json!({"paymentTermsDays":1.5}),
    ] {
        assert!(matches!(
            api.master_data_original_create(
                "synthetic",
                Kind::Customer,
                json!({"input":input,"confirm":true})
                    .to_string()
                    .into_bytes(),
                true
            )
            .await,
            Err(Error::InvalidInput(_))
        ));
    }
    assert!(matches!(
        api.master_data_original_create("synthetic", Kind::Customer, b"{}".to_vec(), false)
            .await,
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        api.customer_original_validate_vat("synthetic", "DK12345678", false)
            .await,
        Err(Error::InvalidInput(_))
    ));
}

#[tokio::test]
async fn master_data_original_sqlite_ascii_order_and_canonical_failed_cvr() {
    let mut rows = Vec::new();
    // SQLite lower() leaves non-ASCII case intact. Both are distinct sort keys;
    // applying Unicode folding would incorrectly demand ascending IDs here.
    for (id, name) in [(1, "Zeta"), (3, "Æble"), (2, "æble")] {
        let mut row = customer();
        row["id"] = json!(id);
        row["name"] = json!(name);
        rows.push(row);
    }
    let value = json!({"ok":true,"count":3,"rows":rows,"errors":[]});
    let (api, count, t) = server(
        200,
        value.to_string(),
        "GET /api/companies/synthetic/master-data/original/customers?archived=false HTTP/1.1",
        None,
    );
    assert_eq!(
        api.master_data_original_list("synthetic", Kind::Customer, false)
            .await
            .unwrap(),
        value
    );
    once(count, t);
    let value = json!({"ok":false,"company":null,"cached":false,"errors":["synthetic provider unavailable"]});
    let (api, count, t) = server(
        200,
        value.to_string(),
        "GET /api/companies/synthetic/master-data/original/cvr-lookup?cvr=55555555 HTTP/1.1",
        None,
    );
    assert_eq!(
        api.master_data_original_cvr_lookup("synthetic", "55555555")
            .await
            .unwrap(),
        value
    );
    once(count, t);
}
