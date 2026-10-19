use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

async fn start(
    execute: impl Fn(ResetRequest) -> Result<(), Failure> + Send + Sync + 'static,
) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let app = reset_router(&origin, CookieProfile::Development, execute).unwrap();
    let handle = tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });
    (origin, handle)
}
fn client() -> reqwest::Client {
    reqwest::Client::builder().no_proxy().build().unwrap()
}
fn post(c: &reqwest::Client, o: &str, body: &str) -> reqwest::RequestBuilder {
    c.post(format!("{o}{PATH}"))
        .header("origin", o)
        .header("content-type", "application/json")
        .body(body.to_owned())
}
const BODY: &str = r#"{"email":"Person@Example.invalid","redirectTo":"/reset-password"}"#;

#[tokio::test]
async fn real_http_ack_and_rate_ignore_forwarded_identity() {
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let (o, server) = start(move |r| {
        assert_eq!(r.email(), "person@example.invalid");
        assert_eq!(r.redirect(), Some("/reset-password"));
        count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    })
    .await;
    let c = client();
    for i in 0..4 {
        let r = post(&c, &o, BODY)
            .header("x-forwarded-for", format!("192.0.2.{i}"))
            .header("x-real-ip", format!("198.51.100.{i}"))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status().as_u16(), if i < 3 { 200 } else { 429 });
        assert_eq!(r.headers()["cache-control"], "no-store");
        assert!(!r.headers().contains_key("set-cookie"));
        if i < 3 {
            assert_eq!(r.text().await.unwrap(), ACK);
        }
    }
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    server.abort();
}

#[tokio::test]
async fn real_http_refuses_origin_media_and_structural_ambiguity_before_work() {
    let (o, server) = start(|_| panic!("unexpected executor call")).await;
    let c = client();
    for request in [
        post(&c, &o, BODY).header("origin", "https://evil.invalid"),
        post(&c, &o, BODY).header("sec-fetch-site", "cross-site"),
        post(&c, &o, BODY).header("host", "evil.invalid"),
    ] {
        assert_eq!(
            request.send().await.unwrap().status(),
            StatusCode::FORBIDDEN
        );
    }
    assert_eq!(
        c.post(format!("{o}{PATH}"))
            .header("content-type", "application/json")
            .body(BODY)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        c.post(format!("{o}{PATH}"))
            .header("origin", &o)
            .header("content-type", "text/plain")
            .body(BODY)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNSUPPORTED_MEDIA_TYPE
    );
    // Rate is independent of account validity: each malformed request still consumes one slot.
    for b in [
        r#"{"email":"x@example.invalid","email":"y@example.invalid"}"#,
        r#"{"email":"x@example.invalid","redirectTo":null}"#,
        r#"{"email":"x@example.invalid","subject":"u"}"#,
    ] {
        assert_eq!(
            post(&c, &o, b).send().await.unwrap().status(),
            StatusCode::BAD_REQUEST
        );
    }
    server.abort();
}

#[tokio::test]
async fn real_http_body_cap_and_unimplemented_routes() {
    let (o, server) = start(|_| panic!("unexpected executor call")).await;
    let c = client();
    assert_eq!(
        post(&c, &o, &"x".repeat(BODY_LIMIT + 1))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::PAYLOAD_TOO_LARGE
    );
    assert_eq!(
        c.get(format!("{o}{PATH}")).send().await.unwrap().status(),
        StatusCode::METHOD_NOT_ALLOWED
    );
    assert_eq!(
        post(&c, &o, "{}").send().await.unwrap().status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        c.post(format!("{o}{PATH}/")).send().await.unwrap().status(),
        StatusCode::NOT_FOUND
    );
    server.abort();
}

#[test]
fn sliding_rate_identity_capacity_and_boundary() {
    let state = StateData {
        origin: String::new(),
        authority: String::new(),
        links: RecoveryLinks::new(
            "https://example.invalid",
            &["https://example.invalid"],
            CookieProfile::Secure,
        )
        .unwrap(),
        rate: Mutex::new(HashMap::new()),
        recovery: None,
        auth: None,
        workers: Arc::new(Semaphore::new(4)),
        execute: Some(Box::new(|_| Ok(()))),
    };
    let now = Instant::now();
    let ip = "2001:db8:1:2::1".parse().unwrap();
    assert!(state.allow_at(ip, now));
    assert!(state.allow_at(
        "2001:db8:1:2::2".parse().unwrap(),
        now + Duration::from_secs(30)
    ));
    assert!(state.allow_at(ip, now + Duration::from_secs(60)));
    assert!(!state.allow_at(ip, now + Duration::from_secs(119)));
    assert!(state.allow_at(ip, now + Duration::from_secs(120)));
    assert_eq!(
        canonical_ip("::ffff:192.0.2.1".parse().unwrap()),
        "192.0.2.1".parse::<IpAddr>().unwrap()
    );
    let later = now + Duration::from_secs(180);
    for n in 0..MAX_KEYS {
        assert!(state.allow_at(IpAddr::V4(std::net::Ipv4Addr::from(n as u32)), later));
    }
    assert!(!state.allow_at("198.51.100.1".parse().unwrap(), later));
    assert!(state.allow_at("198.51.100.1".parse().unwrap(), later + WINDOW));
}

