#![cfg(unix)]
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    os::unix::{
        fs::{DirBuilderExt, PermissionsExt},
        net::UnixListener,
    },
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    thread,
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
fn profile(name: &str, origin: &str) -> (PathBuf, UnixListener) {
    let home = fs::canonicalize(std::env::var_os("HOME").unwrap()).unwrap();
    let path = home.join(format!(".rm138-cli-{}-{name}", std::process::id()));
    fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
    fs::DirBuilder::new()
        .mode(0o700)
        .create(path.join("workspace"))
        .unwrap();
    let listener = UnixListener::bind(path.join("r.sock")).unwrap();
    fs::set_permissions(path.join("r.sock"), fs::Permissions::from_mode(0o600)).unwrap();
    fs::write(path.join("r.json"),json!({"format":"rentemester-local-runtime/1","origin":origin,"socket":"r.sock","workspaceRoot":path.join("workspace"),"workspaceId":"synthetic-workspace"}).to_string()).unwrap();
    fs::set_permissions(path.join("r.json"), fs::Permissions::from_mode(0o600)).unwrap();
    (path, listener)
}
fn reply(status: &str, cookie: Option<&str>, value: Value) -> String {
    let body = value.to_string();
    format!(
        "HTTP/1.1 {status}\r\nContent-Length:{}\r\nConnection:close\r\n{}\r\n{body}",
        body.len(),
        cookie
            .map(|c| format!("Set-Cookie:{c}\r\n"))
            .unwrap_or_default()
    )
}
fn serve(listener: UnixListener, replies: Vec<String>) -> thread::JoinHandle<Vec<String>> {
    thread::spawn(move || {
        let mut requests = vec![];
        for reply in replies {
            let (mut s, _) = listener.accept().unwrap();
            s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut request = vec![];
            loop {
                let mut b = [0; 4096];
                let n = s.read(&mut b).unwrap();
                assert!(n > 0);
                request.extend(&b[..n]);
                if let Some(i) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&request[..i]).to_lowercase();
                    let len = head
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length:"))
                        .map(|v| v.trim().parse::<usize>().unwrap())
                        .unwrap_or(0);
                    if request.len() >= i + 4 + len {
                        break;
                    }
                }
            }
            requests.push(String::from_utf8(request).unwrap());
            s.write_all(reply.as_bytes()).unwrap();
        }
        requests
    })
}
fn call(args: &[&str], input: &[u8]) -> Output {
    let mut child = Command::new(BIN)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}
