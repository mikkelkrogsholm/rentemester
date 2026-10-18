use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
const TOKEN: &str = "AbCdEfGhIjKlMnOpQrStUv01";
fn failure(unknown: bool) -> ContinuationFailure {
    ContinuationFailure {
        stage: if unknown {
            rentemester_application::password_reset::FailureStage::OutcomeUnknown
        } else {
            rentemester_application::password_reset::FailureStage::NotCommitted
        },
        error: rentemester_storage::password_credential::lookup_email("invalid").unwrap_err(),
        connection_reusable: false,
    }
}
async fn start(
    callback_mode: u8,
    complete_mode: u8,
) -> (String, tokio::task::JoinHandle<()>, Arc<AtomicUsize>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let links = RecoveryLinks::new(&origin, &[&origin], CookieProfile::Development).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let complete_calls = calls.clone();
    let app = recovery_router(
        &origin,
        CookieProfile::Development,
        |_| Ok(()),
        move |input| match callback_mode {
            1 => Err(failure(false)),
            2 => Ok(RecoveryLinks::new(
                "https://evil.invalid",
                &["https://evil.invalid"],
                CookieProfile::Secure,
            )
            .unwrap()
            .callback(input.token(), Some("/reset"), true)
            .unwrap()),
            _ => Ok(links
                .callback(
                    input.token(),
                    Some(input.redirect()),
                    input.token() == TOKEN,
                )
                .unwrap()),
        },
        move |input| {
            assert_eq!(input.token(), TOKEN);
            assert!(!input.password().is_empty());
            complete_calls.fetch_add(1, Ordering::SeqCst);
            match complete_mode {
                0 => Ok(()),
                1 => Err(failure(false)),
                2 => Err(failure(true)),
                _ => panic!("synthetic completion panic"),
            }
        },
    )
    .unwrap();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });
    (origin, server, calls)
}
fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
}
fn completion(c: &reqwest::Client, o: &str, password: &str) -> reqwest::RequestBuilder {
    c.post(format!("{o}{COMPLETE}"))
        .header("origin", o)
        .header("content-type", "application/json")
        .body(serde_json::json!({"token":TOKEN,"newPassword":password}).to_string())
}
#[tokio::test]
async fn real_mail_navigation_redirect_does_not_require_origin() {
    let (o, server, _) = start(0, 0).await;
    let c = client();
    let r = c
        .get(format!(
            "{o}{COMPLETE}/{TOKEN}?callbackURL=%2Freset-password%3Fx%3Da%2Bb"
        ))
        .header("sec-fetch-site", "cross-site")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::FOUND);
    assert_eq!(
        r.headers()["location"],
        format!("{o}/reset-password?x=a+b&token={TOKEN}")
    );
    assert_eq!(r.headers()["cache-control"], "no-store");
    assert_eq!(r.headers()["referrer-policy"], "no-referrer");
    assert!(!r.headers().contains_key("set-cookie"));
    let r = c
        .get(format!("{o}{COMPLETE}/{TOKEN}?callbackURL="))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::FOUND);
    assert_eq!(
        r.headers()["location"],
        format!("{o}/api/auth/error?error=INVALID_TOKEN")
    );
    server.abort();
}
#[tokio::test]
async fn callback_decodes_once_refuses_query_ambiguity_and_encoded_token() {
    let (o, server, _) = start(0, 0).await;
    let c = client();
    for suffix in [
        "",
        "?callbackURL=%2Freset&callbackURL=%2Fevil",
        "?callbackURL=%2Freset&extra=x",
        "?callbackURL=%FF",
        "?callbackURL=%",
        "?callbackURL=https%3A%2F%2Fevil.invalid%2Freset",
        "?callbackURL=%252F%252Fevil.invalid",
    ] {
        let r = c
            .get(format!("{o}{COMPLETE}/{TOKEN}{suffix}"))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::BAD_REQUEST, "{suffix}");
        assert!(!r.headers().contains_key("location"));
    }
    let r = c
        .get(format!(
            "{o}{COMPLETE}/%41{}?callbackURL=%2Freset",
            &TOKEN[1..]
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    let r = c
        .get(format!("{o}{COMPLETE}/{TOKEN}?callbackURL=%2Freset"))
        .header("origin", "https://evil.invalid")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::FORBIDDEN);
    server.abort();
}
#[tokio::test]
async fn callback_rate_uses_fixed_operation_without_bearer_key() {
    let (o, server, _) = start(0, 0).await;
    let c = client();
    for _ in 0..12 {
        assert_eq!(
            c.head(format!("{o}{COMPLETE}/{TOKEN}?callbackURL=%2Freset"))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::METHOD_NOT_ALLOWED
        );
    }
    for n in 0..11 {
        let token = format!("{n:024}");
        let r = c
            .get(format!("{o}{COMPLETE}/{token}?callbackURL=%2Freset"))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status().as_u16(), if n < 10 { 302 } else { 429 });
    }
    assert_eq!(
        completion(&c, &o, "Synthetic replacement password")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    server.abort();
}
#[tokio::test]
async fn completion_checks_original_utf16_strict_wire_and_body_token() {
    let (o, server, calls) = start(0, 0).await;
    let c = client();
    for (pw, code) in [
        ("x".repeat(11), "PASSWORD_TOO_SHORT"),
        ("😀".repeat(65), "PASSWORD_TOO_LONG"),
    ] {
        let r = completion(&c, &o, &pw).send().await.unwrap();
        assert_eq!(r.status(), StatusCode::BAD_REQUEST);
        assert!(r.text().await.unwrap().contains(code));
    }
    for pw in ["😀".repeat(6), "😀".repeat(64)] {
        let r = completion(&c, &o, &pw).send().await.unwrap();
        assert_eq!(r.status(), StatusCode::OK);
        assert_eq!(r.text().await.unwrap(), r#"{"status":true}"#);
    }
    for body in [
        format!(r#"{{"token":"{TOKEN}","token":"{TOKEN}","newPassword":"long enough password"}}"#),
        format!(r#"{{"token":"{TOKEN}","newPassword":"long enough password","subject":"u"}}"#),
        r#"{"token":null,"newPassword":"long enough password"}"#.into(),
    ] {
        assert_eq!(
            c.post(format!("{o}{COMPLETE}"))
                .header("origin", &o)
                .header("content-type", "application/json")
                .body(body)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(
        c.post(format!("{o}{COMPLETE}?token={TOKEN}"))
            .header("origin", &o)
            .header("content-type", "application/json")
            .body(r#"{"newPassword":"long enough password"}"#)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    server.abort();
}
#[tokio::test]
async fn uncertain_completion_and_bad_callback_never_claim_success_or_replay() {
    let c = client();
    for mode in 0..4 {
        let (o, server, calls) = start(0, mode).await;
        let r = completion(&c, &o, "Synthetic replacement password")
            .send()
            .await
            .unwrap();
        assert_eq!(
            r.status().as_u16(),
            match mode {
                0 => 200,
                1 => 400,
                _ => 503,
            }
        );
        assert!(!r.headers().contains_key("retry-after"));
        assert!(!r.headers().contains_key("set-cookie"));
        let text = r.text().await.unwrap();
        if mode > 1 {
            assert_eq!(
                text,
                r#"{"message":"Reset outcome could not be confirmed","code":"RESET_UNCONFIRMED"}"#
            );
        }
        assert!(!text.contains(TOKEN));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        server.abort();
    }
    for mode in [1, 2] {
        let (o, server, _) = start(mode, 0).await;
        let r = c
            .get(format!("{o}{COMPLETE}/{TOKEN}?callbackURL=%2Freset"))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(!r.headers().contains_key("location"));
        server.abort();
    }
}
#[tokio::test]
async fn completion_body_limit_origin_and_shared_pool_cancel() {
    let (o, server, calls) = start(0, 0).await;
    let c = client();
    assert_eq!(
        c.post(format!("{o}{COMPLETE}"))
            .header("content-type", "application/json")
            .body("{}")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        c.post(format!("{o}{COMPLETE}"))
            .header("origin", &o)
            .header("content-type", "application/json")
            .body("x".repeat(BODY_LIMIT + 1))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::PAYLOAD_TOO_LARGE
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    server.abort();
    let entered = Arc::new(AtomicUsize::new(0));
    let release = Arc::new(AtomicUsize::new(0));
    let a = entered.clone();
    let b = release.clone();
    let state = Arc::new(StateData {
        origin: "http://127.0.0.1".into(),
        authority: "127.0.0.1".into(),
        links: RecoveryLinks::new(
            "http://127.0.0.1",
            &["http://127.0.0.1"],
            CookieProfile::Development,
        )
        .unwrap(),
        rate: Mutex::new(HashMap::new()),
        workers: Arc::new(Semaphore::new(1)),
        execute: Some(Box::new(|_| panic!("overload called request"))),
        auth: None,
        recovery: Some(RecoveryHandlers {
            callback: Box::new(|_| panic!("overload called callback")),
            complete: Box::new(move |_| {
                a.store(1, Ordering::SeqCst);
                while b.load(Ordering::SeqCst) == 0 {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Ok(())
            }),
        }),
    });
    let make = |uri: &str, body: String| {
        let mut r = Request::builder()
            .uri(uri)
            .method("POST")
            .header("origin", "http://127.0.0.1")
            .header("host", "127.0.0.1")
            .header("content-type", "application/json")
            .body(axum::body::Body::from(body))
            .unwrap();
        r.extensions_mut()
            .insert(ConnectInfo("127.0.0.1:1234".parse::<SocketAddr>().unwrap()));
        r
    };
    let first = tokio::spawn(complete_reset(
        State(state.clone()),
        make(
            COMPLETE,
            serde_json::json!({"token":TOKEN,"newPassword":"long enough password"}).to_string(),
        ),
    ));
    tokio::time::timeout(Duration::from_secs(2), async {
        while entered.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    first.abort();
    let _ = first.await;
    assert_eq!(state.workers.available_permits(), 0);
    assert_eq!(
        request_reset(
            State(state.clone()),
            make(PATH, r#"{"email":"u@example.invalid"}"#.into())
        )
        .await
        .status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        callback_reset(
            State(state.clone()),
            make(
                &format!("{COMPLETE}/{TOKEN}?callbackURL=%2Freset"),
                String::new()
            )
        )
        .await
        .status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    release.store(1, Ordering::SeqCst);
    tokio::time::timeout(Duration::from_secs(2), async {
        while state.workers.available_permits() == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}