#[test]
fn navigation_read_budgets_replenish_without_a_minute_of_inactivity() {
    let state = StateData {
        origin: String::new(), authority: String::new(),
        links: RecoveryLinks::new("https://example.invalid", &["https://example.invalid"], CookieProfile::Secure).unwrap(),
        rate: Mutex::new(HashMap::new()), recovery: None, auth: None,
        workers: Arc::new(Semaphore::new(4)), execute: None,
    };
    let now = Instant::now();
    let ip = "192.0.2.15".parse().unwrap();
    for operation in [Operation::Session, Operation::WorkspaceContext] {
        for n in 0..120 {
            assert!(state.allow_route_at(ip, operation, now + Duration::from_millis(n * 499)));
        }
        assert!(!state.allow_route_at(ip, operation, now + Duration::from_millis(59_999)));
        assert!(state.allow_route_at(ip, operation, now + WINDOW));
        for _ in 1..120 {
            assert!(state.allow_route_at(ip, operation, now + WINDOW));
        }
        assert!(!state.allow_route_at(ip, operation, now + WINDOW));
    }
}

#[test]
fn profiles_and_strict_wire_refuse() {
    assert!(reset_router("http://127.0.0.1", CookieProfile::Secure, |_| Ok(())).is_err());
    assert!(
        reset_router(
            "http://example.invalid",
            CookieProfile::Development,
            |_| Ok(())
        )
        .is_err()
    );
    for b in [
        r#"{"email":null}"#,
        r#"{"email":2}"#,
        r#"{"email":"x@example.invalid"}{}"#,
    ] {
        assert!(serde_json::from_str::<Wire>(b).is_err());
    }
}

#[tokio::test]
async fn real_http_conceals_failure_stages_and_executor_panic() {
    use rentemester_application::reset_request::Stage;
    for mode in 0..4 {
        let (origin, server) = start(move |_| {
            if mode == 3 {
                panic!("synthetic executor panic");
            }
            Err(Failure {
                stage: match mode {
                    0 => Stage::NotCommitted,
                    1 => Stage::OutcomeUnknown,
                    _ => Stage::Committed,
                },
                error: rentemester_storage::password_credential::lookup_email("invalid")
                    .unwrap_err(),
                connection_reusable: false,
            })
        })
        .await;
        let r = post(&client(), &origin, BODY).send().await.unwrap();
        assert_eq!(r.status(), StatusCode::OK);
        assert_eq!(r.headers()["cache-control"], "no-store");
        assert!(!r.headers().contains_key("set-cookie"));
        assert_eq!(r.text().await.unwrap(), ACK);
        server.abort();
    }
}

#[test]
fn parallel_rate_admission_never_exceeds_three() {
    let state = Arc::new(StateData {
        origin: String::new(),
        authority: String::new(),
        links: RecoveryLinks::new(
            "https://example.invalid",
            &["https://example.invalid"],
            CookieProfile::Secure,
        )
        .unwrap(),
        rate: Mutex::new(HashMap::new()),
        recovery: None,
        auth: None,
        workers: Arc::new(Semaphore::new(4)),
        execute: Some(Box::new(|_| Ok(()))),
    });
    let gate = Arc::new(std::sync::Barrier::new(32));
    let jobs: Vec<_> = (0..32)
        .map(|_| {
            let state = state.clone();
            let gate = gate.clone();
            std::thread::spawn(move || {
                gate.wait();
                state.allow("192.0.2.1".parse().unwrap())
            })
        })
        .collect();
    let accepted = jobs
        .into_iter()
        .map(|j| usize::from(j.join().unwrap()))
        .sum::<usize>();
    assert_eq!(accepted, 3);
}

