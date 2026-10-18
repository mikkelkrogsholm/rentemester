use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
#[ignore = "synthetic admitted native HTTP identity server; private fixture on stdin"]
fn probe() {
    use rentemester_application::{mfa_login, session_control, session_read};
    use rentemester_runtime::{
        auth::{AuthError, Clock},
        mfa::MfaKeyring,
    };
    use std::io::{BufRead, Write};
    struct Fixed(i64);
    impl Clock for Fixed {
        fn now_unix_ms(&self) -> Result<i64, AuthError> {
            Ok(self.0)
        }
    }
    fn observe<T>(log: &Option<String>, operation: &str, result: &Result<T, ContinuationFailure>) {
        if let Some(path) = log {
            let value = match result {
                Ok(_) => serde_json::json!({"operation":operation,"ok":true}),
                Err(e) => {
                    serde_json::json!({"operation":operation,"ok":false,"stage":format!("{:?}",e.stage),"code":e.error.code})
                }
            };
            let mut f = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .unwrap();
            writeln!(f, "{value}").unwrap();
        }
    }
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line).unwrap();
    let config: serde_json::Value = serde_json::from_str(&line).unwrap();
    let outcome_log = config["outcomeLog"].as_str().map(str::to_owned);
    let path = config["control"].as_str().unwrap().to_owned();
    let key = config["key"].as_str().unwrap().to_owned();
    let cookies = Arc::new(CookieVerifier::new("w", CookieProfile::Development, &key).unwrap());
    let keys = Arc::new(MfaKeyring::new(&[(1, key.as_str())], None).unwrap());
    let clock = Arc::new(Fixed(config["now"].as_i64().unwrap()));
    let open = move || {
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute_batch("PRAGMA foreign_keys=ON;PRAGMA synchronous=FULL;")
            .unwrap();
        db.busy_timeout(Duration::ZERO).unwrap();
        db
    };
    let login_c = cookies.clone();
    let login_k = keys.clone();
    let login_t = clock.clone();
    let login_open = open.clone();
    let factor = |backup: bool| {
        let c = cookies.clone();
        let k = keys.clone();
        let t = clock.clone();
        let open = open.clone();
        Box::new(move |i: FactorInput| {
            let mut db = open();
            let runtime = mfa_login::Runtime {
                cookies: &c,
                keys: &k,
                clock: t.as_ref(),
                audit_key_version: 1,
            };
            if backup {
                mfa_login::verify_backup(&mut db, &runtime, i.challenge(), i.code())
            } else {
                mfa_login::verify(&mut db, &runtime, i.challenge(), i.code())
            }
        })
            as Box<dyn Fn(FactorInput) -> Result<VerifyOutcome, ContinuationFailure> + Send + Sync>
    };
    let totp = factor(false);
    let backup = factor(true);
    let session_c = cookies.clone();
    let session_t = clock.clone();
    let session_open = open.clone();
    let logout_c = cookies.clone();
    let logout_t = clock.clone();
    let begin_c = cookies.clone();
    let begin_k = keys.clone();
    let begin_t = clock.clone();
    let begin_open = open.clone();
    let confirm_c = cookies.clone();
    let confirm_k = keys.clone();
    let confirm_t = clock.clone();
    let confirm_open = open.clone();
    let begin_log = outcome_log.clone();
    let confirm_log = outcome_log.clone();
    let enrollment = EnrollmentHandlers {
        begin: Box::new(move |i| {
            let result = rentemester_application::mfa_enrollment::begin(
                &mut begin_open(),
                &mfa_login::Runtime {
                    cookies: &begin_c,
                    keys: &begin_k,
                    clock: begin_t.as_ref(),
                    audit_key_version: 1,
                },
                i.email(),
                i.password(),
                "Rentemester",
            );
            observe(&begin_log, "begin", &result);
            result
        }),
        confirm: Box::new(move |i| {
            let result = rentemester_application::mfa_enrollment::confirm(
                &mut confirm_open(),
                &mfa_login::Runtime {
                    cookies: &confirm_c,
                    keys: &confirm_k,
                    clock: confirm_t.as_ref(),
                    audit_key_version: 1,
                },
                i.challenge(),
                i.code(),
            );
            observe(&confirm_log, "confirm", &result);
            result
        }),
    };
    let email_open = open.clone();
    let email_c = cookies.clone();
    let email_t = clock.clone();
    let mut auth = AuthHandlers {
        enrollment: Some(enrollment),
        verification: None,
        cookies,
        sign_in: Box::new(move |i| {
            mfa_login::begin(
                &mut login_open(),
                &mfa_login::Runtime {
                    cookies: &login_c,
                    keys: &login_k,
                    clock: login_t.as_ref(),
                    audit_key_version: 1,
                },
                i.email(),
                i.password(),
            )
        }),
        totp,
        backup,
        session: Box::new(move |p| {
            session_read::execute(&mut session_open(), &session_c, session_t.as_ref(), &p)
        }),
        sign_out: Box::new(move |p| {
            session_control::execute(
                &mut open(),
                &session_control::Runtime {
                    cookies: &logout_c,
                    clock: logout_t.as_ref(),
                    audit_key_version: 1,
                },
                &p,
                session_control::Operation::SignOut,
            )
        }),
    };
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(async move {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let origin = format!("http://{}", listener.local_addr().unwrap());
            if config["verification"]==true {
                use rentemester_runtime::{email_token::EmailTokenSigner,recovery::{VerificationSender,VerificationMessage,DeliveryUnconfirmed}};
                struct Sender {path:Option<String>,control:String,fail:bool}
                impl VerificationSender for Sender {
                    fn enabled(&self)->bool{true}
                    fn send(&mut self,m:&VerificationMessage)->Result<(),DeliveryUnconfirmed>{
                        let db=rusqlite::Connection::open(&self.control).unwrap();
                        db.busy_timeout(Duration::ZERO).unwrap();
                        db.execute_batch("BEGIN IMMEDIATE;ROLLBACK").expect("verification mail while writer held");
                        if let Some(path)=&self.path {
                            let mut file=std::fs::OpenOptions::new().create(true).append(true).open(path).unwrap();
                            writeln!(file,"{}",serde_json::json!({"recipient":m.recipient(),"token":m.token(),"url":m.action_url()})).unwrap();
                        }
                        if self.fail {Err(DeliveryUnconfirmed)}else{Ok(())}
                    }
                }
                let signer=Arc::new(EmailTokenSigner::new("w",&key).unwrap());
                let links=Arc::new(RecoveryLinks::new(&origin,&[&origin],CookieProfile::Development).unwrap());
                let send_signer=signer.clone();let send_links=links.clone();let send_c=email_c.clone();let send_t=email_t.clone();let send_open=email_open.clone();
                let mail_log=config["mailLog"].as_str().map(str::to_owned);
                let mail_control=config["control"].as_str().unwrap().to_owned();
                let delivery_fail=config["deliveryFail"]==true;let request_log=outcome_log.clone();
                auth.verification=Some(VerificationHandlers{
                    request:Box::new(move|i|{
                        let mut sender=Sender{path:mail_log.clone(),control:mail_control.clone(),fail:delivery_fail};
                        let verification=rentemester_application::email_verification::Runtime{signer:&send_signer,clock:send_t.as_ref(),audit_key_version:1};
                        let r=rentemester_application::verification_request::execute(&mut send_open(),&rentemester_application::verification_request::Runtime{verification:&verification,cookies:&send_c,links:&send_links},&mut sender,i.email(),i.callback(),i.caller());
                        if let Some(path)=&request_log {
                            let value=match &r {Ok(())=>serde_json::json!({"operation":"verification-request","ok":true}),Err(e)=>serde_json::json!({"operation":"verification-request","ok":false,"stage":format!("{:?}",e.stage),"code":e.error.code})};
                            let mut f=std::fs::OpenOptions::new().create(true).append(true).open(path).unwrap();writeln!(f,"{value}").unwrap();
                        }
                        r
                    }),
                    complete:Box::new(move|i|{
                        let verification=rentemester_application::email_verification::Runtime{signer:&signer,clock:email_t.as_ref(),audit_key_version:1};
                        let r=rentemester_application::verification_request::complete(&mut email_open(),&rentemester_application::verification_request::Runtime{verification:&verification,cookies:&email_c,links:&links},i.token(),i.callback()).map(|_|());
                        observe(&outcome_log,"verify-email",&r);r
                    }),
                });
            }
            let app = identity_router(&origin, CookieProfile::Development, auth).unwrap();
            println!("{}", serde_json::json!({"origin":origin}));
            std::io::stdout().flush().unwrap();
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .with_graceful_shutdown(async {
                tokio::task::spawn_blocking(|| {
                    let mut line = String::new();
                    std::io::stdin().read_line(&mut line).unwrap();
                })
                .await
                .unwrap();
            })
            .await
            .unwrap();
        });
}

