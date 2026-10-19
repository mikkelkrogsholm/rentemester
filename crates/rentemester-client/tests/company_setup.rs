use rentemester_client::{ApiClient, CompanySetupMutation as Op, Error, MAX_COMPANY_SETUP_BODY};
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
        let reply = r#"{"ok":true,"company":{"payment":null},"accounts":{"accountRoles":{"status":"incomplete","missing":["bank"],"proposals":[]}},"resolution":{"ok":true,"role":"input_vat","accountNo":"V","version":2,"confirmationSource":"explicit"}}"#;
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
#[tokio::test]
async fn native_setup_methods_bodies_and_outcomes_are_exact_once() {
    for (op, path, body) in [
        (
            Op::UpdateProfile,
            "PATCH /api/companies/synthetic/company HTTP/1.1",
            "{ \"address\":\"  Synthetic Street 9  \",\"vatPeriodType\":null,\"paymentTermsDays\":0 }\n",
        ),
        (
            Op::CreateAccount,
            "POST /api/companies/synthetic/accounts HTTP/1.1",
            r#"{"accountNo":"  V  ","name":"Synthetic VAT","type":"vat","normalBalance":"debit","defaultVatCode":null}"#,
        ),
        (
            Op::ConfirmAccountRole,
            "POST /api/companies/synthetic/account-roles HTTP/1.1",
            r#"{"role":"input_vat","accountNo":"V","confirm":true}"#,
        ),
    ] {
        for status in ["200 OK", "422 Unprocessable Entity", "503 Unavailable"] {
            let (origin, t) = server(path, body, status);
            let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
            let out = api
                .company_setup_mutation("synthetic", op, body.as_bytes().to_vec(), true)
                .await;
            match status {
                "200 OK" => {
                    let v = out.unwrap();
                    assert!(v["company"]["payment"].is_null());
                    assert_eq!(v["accounts"]["accountRoles"]["status"], "incomplete");
                    assert_eq!(v["resolution"]["confirmationSource"], "explicit");
                }
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
async fn canonical_profile_reuses_queryless_read_without_unknown_mutation() {
    let (origin, t) = server(
        "GET /api/companies/synthetic/company HTTP/1.1",
        "",
        "503 Unavailable",
    );
    let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
    assert_eq!(
        api.company_profile("synthetic").await.unwrap_err(),
        Error::Refused { status: 503 }
    );
    t.join().unwrap();
}
#[tokio::test]
async fn setup_does_not_invent_profile_fields_inner_confirmation_or_keys() {
    let api = ApiClient::new("http://127.0.0.1:9", true, Some("session=synthetic".into())).unwrap();
    for (op, body) in [
        (
            Op::UpdateProfile,
            r#"{"name":"new arbitrary company"}"#.to_owned(),
        ),
        (Op::UpdateProfile, r#"{"fiscalYearStartMonth":7}"#.into()),
        (
            Op::UpdateProfile,
            r#"{"address":"Synthetic","confirm":true}"#.into(),
        ),
        (
            Op::UpdateProfile,
            r#"{"address":"Synthetic","idempotencyKey":"invented"}"#.into(),
        ),
        (
            Op::CreateAccount,
            r#"{"accountNo":"V","name":"VAT","type":"vat","normalBalance":"debit","active":false}"#
                .into(),
        ),
        (
            Op::ConfirmAccountRole,
            r#"{"role":"input_vat","accountNo":"V","confirm":false}"#.into(),
        ),
        (
            Op::ConfirmAccountRole,
            r#"{"role":"input_vat","accountNo":"V"}"#.into(),
        ),
        (Op::UpdateProfile, " ".repeat(MAX_COMPANY_SETUP_BODY + 1)),
    ] {
        assert!(matches!(
            api.company_setup_mutation("synthetic", op, body.into_bytes(), true)
                .await,
            Err(Error::InvalidInput(_))
        ));
    }
    assert!(matches!(
        api.company_setup_mutation("synthetic", Op::UpdateProfile, b"{}".to_vec(), false)
            .await,
        Err(Error::InvalidInput(_))
    ));
}
