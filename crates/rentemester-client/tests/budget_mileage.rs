use rentemester_client::{ApiClient, Error, MAX_BUDGET_BODY, MAX_MILEAGE_BODY};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::Duration,
};
fn server(
    path: &'static str,
    body: &str,
    status: &'static str,
) -> (String, thread::JoinHandle<()>) {
    let body = body.to_owned();
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", l.local_addr().unwrap());
    let t = thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        let mut bytes = vec![];
        loop {
            let mut chunk = [0; 4096];
            let n = s.read(&mut chunk).unwrap();
            bytes.extend_from_slice(&chunk[..n]);
            if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let h = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                let size = h
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length: "))
                    .map(|s| s.parse::<usize>().unwrap())
                    .unwrap_or(0);
                if bytes.len() >= end + 4 + size {
                    break;
                }
            }
        }
        let wire = String::from_utf8(bytes).unwrap();
        assert!(wire.starts_with(path));
        assert!(wire.ends_with(&body));
        assert!(wire.to_lowercase().contains("cookie: session=synthetic"));
        let reply = r#"{"ok":true,"futureEvidence":{"retained":true},"comparison":[{"budget":null,"variance":null}],"warnings":["native advisory"]}"#;
        write!(
            s,
            "HTTP/1.1 {status}\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{reply}",
            reply.len()
        )
        .unwrap();
        drop(s);
        l.set_nonblocking(true).unwrap();
        thread::sleep(Duration::from_millis(70));
        assert!(l.accept().is_err(), "must never retry");
    });
    (origin, t)
}
const BUDGET: &str =
    "{ \"accountNo\":\"E\",\"period\":\"2026-01\",\"amount\":23.45,\"notes\":\"  revised  \" }\n";
const MILEAGE: &str = r#"{ "tripDate":"2026-01-31","purpose":"Synthetic agent visit","fromLocation":"Synthetic origin","toLocation":"Synthetic destination","kilometers":12.345,"vehicle":"Synthetic car","driver":"Synthetic driver","ratePerKm":3.791,"rateBasis":"Explicit synthetic basis","rateSource":"https://example.invalid/basis","confirm":true }
"#;
#[tokio::test]
async fn budget_and_mileage_exact_writes_once_with_typed_uncertainty() {
    for (budget, path, body) in [
        (
            true,
            "POST /api/companies/synthetic/budget HTTP/1.1",
            BUDGET,
        ),
        (
            false,
            "POST /api/companies/synthetic/mileage HTTP/1.1",
            MILEAGE,
        ),
    ] {
        for status in ["200 OK", "422 Unprocessable Entity", "503 Unavailable"] {
            let (origin, t) = server(path, body, status);
            let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
            let out = if budget {
                api.set_budget("synthetic", body.as_bytes().to_vec(), true)
                    .await
            } else {
                api.register_mileage("synthetic", body.as_bytes().to_vec(), true)
                    .await
            };
            match status {
                "200 OK" => assert_eq!(out.unwrap()["futureEvidence"]["retained"], true),
                "422 Unprocessable Entity" => {
                    assert_eq!(out.unwrap_err(), Error::Refused { status: 422 })
                }
                _ => assert_eq!(out.unwrap_err(), Error::OutcomeUnknown),
            }
            t.join().unwrap();
        }
    }
}
#[tokio::test]
async fn opaque_fiscal_labels_and_scoped_dimension_queries_are_exact_full_json() {
    for (read, path) in [
        (
            0,
            "GET /api/companies/synthetic/budget?year=2026%2F27 HTTP/1.1",
        ),
        (
            1,
            "GET /api/companies/synthetic/budget-vs-actual?year=2026%2F27 HTTP/1.1",
        ),
        (
            2,
            "GET /api/companies/synthetic/budget-dimension-actuals?year=2026%2F27&dimension=project&member=alpha%2Fbeta HTTP/1.1",
        ),
        (
            3,
            "GET /api/companies/synthetic/mileage?year=2026%2F27 HTTP/1.1",
        ),
    ] {
        let (origin, t) = server(path, "", "200 OK");
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        let out = match read {
            0 => api.budget("synthetic", "2026/27").await,
            1 => api.budget_vs_actual("synthetic", "2026/27").await,
            2 => {
                api.budget_dimension_actuals(
                    "synthetic",
                    "2026/27",
                    Some("project"),
                    Some("alpha/beta"),
                )
                .await
            }
            _ => api.mileage("synthetic", "2026/27").await,
        }
        .unwrap();
        assert!(out["comparison"][0]["budget"].is_null());
        assert!(out["comparison"][0]["variance"].is_null());
        assert_eq!(out["warnings"][0], "native advisory");
        t.join().unwrap();
    }
}
#[tokio::test]
async fn selector_size_confirmation_and_no_invented_native_fields() {
    let api = ApiClient::new("http://127.0.0.1:9", true, Some("session=synthetic".into())).unwrap();
    for year in ["", "x\n", &"x".repeat(129)] {
        assert!(matches!(
            api.budget("synthetic", year).await,
            Err(Error::InvalidInput(_))
        ));
    }
    assert!(matches!(
        api.mileage("synthetic", &"x".repeat(17)).await,
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        api.budget_dimension_actuals("synthetic", "2026", None, Some("member"))
            .await,
        Err(Error::InvalidInput(_))
    ));
    for body in [
        BUDGET.replace("23.45", "23.45,\"confirm\":true"),
        BUDGET.replace("23.45", "23.45,\"idempotencyKey\":\"invented\""),
        " ".repeat(MAX_BUDGET_BODY + 1),
    ] {
        assert!(matches!(
            api.set_budget("synthetic", body.into_bytes(), true).await,
            Err(Error::InvalidInput(_))
        ));
    }
    assert!(matches!(
        api.set_budget("synthetic", BUDGET.as_bytes().to_vec(), false)
            .await,
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        api.register_mileage("synthetic", MILEAGE.as_bytes().to_vec(), false)
            .await,
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        api.register_mileage("synthetic", vec![b' '; MAX_MILEAGE_BODY + 1], true)
            .await,
        Err(Error::InvalidInput(_))
    ));
}
#[tokio::test]
async fn read_transport_refusal_never_claims_uncertain_mutation() {
    let (origin, t) = server(
        "GET /api/companies/synthetic/mileage?year=2026 HTTP/1.1",
        "",
        "503 Unavailable",
    );
    let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
    assert_eq!(
        api.mileage("synthetic", "2026").await.unwrap_err(),
        Error::Refused { status: 503 }
    );
    t.join().unwrap();
}