const LOGIN: &str = "/api/auth/sign-in/email";
const TOTP: &str = "/api/auth/two-factor/verify-totp";
const BACKUP: &str = "/api/auth/two-factor/verify-backup-code";
const SESSION: &str = "/api/auth/get-session";
const LOGOUT: &str = "/api/auth/sign-out";
const BODY: &str =
    r#"{"email":"Person@Example.invalid","password":"synthetic-password","rememberMe":false}"#;
fn verifier(profile: CookieProfile) -> Arc<CookieVerifier> {
    Arc::new(
        CookieVerifier::new("w", profile, "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8").unwrap(),
    )
}
fn failure(unknown: bool) -> ContinuationFailure {
    ContinuationFailure {
        stage: if unknown {
            FailureStage::OutcomeUnknown
        } else {
            FailureStage::NotCommitted
        },
        error: rentemester_storage::password_credential::lookup_email("invalid").unwrap_err(),
        connection_reusable: false,
    }
}
fn handlers(cookies: Arc<CookieVerifier>, calls: Arc<AtomicUsize>, mode: u8) -> AuthHandlers {
    let c = cookies.clone();
    let count = calls.clone();
    let sign_in = Box::new(move |input: LoginInput| {
        count.fetch_add(1, Ordering::SeqCst);
        assert_eq!(input.email(), "person@example.invalid");
        assert_eq!(input.password(), "synthetic-password");
        match mode {
            1 => Err(failure(true)),
            2 => Ok(BeginOutcome::EnrollmentRequired),
            _ => Ok(BeginOutcome::Challenge(
                c.issue_challenge("2fa-synthetic-challenge").unwrap(),
            )),
        }
    });
    let factor = |cookies: Arc<CookieVerifier>, calls: Arc<AtomicUsize>| {
        Box::new(move |input: FactorInput| {
            calls.fetch_add(1, Ordering::SeqCst);
            assert_eq!(input.code(), "123456");
            match mode {
                1 => Err(failure(true)),
                3 => Ok(VerifyOutcome::Rejected(Rejection::AccountLocked)),
                _ => Ok(VerifyOutcome::Session(
                    cookies.issue_session("synthetic-session").unwrap(),
                )),
            }
        })
            as Box<dyn Fn(FactorInput) -> Result<VerifyOutcome, ContinuationFailure> + Send + Sync>
    };
    let totp = factor(cookies.clone(), calls.clone());
    let backup = factor(cookies.clone(), calls.clone());
    let count = calls.clone();
    AuthHandlers {
        enrollment: None,
        verification: None,
        cookies,
        sign_in,
        totp,
        backup,
        session: Box::new(move |_| {
            count.fetch_add(1, Ordering::SeqCst);
            Err(failure(true))
        }),
        sign_out: Box::new(move |_| {
            calls.fetch_add(1, Ordering::SeqCst);
            if mode == 1 {
                Err(failure(true))
            } else {
                Ok(Outcome {
                    sessions: vec![],
                    deleted: 1,
                    clear_cookies: vec![],
                })
            }
        }),
    }
}
async fn start(
    mode: u8,
) -> (
    String,
    tokio::task::JoinHandle<()>,
    Arc<AtomicUsize>,
    Arc<CookieVerifier>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let cookies = verifier(CookieProfile::Development);
    let calls = Arc::new(AtomicUsize::new(0));
    let app = identity_router(
        &origin,
        CookieProfile::Development,
        handlers(cookies.clone(), calls.clone(), mode),
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
    (origin, server, calls, cookies)
}
fn client() -> reqwest::Client {
    reqwest::Client::builder().no_proxy().build().unwrap()
}
fn post(c: &reqwest::Client, o: &str, path: &str, body: &str) -> reqwest::RequestBuilder {
    c.post(format!("{o}{path}"))
        .header("origin", o)
        .header("content-type", "application/json")
        .body(body.to_owned())
}
fn set_cookies(r: &reqwest::Response) -> Vec<String> {
    r.headers()
        .get_all("set-cookie")
        .iter()
        .map(|v| v.to_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn enrollment_routes_require_explicit_trusted_handlers() {
    let (o, server, calls, _) = start(0).await;
    for path in [
        "/api/auth/mfa-enrollment/begin",
        "/api/auth/mfa-enrollment/confirm",
    ] {
        let r = post(&client(), &o, path, "{}").send().await.unwrap();
        assert_eq!(r.status(), StatusCode::NOT_FOUND);
        assert!(set_cookies(&r).is_empty());
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    server.abort();
}

#[tokio::test]
async fn mixed_credentials_never_dispatch_even_with_valid_browser_proofs() {
    // Fresh server per case avoids IP-rate interference with the credential precedence check.
    for path in [LOGIN, TOTP, BACKUP, SESSION, LOGOUT] {
        for header in ["authorization", "x-rentemester-service-key"] {
            for values in [vec![""], vec!["synthetic-key"], vec!["", "another-key"]] {
                let (o, server, calls, cookies) = start(0).await;
                let proof = if path == TOTP || path == BACKUP {
                    cookies.issue_challenge("2fa-synthetic-challenge").unwrap()
                } else {
                    cookies.issue_session("synthetic-session").unwrap()
                };
                let c = client();
                let mut r = if path == SESSION {
                    c.get(format!("{o}{path}"))
                } else {
                    post(
                        &c,
                        &o,
                        path,
                        if path == LOGIN {
                            BODY
                        } else {
                            r#"{"code":"123456"}"#
                        },
                    )
                };
                r = r.header("cookie", proof.request_header());
                for value in values {
                    r = r.header(header, value);
                }
                let r = r.send().await.unwrap();
                assert_eq!(r.status(), StatusCode::UNAUTHORIZED, "{path} {header}");
                assert!(set_cookies(&r).is_empty());
                assert_eq!(calls.load(Ordering::SeqCst), 0);
                server.abort();
            }
        }
    }
}

#[tokio::test]
async fn pending_factor_is_not_session_and_success_cookie_has_browser_lifetime() {
    let (o, server, calls, cookies) = start(0).await;
    let c = client();
    let r = post(&c, &o, LOGIN, BODY).send().await.unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let headers = set_cookies(&r);
    assert_eq!(headers.len(), 5);
    assert!(headers[..4].iter().all(|h| h.contains("Max-Age=0")));
    let challenge = headers[4].split(';').next().unwrap().to_owned();
    assert!(headers[4].contains("Max-Age=600"));
    assert!(r.text().await.unwrap().contains("twoFactorRedirect"));
    let r = c
        .get(format!("{o}{SESSION}"))
        .header("cookie", &challenge)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    assert!(set_cookies(&r).is_empty());
    assert_eq!(r.text().await.unwrap(), "null");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    for path in [TOTP, BACKUP] {
        let r = post(&c, &o, path, r#"{"code":"123456","trustDevice":false}"#)
            .header("cookie", &challenge)
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::OK);
        let headers = set_cookies(&r);
        assert_eq!(headers.len(), 5);
        let session = headers.last().unwrap();
        assert!(!session.contains("Max-Age"));
        assert!(session.contains("HttpOnly; SameSite=Lax"));
        assert!(
            cookies
                .verify(Some(session.split(';').next().unwrap()), None)
                .is_ok()
        );
        assert_eq!(r.text().await.unwrap(), r#"{"status":true}"#);
    }
    server.abort();
}

#[tokio::test]
async fn uncertainty_never_issues_or_clears_authority() {
    let (o, server, calls, cookies) = start(1).await;
    let c = client();
    let challenge = cookies.issue_challenge("2fa-synthetic-challenge").unwrap();
    let session = cookies.issue_session("synthetic-session").unwrap();
    for path in [LOGIN, TOTP, BACKUP, SESSION, LOGOUT] {
        let r = if path == SESSION {
            c.get(format!("{o}{path}"))
        } else {
            post(
                &c,
                &o,
                path,
                match path {
                    LOGIN => BODY,
                    LOGOUT => "{}",
                    _ => r#"{"code":"123456"}"#,
                },
            )
        };
        let r = r
            .header(
                "cookie",
                if path == TOTP || path == BACKUP {
                    challenge.request_header()
                } else {
                    session.request_header()
                },
            )
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::SERVICE_UNAVAILABLE, "{path}");
        assert!(set_cookies(&r).is_empty());
        assert!(r.text().await.unwrap().contains("AUTH_UNCONFIRMED"));
    }
    assert_eq!(calls.load(Ordering::SeqCst), 5);
    server.abort();
}

#[tokio::test]
async fn enrollment_required_clears_previous_identity_without_issuing_session() {
    let (o, server, _, _) = start(2).await;
    let r = post(&client(), &o, LOGIN, BODY).send().await.unwrap();
    assert_eq!(r.status(), StatusCode::CONFLICT);
    let headers = set_cookies(&r);
    assert_eq!(headers.len(), 4);
    assert!(headers.iter().all(|h| h.contains("Max-Age=0")));
    assert!(r.text().await.unwrap().contains("MFA_ENROLLMENT_REQUIRED"));
    server.abort();
}

#[tokio::test]
async fn unsupported_options_and_ambiguous_json_never_dispatch() {
    for (path, body) in [
        (
            LOGIN,
            r#"{"email":"person@example.invalid","password":"synthetic-password","rememberMe":true}"#,
        ),
        (
            LOGIN,
            r#"{"email":"person@example.invalid","password":"synthetic-password","rememberMe":null}"#,
        ),
        (
            LOGIN,
            r#"{"email":"person@example.invalid","password":"a","password":"b"}"#,
        ),
        (TOTP, r#"{"code":"123456","trustDevice":true}"#),
        (BACKUP, r#"{"code":"123456","disableSession":true}"#),
        (TOTP, r#"{"code":"123456","trustDevice":null}"#),
        (BACKUP, r#"{"code":"123456","code":"654321"}"#),
        (LOGOUT, r#"{"userId":"other-user"}"#),
    ] {
        let (o, server, calls, cookies) = start(0).await;
        let proof = cookies.issue_challenge("2fa-synthetic-challenge").unwrap();
        let r = post(&client(), &o, path, body)
            .header("cookie", proof.request_header())
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::BAD_REQUEST, "{path}");
        assert!(set_cookies(&r).is_empty());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        server.abort();
    }
}

#[tokio::test]
async fn session_navigation_gates_and_head_never_dispatch() {
    let (o, server, calls, _) = start(0).await;
    let c = client();
    for r in [
        c.get(format!("{o}{SESSION}"))
            .header("sec-fetch-site", "cross-site"),
        c.get(format!("{o}{SESSION}"))
            .header("origin", "https://evil.invalid"),
    ] {
        assert_eq!(r.send().await.unwrap().status(), StatusCode::FORBIDDEN);
    }
    let r = c.head(format!("{o}{SESSION}")).send().await.unwrap();
    assert_eq!(r.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(r.headers()["allow"], "GET");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let r = c.get(format!("{o}{SESSION}")).send().await.unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    assert_eq!(r.text().await.unwrap(), "null");
    server.abort();
}

#[tokio::test]
async fn logout_accepts_source_empty_body_and_clears_only_known_outcomes() {
    let (o, server, calls, cookies) = start(0).await;
    let c = client();
    let proof = cookies.issue_session("synthetic-session").unwrap();
    for cookie in [None, Some(proof.request_header())] {
        let mut r = c.post(format!("{o}{LOGOUT}")).header("origin", &o);
        if let Some(cookie) = cookie {
            r = r.header("cookie", cookie);
        }
        let r = r.send().await.unwrap();
        assert_eq!(r.status(), StatusCode::OK);
        assert_eq!(set_cookies(&r).len(), 4);
        assert_eq!(r.text().await.unwrap(), r#"{"success":true}"#);
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    server.abort();
}

#[tokio::test]
async fn fixed_factor_rate_cannot_be_bypassed_by_forwarded_identity() {
    let (o, server, calls, cookies) = start(3).await;
    let c = client();
    let proof = cookies.issue_challenge("2fa-synthetic-challenge").unwrap();
    for n in 0..4 {
        let r = post(&c, &o, TOTP, r#"{"code":"123456"}"#)
            .header("cookie", proof.request_header())
            .header("x-forwarded-for", format!("192.0.2.{n}"))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(set_cookies(&r).is_empty());
        if n < 3 {
            assert!(!r.headers().contains_key("retry-after"));
        }
    }
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    server.abort();
}

#[test]
fn cookie_profile_is_bound_and_browser_lifetime_preserves_signature() {
    for profile in [CookieProfile::Development, CookieProfile::Secure] {
        let c = verifier(profile);
        let issued = c.issue_browser_session("synthetic-session").unwrap();
        assert!(!issued.set_cookie().contains("Max-Age"));
        assert_eq!(
            issued.set_cookie().contains("; Secure"),
            profile == CookieProfile::Secure
        );
        assert!(c.verify(Some(issued.request_header()), None).is_ok());
        assert!(
            c.issue_session("synthetic-session")
                .unwrap()
                .set_cookie()
                .contains("Max-Age=43200")
        );
    }
    let c = verifier(CookieProfile::Secure);
    assert!(
        identity_router(
            "http://127.0.0.1",
            CookieProfile::Development,
            handlers(c, Arc::new(AtomicUsize::new(0)), 0)
        )
        .is_err()
    );
}

async fn verification_server(mode: u8) -> (String, tokio::task::JoinHandle<()>, Arc<AtomicUsize>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let calls = Arc::new(AtomicUsize::new(0));
    let request_calls = calls.clone();
    let complete_calls = calls.clone();
    let mut auth = handlers(verifier(CookieProfile::Development), calls.clone(), 0);
    auth.verification = Some(VerificationHandlers {
        request: Box::new(move |_| {
            request_calls.fetch_add(1, Ordering::SeqCst);
            if mode == 3 {
                panic!("synthetic verification request panic");
            }
            if mode == 0 {
                Ok(())
            } else {
                Err(rentemester_application::reset_request::Failure {
                    stage: match mode {
                        1 => rentemester_application::reset_request::Stage::NotCommitted,
                        2 => rentemester_application::reset_request::Stage::OutcomeUnknown,
                        _ => rentemester_application::reset_request::Stage::Committed,
                    },
                    error: failure(false).error,
                    connection_reusable: false,
                })
            }
        }),
        complete: Box::new(move |_| {
            complete_calls.fetch_add(1, Ordering::SeqCst);
            match mode {
                0 => Ok(()),
                1 => Err(failure(false)),
                2 => Err(failure(true)),
                _ => panic!("synthetic verification completion panic"),
            }
        }),
    });
    let app = identity_router(&origin, CookieProfile::Development, auth).unwrap();
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });
    (origin, task, calls)
}
#[tokio::test]
async fn verification_admitted_ack_is_generic_for_error_unknown_delivery_and_panic() {
    for mode in 0..5 {
        let (o, task, calls) = verification_server(mode).await;
        let started = Instant::now();
        let r = reqwest::Client::builder()
            .no_proxy()
            .build()
            .unwrap()
            .post(format!("{o}/api/auth/send-verification-email"))
            .header("origin", &o)
            .header("content-type", "application/json")
            .body(r#"{"email":"u@example.invalid"}"#)
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::OK);
        assert_eq!(r.headers()["cache-control"], "no-store");
        assert!(!r.headers().contains_key("set-cookie"));
        assert_eq!(r.text().await.unwrap(), r#"{"status":true}"#);
        assert!(started.elapsed() >= Duration::from_millis(450));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        task.abort();
    }
}
#[tokio::test]
async fn verification_head_and_invalid_destinations_never_dispatch() {
    let (o, task, calls) = verification_server(0).await;
    let c = reqwest::Client::builder().no_proxy().build().unwrap();
    assert_eq!(
        c.head(format!("{o}/api/auth/verify-email?token=e30.e30.c2ln"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::METHOD_NOT_ALLOWED
    );
    for query in [
        "token=e30.e30.c2ln&callbackURL=https%3A%2F%2Fevil.invalid",
        "token=e30.e30.c2ln&token=e30.e30.c2ln",
        "token=e30.e30.c2ln&callbackURL=%FF",
        "token=%6530.e30.c2ln",
    ] {
        assert_eq!(
            c.get(format!("{o}/api/auth/verify-email?{query}"))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    task.abort();
}
#[tokio::test]
async fn verification_callback_never_turns_unknown_or_panic_into_success() {
    let c = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    for (mode, status, code) in [
        (0, StatusCode::OK, ""),
        (1, StatusCode::UNAUTHORIZED, "INVALID_TOKEN"),
        (
            2,
            StatusCode::SERVICE_UNAVAILABLE,
            "VERIFICATION_UNCONFIRMED",
        ),
        (
            3,
            StatusCode::SERVICE_UNAVAILABLE,
            "VERIFICATION_UNCONFIRMED",
        ),
    ] {
        let (o, task, calls) = verification_server(mode).await;
        let r = c
            .get(format!("{o}/api/auth/verify-email?token=e30.e30.c2ln"))
            .header("sec-fetch-site", "cross-site")
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), status);
        let body = r.text().await.unwrap();
        if mode == 0 {
            assert_eq!(body, r#"{"status":true,"user":null}"#);
        } else {
            assert!(body.contains(code));
        }
        let r=c.get(format!("{o}/api/auth/verify-email?token=e30.e30.c2ln&callbackURL=%2Flogin%3Ftoken%3Dold%26error%3Dold%26verified%3Dtrue")).send().await.unwrap();
        assert_eq!(r.status(), StatusCode::FOUND);
        assert_eq!(
            r.headers()["location"],
            if mode == 0 {
                format!("{o}/login")
            } else {
                format!("{o}/login?error={code}")
            }
        );
        assert!(!r.headers().contains_key("set-cookie"));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        task.abort();
    }
}
