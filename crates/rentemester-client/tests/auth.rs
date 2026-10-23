mod common;
use common::TcpListener;
use rentemester_client::{ApiClient, Error, LoginProgress, MfaFactor};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    sync::mpsc,
    thread,
};
fn response(status: &str, cookie: Option<&str>, body: Value) -> String {
    let body = body.to_string();
    format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n{}\r\n{body}",
        body.len(),
        cookie
            .map(|cookie| format!("Set-Cookie: {cookie}\r\n"))
            .unwrap_or_default()
    )
}
fn mock(replies: Vec<String>) -> (String, mpsc::Receiver<String>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let (send, recv) = mpsc::channel();
    let task = thread::spawn(move || {
        for reply in replies {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                .unwrap();
            let mut bytes = Vec::new();
            loop {
                let mut chunk = [0; 4096];
                let n = stream.read(&mut chunk).unwrap();
                if n == 0 {
                    break;
                }
                bytes.extend_from_slice(&chunk[..n]);
                if let Some(index) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&bytes[..index]).to_lowercase();
                    let length = header
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length: "))
                        .map(|v| v.parse::<usize>().unwrap())
                        .unwrap_or(0);
                    if bytes.len() >= index + 4 + length {
                        break;
                    }
                }
            }
            send.send(String::from_utf8(bytes).unwrap()).unwrap();
            stream.write_all(reply.as_bytes()).unwrap();
        }
    });
    (origin, recv, task)
}
const PENDING: &str =
    "better-auth.two_factor=pending-secret; Path=/; HttpOnly; SameSite=Lax; Max-Age=600";
const CURRENT: &str = "better-auth.session_token=current-secret; Path=/; HttpOnly; SameSite=Lax";
#[tokio::test]
async fn correct_origin_bound_challenge_to_current_session_transition() {
    let challenge = response("200 OK",Some(PENDING),json!({"twoFactorRedirect":true,"twoFactorMethods":["totp"]})).replace("Set-Cookie:","Set-Cookie: better-auth.two_factor=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0\r\nSet-Cookie:");
    let (origin, requests, task) = mock(vec![
        challenge,
        response("200 OK", Some(CURRENT), json!({"status":true})),
        response("200 OK", None, json!({"user":"synthetic"})),
    ]);
    let mut api = ApiClient::new(
        &origin,
        true,
        Some("old-session=never-send-to-login".into()),
    )
    .unwrap();
    let LoginProgress::MfaRequired(pending) = api
        .begin_login("synthetic@example.test", "password-secret".into())
        .await
        .unwrap()
    else {
        panic!("expected challenge")
    };
    let other = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    assert!(matches!(
        other
            .complete_login(&pending, MfaFactor::Totp, "123456".into())
            .await,
        Err(Error::InvalidInput(_))
    ));
    let session = api
        .complete_login(&pending, MfaFactor::Totp, "123456".into())
        .await
        .unwrap();
    api.adopt_session(session).unwrap();
    assert_eq!(api.me().await.unwrap()["user"], "synthetic");
    let login = requests.recv().unwrap();
    assert!(login.starts_with("POST /api/auth/sign-in/email "));
    assert!(!login.to_lowercase().contains("\r\ncookie:"));
    let body: Value = serde_json::from_str(login.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(
        body,
        json!({"email":"synthetic@example.test","password":"password-secret","rememberMe":false})
    );
    let factor = requests.recv().unwrap();
    assert!(factor.starts_with("POST /api/auth/two-factor/verify-totp "));
    assert!(factor.contains("cookie: better-auth.two_factor=pending-secret"));
    assert!(!factor.contains("old-session"));
    assert!(factor.contains("\"trustDevice\":false"));
    let current = requests.recv().unwrap();
    assert!(current.contains("cookie: better-auth.session_token=current-secret"));
    assert!(!current.contains("pending-secret"));
    task.join().unwrap();
}
#[tokio::test]
async fn enrollment_required_and_auth_refusals_are_typed_without_secrets() {
    let (origin, requests, task) = mock(vec![response(
        "409 Conflict",
        None,
        json!({"code":"MFA_ENROLLMENT_REQUIRED"}),
    )]);
    let api = ApiClient::new(&origin, true, None).unwrap();
    assert!(matches!(
        api.begin_login("user@example.test", "password-secret".into())
            .await,
        Ok(LoginProgress::EnrollmentRequired)
    ));
    requests.recv().unwrap();
    task.join().unwrap();
    let (origin, requests, task) = mock(vec![response(
        "401 Unauthorized",
        None,
        json!({"message":"password-secret"}),
    )]);
    let api = ApiClient::new(&origin, true, None).unwrap();
    let error = api
        .begin_login("user@example.test", "password-secret".into())
        .await
        .err()
        .unwrap();
    assert_eq!(error, Error::Refused { status: 401 });
    assert!(!error.to_string().contains("password-secret"));
    requests.recv().unwrap();
    task.join().unwrap();
}
#[tokio::test]
async fn backup_route_and_cookie_file_never_overwrite_existing_profile() {
    let (origin, requests, task) = mock(vec![
        response("200 OK", Some(PENDING), json!({"twoFactorRedirect":true})),
        response("200 OK", Some(CURRENT), json!({"status":true})),
    ]);
    let api = ApiClient::new(&origin, true, None).unwrap();
    let LoginProgress::MfaRequired(pending) = api
        .begin_login("user@example.test", "password-secret".into())
        .await
        .unwrap()
    else {
        panic!("challenge required")
    };
    let session = api
        .complete_login(&pending, MfaFactor::BackupCode, "backup-secret".into())
        .await
        .unwrap();
    let path = std::env::temp_dir().join(format!("rm-auth-output-{}.cookie", std::process::id()));
    session.write_cookie_file_new(&path).unwrap();
    assert_eq!(
        rentemester_client::read_cookie_file(&path).unwrap(),
        "better-auth.session_token=current-secret"
    );
    assert!(session.write_cookie_file_new(&path).is_err());
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(std::fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
    }
    std::fs::remove_file(path).unwrap();
    requests.recv().unwrap();
    let factor = requests.recv().unwrap();
    assert!(factor.starts_with("POST /api/auth/two-factor/verify-backup-code "));
    assert!(factor.contains("backup-secret"));
    task.join().unwrap();
}
#[tokio::test]
async fn wrong_or_cross_domain_auth_cookie_is_unknown_and_never_adopted() {
    for cookie in [
        "better-auth.session_token=unexpected; Path=/; HttpOnly; SameSite=Lax",
        "better-auth.two_factor=pending; Domain=other.test; Path=/; HttpOnly; SameSite=Lax",
    ] {
        let (origin, requests, task) = mock(vec![response(
            "200 OK",
            Some(cookie),
            json!({"twoFactorRedirect":true}),
        )]);
        let api = ApiClient::new(&origin, true, None).unwrap();
        assert!(matches!(
            api.begin_login("user@example.test", "secret".into()).await,
            Err(Error::OutcomeUnknown)
        ));
        requests.recv().unwrap();
        task.join().unwrap();
    }
}
