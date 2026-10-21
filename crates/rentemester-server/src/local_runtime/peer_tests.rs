use super::*;
use std::os::unix::fs::PermissionsExt;
use std::sync::atomic::{AtomicUsize, Ordering};
static NEXT: AtomicUsize = AtomicUsize::new(0);
fn socket() -> (std::path::PathBuf, tokio::net::UnixListener, u32) {
    let (stream, _other) = tokio::net::UnixStream::pair().unwrap();
    let uid = stream.peer_cred().unwrap().uid();
    let path = std::env::temp_dir().canonicalize().unwrap().join(format!(
        "rm138-peer-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    let listener = tokio::net::UnixListener::bind(&path).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    (path, listener, uid)
}
#[tokio::test]
async fn actual_unix_uid_rates_are_distinct_from_tcp_and_headers_still_bind() {
    let tcp = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", tcp.local_addr().unwrap());
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let router = reset_router(&origin, CookieProfile::Development, move |_| {
        count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    })
    .unwrap();
    let tcp_app = router.clone();
    let tcp_task = tokio::spawn(async move {
        axum::serve(
            tcp,
            tcp_app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap()
    });
    let (path, uds, uid) = socket();
    let router = admit_unix_peer(router, uid);
    let uds_task = tokio::spawn(async move {
        axum::serve(
            uds,
            router.into_make_service_with_connect_info::<UnixPeer>(),
        )
        .await
        .unwrap()
    });
    let client = reqwest::Client::builder()
        .unix_socket(path.as_path())
        .no_proxy()
        .build()
        .unwrap();
    let body = r#"{"email":"synthetic@example.invalid","redirectTo":"/reset-password"}"#;
    let post = || {
        client
            .post(format!("{origin}{PATH}"))
            .header("origin", &origin)
            .header("content-type", "application/json")
            .body(body)
    };
    assert_eq!(
        post()
            .header("origin", "https://wrong.invalid")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    for i in 0..4 {
        assert_eq!(
            post()
                .header("x-forwarded-for", format!("192.0.2.{i}"))
                .send()
                .await
                .unwrap()
                .status()
                .as_u16(),
            if i < 3 { 200 } else { 429 }
        );
    }
    let tcp_client = reqwest::Client::builder().no_proxy().build().unwrap();
    assert_eq!(
        tcp_client
            .post(format!("{origin}{PATH}"))
            .header("origin", &origin)
            .header("content-type", "application/json")
            .body(body)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(calls.load(Ordering::SeqCst), 4);
    uds_task.abort();
    tcp_task.abort();
    std::fs::remove_file(path).unwrap();
}
#[tokio::test]
async fn unix_transport_never_creates_session_and_wrong_uid_is_denied() {
    for wrong_uid in [false, true] {
        let (path, uds, uid) = socket();
        let cookies = Arc::new(
            rentemester_runtime::auth::CookieVerifier::new(
                "synthetic",
                CookieProfile::Development,
                "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE",
            )
            .unwrap(),
        );
        let handlers = auth_http::AuthHandlers {
            cookies,
            enrollment: None,
            verification: None,
            backup_management: None,
            password_change: None,
            workspace_context: None,
            service_context: None,
            service_lifecycle: None,
            session_control: None,
            sign_in: Box::new(|_| panic!("unexpected login dispatch")),
            totp: Box::new(|_| panic!("unexpected factor dispatch")),
            backup: Box::new(|_| panic!("unexpected backup dispatch")),
            session: Box::new(|_| panic!("transport must not fabricate session")),
            sign_out: Box::new(|_| panic!("unexpected logout")),
        };
        let origin = "http://127.0.0.1:40138";
        let router = admit_unix_peer(
            identity_router(origin, CookieProfile::Development, handlers).unwrap(),
            uid + u32::from(wrong_uid),
        );
        let task = tokio::spawn(async move {
            axum::serve(
                uds,
                router.into_make_service_with_connect_info::<UnixPeer>(),
            )
            .await
            .unwrap()
        });
        let client = reqwest::Client::builder()
            .unix_socket(path.as_path())
            .no_proxy()
            .build()
            .unwrap();
        let r = client
            .get(format!("{origin}/api/auth/get-session"))
            .header("origin", origin)
            .send()
            .await
            .unwrap();
        if wrong_uid {
            assert_eq!(r.status(), StatusCode::FORBIDDEN)
        } else {
            assert_eq!(r.status(), StatusCode::OK);
            assert_eq!(r.text().await.unwrap(), "null");
        }
        task.abort();
        std::fs::remove_file(path).unwrap();
    }
}
