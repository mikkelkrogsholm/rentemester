#![cfg(target_os = "macos")]
use serde_json::{Value, json};
use std::{
    net::TcpListener,
    os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::Duration,
};
struct Synthetic(PathBuf);
impl Drop for Synthetic {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
struct Server(Child);
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn spawn(config: &Path, web: &Path, create: bool, mask: &str) -> (Server, String) {
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let addr = format!("127.0.0.1:{port}");
    let mut command = Command::new("/bin/sh");
    command
        .args(["-c", &format!("umask {mask}; exec \"$@\""), "local-test"])
        .arg(env!("CARGO_BIN_EXE_rentemester-server"))
        .args([
            "--listen",
            &addr,
            "--config",
            config.to_str().unwrap(),
            "--web-dir",
            web.to_str().unwrap(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    if create {
        command.arg("--create");
    }
    (Server(command.spawn().unwrap()), format!("http://{addr}"))
}
async fn healthy(server: &mut Server, origin: &str) {
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    for _ in 0..100 {
        if let Some(status) = server.0.try_wait().unwrap() {
            panic!("server exited before health: {status}");
        }
        if let Ok(response) = client.get(format!("{origin}/api/health")).send().await {
            let body: Value = serde_json::from_slice(&response.bytes().await.unwrap()).unwrap();
            assert_eq!(body["workspaceReady"], true);
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("local server failed to become healthy");
}
async fn stop_cleanly(server: &mut Server, signal: &str) {
    assert!(Command::new("/bin/kill")
        .args([signal, &server.0.id().to_string()])
        .status().unwrap().success());
    for _ in 0..200 {
        if let Some(status) = server.0.try_wait().unwrap() {
            assert!(status.success(), "shutdown must drain and exit successfully: {status}");
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("server failed to drain after {signal}");
}
#[tokio::test]
async fn explicit_native_profile_creates_owned_workspace_excludes_second_owner_and_restarts() {
    let token = rentemester_runtime::auth::new_token().unwrap();
    let appdata = PathBuf::from(std::env::var_os("HOME").unwrap())
        .join("Library/Application Support")
        .canonicalize()
        .unwrap();
    let base = appdata.join(format!("Rentemester-Synthetic-{}", token.as_str()));
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&base)
        .unwrap();
    let base = Synthetic(base);
    let web = base.0.join("web");
    std::fs::create_dir(&web).unwrap();
    std::fs::write(
        web.join("index.html"),
        "<!doctype html><title>Synthetic</title>",
    )
    .unwrap();
    let uid = std::fs::metadata(&base.0).unwrap().uid();
    assert_ne!(
        uid, 0,
        "run this local-user qualification as an ordinary account"
    );
    let root = base.0.join("workspace");
    let config = base.0.join("server.json");
    let key = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"; // synthetic 32-byte key only
    std::fs::write(&config,json!({"root":root,"brokerUid":uid,"runtimeProfile":"local-user-macos","workspace":"synthetic","companySlug":"a","companyName":"Synthetic Local","cookieKey":key,"mfaKey":key,"accounts":[]}).to_string()).unwrap();
    std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o600)).unwrap();
    let (mut unsafe_mask, _) = spawn(&config, &web, true, "022");
    assert!(!unsafe_mask.0.wait().unwrap().success());
    assert!(!root.exists());
    drop(unsafe_mask);
    let (mut first, origin) = spawn(&config, &web, true, "077");
    healthy(&mut first, &origin).await;
    for relative in [
        "workspace.json",
        ".rentemester/workspace-control.sqlite",
        "a/data/ledger.sqlite",
    ] {
        let metadata = std::fs::metadata(root.join(relative)).unwrap();
        assert_eq!(metadata.uid(), uid);
        assert_eq!(metadata.mode() & 0o7777, 0o600);
    }
    let (mut contender, _) = spawn(&config, &web, false, "077");
    assert!(!contender.0.wait().unwrap().success());
    drop(contender);
    stop_cleanly(&mut first, "-TERM").await;
    drop(first);
    let (mut reopened, origin) = spawn(&config, &web, false, "077");
    healthy(&mut reopened, &origin).await;
    stop_cleanly(&mut reopened, "-INT").await;
    drop(reopened);
}

#[tokio::test]
async fn sales_head_requests_cannot_enter_mutation_dispatch() {
    let token = rentemester_runtime::auth::new_token().unwrap();
    let appdata = PathBuf::from(std::env::var_os("HOME").unwrap())
        .join("Library/Application Support")
        .canonicalize()
        .unwrap();
    let base = appdata.join(format!("Rentemester-Synthetic-Head-{}", token.as_str()));
    std::fs::DirBuilder::new().mode(0o700).create(&base).unwrap();
    let base = Synthetic(base);
    let web = base.0.join("web");
    std::fs::create_dir(&web).unwrap();
    std::fs::write(web.join("index.html"), "<!doctype html><title>Synthetic</title>").unwrap();
    let uid = std::fs::metadata(&base.0).unwrap().uid();
    assert_ne!(uid, 0);
    let root = base.0.join("workspace");
    let config = base.0.join("server.json");
    let key = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    std::fs::write(&config, json!({"root":root,"brokerUid":uid,"runtimeProfile":"local-user-macos","workspace":"synthetic","companySlug":"a","companyName":"Synthetic HEAD","cookieKey":key,"mfaKey":key,"accounts":[]}).to_string()).unwrap();
    std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o600)).unwrap();
    let (mut server, origin) = spawn(&config, &web, true, "077");
    healthy(&mut server, &origin).await;
    let ledger = root.join("a/data/ledger.sqlite");
    let counts = || {
        let db = rusqlite::Connection::open_with_flags(&ledger, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        db.query_row("SELECT (SELECT COUNT(*) FROM documents), (SELECT COUNT(*) FROM journal_entries), (SELECT COUNT(*) FROM accounts), (SELECT name FROM companies WHERE id=1)", [], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?, row.get::<_, String>(3)?))).unwrap()
    };
    fn artifacts(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        fn visit(root: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
            for entry in std::fs::read_dir(root).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() { visit(&path, out); }
                else if !path.file_name().unwrap().to_string_lossy().contains(".sqlite") {
                    out.push((path.clone(), std::fs::read(&path).unwrap()));
                }
            }
        }
        let mut out = Vec::new();
        visit(root, &mut out);
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }
    let before = counts();
    let originals = artifacts(&root.join("a"));
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let invoice = r#"{"issueDate":"2026-01-01","buyer":{"name":"Synthetic Buyer","address":"Synthetic Street"},"lines":[{"description":"Synthetic","quantity":1,"unitPriceExVat":100}]}"#;
    for path in ["contacts", "invoices", "invoices/1/pdf", "recurring-invoices", "recurring-invoices/original/list", "accounts", "company"] {
        for body in ["", invoice] {
            let response = client.request(reqwest::Method::HEAD, format!("{origin}/api/companies/a/{path}"))
                .header("origin", &origin)
                .header("content-type", "application/json")
                .header("cookie", "better-auth.session_token=synthetic-invalid")
                .body(body.to_owned()).send().await.unwrap();
            assert_eq!(response.status(), 405, "HEAD {path} must refuse before authentication or invoice dispatch");
            assert_eq!(response.headers()["allow"], "GET");
            assert!(response.bytes().await.unwrap().is_empty());
        }
        assert_eq!(client.get(format!("{origin}/api/companies/a/{path}")).send().await.unwrap().status(), 401);
    }
    assert_eq!(client.post(format!("{origin}/api/companies/a/invoices/issue"))
        .header("origin", &origin).header("content-type", "application/json")
        .body(invoice).send().await.unwrap().status(), 401);
    assert_eq!(counts(), before);
    assert_eq!(artifacts(&root.join("a")), originals);
    stop_cleanly(&mut server, "-TERM").await;
}
