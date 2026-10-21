#![cfg(unix)]
use rentemester_client::{
    ApiClient, Error, JournalMutation, LocalRuntime, LoginProgress, MfaFactor,
};
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    os::unix::{
        fs::{DirBuilderExt, PermissionsExt, symlink},
        net::UnixListener,
    },
    path::PathBuf,
    thread,
};
fn profile(name: &str, origin: &str) -> (PathBuf, UnixListener) {
    let home = fs::canonicalize(std::env::var_os("HOME").unwrap()).unwrap();
    let path = home.join(format!(".rm138-sdk-{}-{name}", std::process::id()));
    fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
    fs::DirBuilder::new()
        .mode(0o700)
        .create(path.join("workspace"))
        .unwrap();
    let listener = UnixListener::bind(path.join("r.sock")).unwrap();
    fs::set_permissions(path.join("r.sock"), fs::Permissions::from_mode(0o600)).unwrap();
    descriptor(&path, origin);
    (path, listener)
}
fn descriptor(path: &std::path::Path, origin: &str) {
    let value = json!({"format":"rentemester-local-runtime/1","origin":origin,"socket":"r.sock","workspaceRoot":path.join("workspace"),"workspaceId":"synthetic-workspace"});
    fs::write(path.join("r.json"), value.to_string()).unwrap();
    fs::set_permissions(path.join("r.json"), fs::Permissions::from_mode(0o600)).unwrap();
}
fn request(s: &mut impl Read) -> String {
    let mut data = vec![];
    loop {
        let mut b = [0; 4096];
        let n = s.read(&mut b).unwrap();
        assert!(n > 0);
        data.extend(&b[..n]);
        if let Some(i) = data.windows(4).position(|w| w == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&data[..i]).to_lowercase();
            let len = head
                .lines()
                .find_map(|l| l.strip_prefix("content-length:"))
                .map(|s| s.trim().parse::<usize>().unwrap())
                .unwrap_or(0);
            if data.len() >= i + 4 + len {
                break;
            }
        }
    }
    String::from_utf8(data).unwrap()
}
#[tokio::test]
async fn real_unix_login_read_and_mutation_preserve_headers_credentials_and_exact_bytes() {
    let tcp = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", tcp.local_addr().unwrap());
    let (path, listener) = profile("auth", &origin);
    let local = LocalRuntime::load(&path).unwrap();
    assert_eq!(local.workspace_id(), "synthetic-workspace");
    assert_eq!(local.workspace_root(), path.join("workspace"));
    let task = thread::spawn(move || {
        let mut requests = vec![];
        for (cookie, body) in [
            (
                Some("better-auth.two_factor=pending; Path=/; HttpOnly; SameSite=Lax"),
                json!({"twoFactorRedirect":true}),
            ),
            (
                Some("better-auth.session_token=current; Path=/; HttpOnly; SameSite=Lax"),
                json!({"status":true}),
            ),
            (None, json!({"ok":true,"user":"synthetic"})),
            (None, json!({"ok":true,"posted":true})),
        ] {
            let (mut s, _) = listener.accept().unwrap();
            s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            requests.push(request(&mut s));
            let body = body.to_string();
            write!(
                s,
                "HTTP/1.1 200 OK\r\nContent-Length:{}\r\nConnection:close\r\n{}\r\n{body}",
                body.len(),
                cookie
                    .map(|c| format!("Set-Cookie:{c}\r\n"))
                    .unwrap_or_default()
            )
            .unwrap();
        }
        requests
    });
    let mut api = ApiClient::new_local(&local, None).unwrap();
    let LoginProgress::MfaRequired(pending) = api
        .begin_login("synthetic@example.test", "private-password".into())
        .await
        .unwrap()
    else {
        panic!("MFA required")
    };
    let session = api
        .complete_login(&pending, MfaFactor::BackupCode, "private-backup".into())
        .await
        .unwrap();
    api.adopt_session(session).unwrap();
    assert_eq!(api.me().await.unwrap()["user"], "synthetic");
    let bytes =
        br#"{ "confirm":true,"input":{"idempotencyKey":"synthetic-once","text":"preserve me"} }"#
            .to_vec();
    assert_eq!(
        api.manual_journal("synthetic", JournalMutation::Post, bytes.clone())
            .await
            .unwrap()["posted"],
        true
    );
    let requests = task.join().unwrap();
    for request in &requests {
        assert!(request.to_lowercase().contains(&format!(
            "host: {}",
            origin.strip_prefix("http://").unwrap()
        )));
        assert!(
            request
                .to_lowercase()
                .contains(&format!("origin: {origin}"))
        );
    }
    assert!(!requests[0].to_lowercase().contains("\r\ncookie:"));
    assert!(requests[1].contains("cookie: better-auth.two_factor=pending"));
    assert!(requests[2].contains("cookie: better-auth.session_token=current"));
    assert!(requests[3].starts_with("POST /api/companies/synthetic/manual-journal "));
    assert_eq!(
        requests[3].split_once("\r\n\r\n").unwrap().1.as_bytes(),
        bytes
    );
    tcp.set_nonblocking(true).unwrap();
    assert!(tcp.accept().is_err());
    fs::remove_dir_all(path).unwrap();
}
#[test]
fn invalid_private_descriptor_alias_permissions_and_socket_refuse_before_network() {
    let (path, listener) = profile("invalid", "http://127.0.0.1:9");
    let original = fs::read(path.join("r.json")).unwrap();
    for change in [
        json!({"format":"unknown"}),
        json!({"origin":"http://localhost:9"}),
        json!({"origin":"https://127.0.0.1:9"}),
        json!({"origin":"http://127.0.0.1:9/"}),
        json!({"socket":"other.sock"}),
        json!({"workspaceRoot":"/different"}),
        json!({"workspaceId":""}),
        json!({"secret":"unexpected"}),
    ] {
        let mut value: Value = serde_json::from_slice(&original).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .extend(change.as_object().unwrap().clone());
        fs::write(path.join("r.json"), value.to_string()).unwrap();
        assert!(LocalRuntime::load(&path).is_err());
    }
    fs::write(path.join("r.json"), &original).unwrap();
    for target in ["r.json", "r.sock", "workspace"] {
        let p = path.join(target);
        let good = if target == "workspace" { 0o700 } else { 0o600 };
        fs::set_permissions(&p, fs::Permissions::from_mode(good | 0o044)).unwrap();
        assert!(LocalRuntime::load(&path).is_err());
        fs::set_permissions(p, fs::Permissions::from_mode(good)).unwrap();
    }
    let link = path.with_extension("alias");
    symlink(&path, &link).unwrap();
    assert!(LocalRuntime::load(&link).is_err());
    fs::remove_file(link).unwrap();
    fs::rename(path.join("r.json"), path.join("descriptor")).unwrap();
    symlink(path.join("descriptor"), path.join("r.json")).unwrap();
    assert!(LocalRuntime::load(&path).is_err());
    fs::remove_file(path.join("r.json")).unwrap();
    fs::rename(path.join("descriptor"), path.join("r.json")).unwrap();
    fs::hard_link(path.join("r.json"), path.join("linked")).unwrap();
    assert!(LocalRuntime::load(&path).is_err());
    fs::remove_file(path.join("linked")).unwrap();
    assert!(LocalRuntime::load(&path).is_ok());
    drop(listener);
    fs::remove_file(path.join("r.sock")).unwrap();
    assert!(LocalRuntime::load(&path).is_err());
    fs::remove_dir_all(path).unwrap();
}
#[tokio::test]
async fn every_new_client_revalidates_and_stopped_socket_never_uses_live_tcp_origin() {
    let tcp = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", tcp.local_addr().unwrap());
    let (path, listener) = profile("stopped", &origin);
    let local = LocalRuntime::load(&path).unwrap();
    let api = ApiClient::new_local(&local, Some("session=synthetic".into())).unwrap();
    descriptor(&path, "http://127.0.0.1:10");
    assert!(ApiClient::new_local(&local, None).is_err());
    descriptor(&path, &origin);
    drop(listener);
    assert_eq!(api.me().await.unwrap_err(), Error::Transport);
    let service = ApiClient::with_local_service_key(&local, "synthetic-key".into()).unwrap();
    assert_eq!(service.me().await.unwrap_err(), Error::Transport);
    tcp.set_nonblocking(true).unwrap();
    assert!(tcp.accept().is_err());
    fs::remove_dir_all(path).unwrap();
}