#[tokio::test]
async fn cancellation_keeps_permit_until_blocking_work_finishes() {
    let started = Arc::new(AtomicUsize::new(0));
    let done = Arc::new(AtomicUsize::new(0));
    let a = started.clone();
    let b = done.clone();
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
        recovery: None,
        auth: None,
        workers: Arc::new(Semaphore::new(1)),
        execute: Some(Box::new(move |_| {
            a.store(1, Ordering::SeqCst);
            while b.load(Ordering::SeqCst) == 0 {
                std::thread::sleep(Duration::from_millis(5));
            }
            Ok(())
        })),
    });
    let make = || {
        let mut r = Request::builder()
            .uri(PATH)
            .method("POST")
            .header("origin", "http://127.0.0.1")
            .header("host", "127.0.0.1")
            .header("content-type", "application/json")
            .body(axum::body::Body::from(BODY))
            .unwrap();
        r.extensions_mut()
            .insert(ConnectInfo("127.0.0.1:1234".parse::<SocketAddr>().unwrap()));
        r
    };
    let first = tokio::spawn(request_reset(State(state.clone()), make()));
    tokio::time::timeout(Duration::from_secs(2), async {
        while started.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    first.abort();
    let _ = first.await;
    assert_eq!(state.workers.available_permits(), 0);
    assert_eq!(
        request_reset(State(state.clone()), make()).await.status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    done.store(1, Ordering::SeqCst);
    tokio::time::timeout(Duration::from_secs(2), async {
        while state.workers.available_permits() == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(state.workers.available_permits(), 1);
}

#[tokio::test]
async fn chunked_limit_and_slow_body_use_real_socket() {
    let (o, server) = start(|_| panic!("unexpected executor call")).await;
    let address = o.strip_prefix("http://").unwrap().to_owned();
    let origin = o.clone();
    tokio::task::spawn_blocking(move || {
        use std::io::{BufRead, BufReader, Read, Write};
        fn read_refusal(socket: std::net::TcpStream, status: u16) {
            // Early body rejection may close with RST after the complete response. Read exact
            // HTTP framing instead of requiring a clean TCP EOF; truncated replies still fail.
            let mut reader = BufReader::new(socket);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            assert!(line.starts_with(&format!("HTTP/1.1 {status} ")));
            let mut length = None;
            let mut complete = false;
            for _ in 0..64 {
                line.clear();
                assert!(reader.read_line(&mut line).unwrap() > 0);
                assert!(line.len() <= 8192);
                if line == "\r\n" { complete = true; break; }
                if let Some((name, value)) = line.split_once(':')
                    && name.eq_ignore_ascii_case("content-length") {
                    assert!(length.is_none());
                    length = Some(value.trim().parse::<usize>().unwrap());
                }
            }
            assert!(complete);
            let expected = br#"{"code":"request_refused"}"#;
            assert_eq!(length, Some(expected.len()));
            let mut body = vec![0; expected.len()];
            reader.read_exact(&mut body).unwrap();
            assert_eq!(body, expected);
        }
        let mut socket = std::net::TcpStream::connect(&address).unwrap();
        socket.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        write!(socket,"POST {PATH} HTTP/1.1\r\nHost: {address}\r\nOrigin: {origin}\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n2001\r\n{}\r\n0\r\n\r\n","x".repeat(BODY_LIMIT+1)).unwrap();
        read_refusal(socket,413);
        let mut socket = std::net::TcpStream::connect(&address).unwrap();
        socket.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        write!(socket,"POST {PATH} HTTP/1.1\r\nHost: {address}\r\nOrigin: {origin}\r\nContent-Type: application/json\r\nContent-Length: 100\r\nConnection: close\r\n\r\n{{").unwrap();
        let now=Instant::now();
        read_refusal(socket,408);
        assert!(now.elapsed()>=Duration::from_millis(1800)&&now.elapsed()<Duration::from_secs(5));
    }).await.unwrap();
    server.abort();
}

#[test]
#[ignore = "synthetic native HTTP reset integration server; private fixture paths on stdin only"]
fn probe() {
    use rentemester_runtime::{
        auth::{AuthError, Clock, CookieVerifier},
        recovery::{DeliveryUnconfirmed, ResetMessage, ResetSender},
    };
    use std::io::{BufRead, Write};
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line).unwrap();
    let config: Arc<serde_json::Value> = Arc::new(serde_json::from_str(&line).unwrap());
    struct Fixed(i64);
    impl Clock for Fixed {
        fn now_unix_ms(&self) -> Result<i64, AuthError> {
            Ok(self.0)
        }
    }
    struct Sender {
        path: String,
        log: String,
        fail: bool,
    }
    impl ResetSender for Sender {
        fn enabled(&self) -> bool {
            true
        }
        fn send(&mut self, m: &ResetMessage<'_>) -> Result<(), DeliveryUnconfirmed> {
            let db = rusqlite::Connection::open(&self.path).unwrap();
            db.execute_batch("BEGIN IMMEDIATE;ROLLBACK").unwrap();
            let count: i64 = db
                .query_row(
                    "SELECT count(*) FROM verification WHERE identifier=?1",
                    [format!("reset-password:{}", m.token())],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(count, 1);
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.log)
                .unwrap();
            writeln!(file,"{}",serde_json::json!({"recipient":m.recipient(),"token":m.token(),"url":m.action_url()})).unwrap();
            if self.fail {
                Err(DeliveryUnconfirmed)
            } else {
                Ok(())
            }
        }
    }
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let base = origin.clone();
        let callback_base = origin.clone();
        let callback_config = config.clone();
        let complete_config = config.clone();
        let app = recovery_router(
            &origin,
            CookieProfile::Development,
            move |input| {
                let path = config["control"].as_str().unwrap();
                let mut db = rusqlite::Connection::open(path).unwrap();
                db.execute_batch("PRAGMA foreign_keys=ON;PRAGMA synchronous=FULL;")
                    .unwrap();
                db.busy_timeout(Duration::ZERO).unwrap();
                let cookies = CookieVerifier::new(
                    "w",
                    CookieProfile::Development,
                    config["key"].as_str().unwrap(),
                )
                .unwrap();
                let links =
                    RecoveryLinks::new(&base, &[&base], CookieProfile::Development).unwrap();
                let clock = Fixed(config["now"].as_i64().unwrap());
                let r = rentemester_application::reset_request::Runtime {
                    cookies: &cookies,
                    clock: &clock,
                    links: &links,
                    audit_key_version: 1,
                };
                let mut sender = Sender {
                    path: path.to_owned(),
                    log: config["mailLog"].as_str().unwrap().to_owned(),
                    fail: config["failMail"] == true,
                };
                // Each fixture connection is dropped; unusable handles are never pooled.
                rentemester_application::reset_request::execute(
                    &mut db,
                    &r,
                    &mut sender,
                    input.email(),
                    input.redirect(),
                )
            },
            move |input| {
                let mut db =
                    rusqlite::Connection::open(callback_config["control"].as_str().unwrap())
                        .unwrap();
                db.execute_batch("PRAGMA foreign_keys=ON;PRAGMA synchronous=FULL;")
                    .unwrap();
                db.busy_timeout(Duration::ZERO).unwrap();
                let cookies = CookieVerifier::new(
                    "w",
                    CookieProfile::Development,
                    callback_config["key"].as_str().unwrap(),
                )
                .unwrap();
                let links = RecoveryLinks::new(
                    &callback_base,
                    &[&callback_base],
                    CookieProfile::Development,
                )
                .unwrap();
                let clock = Fixed(callback_config["now"].as_i64().unwrap());
                let r = rentemester_application::reset_request::Runtime {
                    cookies: &cookies,
                    clock: &clock,
                    links: &links,
                    audit_key_version: 1,
                };
                rentemester_application::reset_request::callback(
                    &mut db,
                    &r,
                    input.token(),
                    Some(input.redirect()),
                )
            },
            move |input| {
                let mut db =
                    rusqlite::Connection::open(complete_config["control"].as_str().unwrap())
                        .unwrap();
                db.execute_batch("PRAGMA foreign_keys=ON;PRAGMA synchronous=FULL;")
                    .unwrap();
                db.busy_timeout(Duration::ZERO).unwrap();
                let clock = Fixed(complete_config["now"].as_i64().unwrap());
                let r = rentemester_application::password_reset::Runtime {
                    clock: &clock,
                    audit_key_version: 1,
                };
                let result=rentemester_application::password_reset::execute(&mut db,&r,input.token(),input.password());
                if let Some(path)=complete_config["completionLog"].as_str() {
                    let observed=match &result {Ok(())=>serde_json::json!({"ok":true}),Err(e)=>serde_json::json!({"ok":false,"stage":format!("{:?}",e.stage),"code":e.error.code})};
                    let mut file=std::fs::OpenOptions::new().create(true).append(true).open(path).unwrap();writeln!(file,"{observed}").unwrap();
                }
                result
            },
        )
        .unwrap();
        println!("{}", serde_json::json!({"origin":origin}));
        std::io::stdout().flush().unwrap();
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(async {
            let _ = tokio::task::spawn_blocking(|| {
                let mut line = String::new();
                std::io::stdin().lock().read_line(&mut line).unwrap();
            })
            .await;
        })
        .await
        .unwrap();
    });
}
