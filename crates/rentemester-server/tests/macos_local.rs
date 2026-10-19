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
    drop(first);
    let (mut reopened, origin) = spawn(&config, &web, false, "077");
    healthy(&mut reopened, &origin).await;
    drop(reopened);
}