fn local<'a>(profile: &'a Path, command: &'a str, cookie: &'a Path) -> Vec<&'a str> {
    vec![
        "remote",
        command,
        "--local-profile",
        profile.to_str().unwrap(),
        "--cookie-file",
        cookie.to_str().unwrap(),
    ]
}
#[test]
fn normal_mfa_read_mutation_and_direct_constructor_use_only_same_unix_transport() {
    let tcp = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", tcp.local_addr().unwrap());
    let (path, listener) = profile("auth", &origin);
    let cookie = path.join("session.cookie");
    let task = serve(
        listener,
        vec![
            reply(
                "200 OK",
                Some("better-auth.two_factor=pending; Path=/; HttpOnly; SameSite=Lax"),
                json!({"twoFactorRedirect":true}),
            ),
            reply(
                "200 OK",
                Some("better-auth.session_token=current; Path=/; HttpOnly; SameSite=Lax"),
                json!({"status":true}),
            ),
            reply("200 OK", None, json!({"ok":true,"user":"synthetic"})),
            reply("200 OK", None, json!({"ok":true,"posted":true})),
            reply("403 Forbidden", None, json!({"ok":false})),
            reply(
                "200 OK",
                None,
                json!({"ok":true,"journalHistory":{"entries":[],"count":0,"total":0,"limit":0,"offset":0,"hasMore":false}}),
            ),
        ],
    );
    let output = call(
        &[
            "--local-profile",
            path.to_str().unwrap(),
            "remote",
            "login",
            "--email",
            "synthetic@example.test",
            "--factor",
            "backup",
            "--session-out",
            cookie.to_str().unwrap(),
        ],
        b"private-password\nprivate-backup\n",
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(
        fs::read_to_string(&cookie).unwrap(),
        "better-auth.session_token=current"
    );
    assert_eq!(
        fs::metadata(&cookie).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let output = call(&local(&path, "me", &cookie), b"");
    assert_eq!(output.status.code(), Some(0));
    let bytes = br#"{ "confirm":true,"input":{"idempotencyKey":"synthetic-once","text":"exact"} }"#;
    let mut args = local(&path, "post-journal", &cookie);
    args.extend(["--company", "synthetic"]);
    let output = call(&args, bytes);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let mut args = local(&path, "saft-export", &cookie);
    args.extend(["--company", "synthetic", "--confirm"]);
    let output = call(
        &args,
        br#"{"confirm":true,"periodStart":"2026-01-01","periodEnd":"2026-12-31"}"#,
    );
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["code"],
        "server_refused"
    );
    let output = call(
        &[
            "journal",
            "list",
            "--json",
            "--cookie-file",
            cookie.to_str().unwrap(),
            "--company",
            "synthetic",
            "--local-profile",
            path.to_str().unwrap(),
        ],
        b"",
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap(),
        json!([])
    );
    let requests = task.join().unwrap();
    assert!(requests[0].starts_with("POST /api/auth/sign-in/email "));
    assert!(requests[1].starts_with("POST /api/auth/two-factor/verify-backup-code "));
    assert!(!requests[0].to_lowercase().contains("\r\ncookie:"));
    assert!(requests[1].contains("cookie: better-auth.two_factor=pending"));
    for r in &requests[2..] {
        assert!(r.contains("cookie: better-auth.session_token=current"));
    }
    for r in &requests {
        assert!(r.to_lowercase().contains(&format!(
            "host: {}",
            origin.strip_prefix("http://").unwrap()
        )));
        assert!(r.to_lowercase().contains(&format!("origin: {origin}")));
    }
    assert_eq!(
        requests[3].split_once("\r\n\r\n").unwrap().1.as_bytes(),
        bytes
    );
    assert!(requests[4].starts_with("POST /api/companies/synthetic/saft-export "));
    assert!(requests[5].contains("/journal/entries?all=true"));
    tcp.set_nonblocking(true).unwrap();
    assert!(tcp.accept().is_err());
    fs::remove_dir_all(path).unwrap();
}
#[test]
fn help_never_reads_profile_and_global_conflicts_missing_values_are_usage_two() {
    for args in [
        vec![
            "--local-profile",
            "/nonexistent/private",
            "remote",
            "--help",
        ],
        vec![
            "system",
            "export-saft",
            "--local-profile",
            "/nonexistent/private",
            "--help",
        ],
        vec!["--local-profile=/nonexistent/private", "--help"],
    ] {
        let output = call(&args, b"");
        assert_eq!(
            output.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
    for args in [
        vec!["remote", "me", "--local-profile"],
        vec![
            "remote",
            "me",
            "--local-profile",
            "/missing",
            "--local-profile",
            "/other",
        ],
        vec![
            "remote",
            "me",
            "--local-profile",
            "/missing",
            "--server",
            "http://127.0.0.1:9",
        ],
        vec![
            "remote",
            "me",
            "--local-profile",
            "/missing",
            "--allow-loopback-http",
        ],
        vec!["remote", "me", "--local-profile", "/missing"],
    ] {
        let output = call(&args, b"");
        assert_eq!(output.status.code(), Some(2));
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["ok"], false);
        assert_eq!(value["code"], "invalid_input");
    }
}
#[test]
fn stopped_or_missing_socket_and_unsafe_metadata_never_fall_back_to_live_tcp() {
    let tcp = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", tcp.local_addr().unwrap());
    let (path, listener) = profile("stopped", &origin);
    let cookie = path.join("cookie");
    fs::write(&cookie, b"session=synthetic").unwrap();
    fs::set_permissions(&cookie, fs::Permissions::from_mode(0o600)).unwrap();
    drop(listener);
    let output = call(&local(&path, "me", &cookie), b"");
    assert_eq!(output.status.code(), Some(1));
    fs::remove_file(path.join("r.sock")).unwrap();
    let output = call(&local(&path, "me", &cookie), b"");
    assert_eq!(output.status.code(), Some(2));
    fs::set_permissions(path.join("r.json"), fs::Permissions::from_mode(0o644)).unwrap();
    let output = call(&local(&path, "me", &cookie), b"");
    assert_eq!(output.status.code(), Some(2));
    tcp.set_nonblocking(true).unwrap();
    assert!(tcp.accept().is_err());
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn knowledge_alias_and_native_root_share_service_route_and_reject_hostile_selectors() {
    let (path, listener) = profile("knowledge705", "http://127.0.0.1:9");
    let company = path.join("workspace/synthetic");
    fs::create_dir(&company).unwrap();
    fs::create_dir(path.join("workspace/unregistered")).unwrap();
    fs::write(path.join("workspace/workspace.json"),json!({"version":1,"companies":[{"slug":"synthetic","name":"Synthetic","createdAt":"2026-01-01","archived":false}]}).to_string()).unwrap();
    fs::write(path.join("key"), "synthetic-scoped-key").unwrap();
    fs::write(path.join("cookie"), "session=synthetic").unwrap();
    for name in ["key", "cookie"] { fs::set_permissions(path.join(name),fs::Permissions::from_mode(0o600)).unwrap(); }
    let task = serve(listener, vec![reply("200 OK",None,json!({"ok":true})),reply("200 OK",None,json!({"ok":true}))]);
    for (prefix, selector) in [(vec!["remote","company-knowledge"],"synthetic".to_owned()),(vec!["company-knowledge","context"],company.to_str().unwrap().to_owned())] {
        let out = Command::new(BIN).args(prefix).args(["--local-profile",path.to_str().unwrap(),"--company",&selector,"--service-key-file",path.join("key").to_str().unwrap()]).output().unwrap();
        assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stdout));
    }
    for request in task.join().unwrap() {
        assert!(request.starts_with("GET /api/companies/synthetic/knowledge?includeProposed=false HTTP/1.1"));
        let lower = request.to_lowercase();
        assert!(lower.contains("x-rentemester-service-key: synthetic-scoped-key"));
        assert!(!lower.contains("cookie:"));
    }
    std::os::unix::fs::symlink(&company,path.join("workspace/alias")).unwrap();
    for selector in [path.join("workspace/unregistered"),path.join("workspace/alias"),path.clone()] {
        let out=Command::new(BIN).args(["remote","company-knowledge","--local-profile",path.to_str().unwrap(),"--company",selector.to_str().unwrap(),"--service-key-file",path.join("key").to_str().unwrap()]).output().unwrap();
        assert_eq!(out.status.code(),Some(2));
    }
    let out=Command::new(BIN).args(["remote","company-knowledge","--local-profile",path.to_str().unwrap(),"--company","synthetic","--service-key-file",path.join("key").to_str().unwrap(),"--cookie-file",path.join("cookie").to_str().unwrap()]).output().unwrap();
    assert_eq!(out.status.code(),Some(2));
    fs::remove_dir_all(path).unwrap();
}
