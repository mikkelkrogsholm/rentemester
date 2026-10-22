#![cfg(unix)]
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    os::unix::{
        fs::{DirBuilderExt, PermissionsExt, symlink},
        net::UnixListener,
    },
    path::PathBuf,
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
struct Fixture {
    path: PathBuf,
    listener: UnixListener,
}
impl Fixture {
    fn new(name: &str) -> Self {
        let home = fs::canonicalize(std::env::var_os("HOME").unwrap()).unwrap();
        let path = home.join(format!(".rm158-{}-{name}", std::process::id()));
        fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
        fs::DirBuilder::new()
            .mode(0o700)
            .create(path.join("workspace"))
            .unwrap();
        fs::create_dir(path.join("workspace/synthetic")).unwrap();
        let listener = UnixListener::bind(path.join("r.sock")).unwrap();
        listener.set_nonblocking(true).unwrap();
        fs::set_permissions(path.join("r.sock"), fs::Permissions::from_mode(0o600)).unwrap();
        fs::write(path.join("r.json"), json!({"format":"rentemester-local-runtime/1","origin":"http://127.0.0.1:9","socket":"r.sock","workspaceRoot":path.join("workspace"),"workspaceId":"synthetic-workspace"}).to_string()).unwrap();
        fs::set_permissions(path.join("r.json"), fs::Permissions::from_mode(0o600)).unwrap();
        fs::write(path.join("cookie"), "better-auth.session_token=current").unwrap();
        fs::set_permissions(path.join("cookie"), fs::Permissions::from_mode(0o600)).unwrap();
        Self { path, listener }
    }
    fn call(
        &self,
        args: &[String],
        company: Option<&str>,
        workspace: Option<&str>,
        auth: bool,
    ) -> Output {
        let mut command = Command::new(BIN);
        command
            .current_dir(&self.path)
            .args(args)
            .env_remove("RENTEMESTER_COMPANY")
            .env_remove("RENTEMESTER_WORKSPACE")
            .stdin(Stdio::null());
        if let Some(value) = company {
            command.env("RENTEMESTER_COMPANY", value);
        }
        if let Some(value) = workspace {
            command.env("RENTEMESTER_WORKSPACE", value);
        }
        if auth {
            command.arg("--cookie-file").arg(self.path.join("cookie"));
        }
        command.output().unwrap()
    }
    fn args(&self, company: Option<&str>) -> Vec<String> {
        let mut args = vec![
            format!("--local-profile={}", self.path.display()),
            "journal".into(),
            "list".into(),
            "--json".into(),
        ];
        if let Some(company) = company {
            args.insert(0, format!("--company={company}"));
        }
        args
    }
    fn response(&self, status: &str, body: Value) -> thread::JoinHandle<String> {
        let listener = self.listener.try_clone().unwrap();
        let status = status.to_owned();
        thread::spawn(move || {
            let start = Instant::now();
            let mut socket = loop {
                match listener.accept() {
                    Ok((socket, _)) => break socket,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(
                            start.elapsed() < Duration::from_secs(10),
                            "expected request"
                        );
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(e) => panic!("{e}"),
                }
            };
            socket.set_nonblocking(false).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = Vec::new();
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                let mut bytes = [0; 4096];
                let n = socket.read(&mut bytes).unwrap();
                assert!(n > 0);
                request.extend_from_slice(&bytes[..n]);
            }
            let body = body.to_string();
            write!(
                socket,
                "HTTP/1.1 {status}\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
            String::from_utf8(request).unwrap()
        })
    }
    fn no_request(&self) {
        assert!(self.listener.accept().is_err());
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.path).unwrap();
    }
}
fn success() -> Value {
    json!({"ok":true,"journalHistory":{"entries":[],"count":0,"total":0,"limit":0,"offset":0,"hasMore":false}})
}
#[test]
fn explicit_env_and_verified_absolute_relative_paths_select_same_server_slug() {
    let fixture = Fixture::new("positive");
    let absolute = fixture
        .path
        .join("workspace/synthetic")
        .to_str()
        .unwrap()
        .to_owned();
    for (explicit, env) in [
        (Some(" synthetic "), Some("ignored")),
        (None, Some(" synthetic ")),
        (Some("  "), Some("synthetic")),
        (Some(absolute.as_str()), None),
        (Some("./workspace/synthetic"), None),
    ] {
        let task = fixture.response("200 OK", success());
        let output = fixture.call(
            &fixture.args(explicit),
            env,
            Some(fixture.path.join("workspace").to_str().unwrap()),
            true,
        );
        assert_eq!(
            output.status.code(),
            Some(0),
            "{} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&output.stdout).unwrap(),
            json!([])
        );
        let request = task.join().unwrap();
        assert!(request.starts_with("GET /api/companies/synthetic/journal/entries?all=true "));
        assert!(request.contains("cookie: better-auth.session_token=current"));
        fixture.no_request();
    }
}
#[test]
fn invalid_selection_refuses_before_any_request() {
    let fixture = Fixture::new("bounds");
    symlink(
        fixture.path.join("workspace/synthetic"),
        fixture.path.join("workspace/alias"),
    )
    .unwrap();
    for company in [
        None,
        Some(" "),
        Some("workspace/../workspace/synthetic"),
        Some("workspace/alias"),
        Some("./cookie"),
        Some("./missing"),
        Some("/tmp"),
        Some("a\\b"),
        Some("bad.slug"),
    ] {
        let output = fixture.call(&fixture.args(company), None, None, true);
        assert_eq!(output.status.code(), Some(2), "{company:?}");
        fixture.no_request();
    }
    let output = fixture.call(
        &fixture.args(Some("synthetic")),
        None,
        Some("/different"),
        true,
    );
    assert_eq!(output.status.code(), Some(2));
    fixture.no_request();
    // No local profile: reject raw paths even when connection/credential paths are unusable.
    let output = fixture.call(
        &[
            "journal".into(),
            "list".into(),
            "--company=/missing/company".into(),
            "--cookie-file=/missing/cookie".into(),
        ],
        None,
        None,
        false,
    );
    assert_eq!(output.status.code(), Some(2));
    fixture.no_request();
}
#[test]
fn unknown_revoked_and_unauthenticated_companies_do_not_gain_local_authority() {
    let fixture = Fixture::new("authority");
    for (company, status) in [("unknown", "404 Not Found"), ("revoked", "403 Forbidden")] {
        let task = fixture.response(status, json!({"ok":false}));
        let output = fixture.call(&fixture.args(Some(company)), None, None, true);
        assert_eq!(output.status.code(), Some(1));
        assert!(
            task.join()
                .unwrap()
                .contains(&format!("/api/companies/{company}/"))
        );
        fixture.no_request();
        assert!(!fixture.path.join("workspace").join(company).exists());
    }
    // A mapped existing directory is still not a credential.
    let output = fixture.call(
        &fixture.args(Some("./workspace/synthetic")),
        None,
        None,
        false,
    );
    assert_ne!(output.status.code(), Some(0));
    fixture.no_request();
}

#[test]
fn missing_and_duplicate_company_keep_family_usage_streams_before_credentials() {
    let fixture = Fixture::new("usage-streams");
    let base = vec![
        "invoice".into(),
        "list".into(),
        "--server".into(),
        "http://127.0.0.1:9".into(),
        "--allow-loopback-http".into(),
        "--cookie-file".into(),
        "/nonexistent/private-cookie".into(),
        "--json".into(),
    ];
    for env in [None, Some(""), Some("   ")] {
        let output = fixture.call(&base, env, None, false);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert_eq!(output.stderr, b"Required --company\n");
        fixture.no_request();
    }
    let mut duplicate = base;
    duplicate.extend([
        "--company".into(),
        "synthetic".into(),
        "--company".into(),
        "other".into(),
    ]);
    for env in [None, Some("fallback")] {
        let output = fixture.call(&duplicate, env, None, false);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert_eq!(output.stderr, b"Duplicate --company\n");
        fixture.no_request();
    }
}
