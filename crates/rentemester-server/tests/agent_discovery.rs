use serde_json::Value;
use std::{
    net::TcpListener,
    process::{Child, Command, Stdio},
    time::Duration,
};
struct Web(std::path::PathBuf);
impl Drop for Web {
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

#[tokio::test]
async fn public_discovery_without_workspace_is_inert_and_never_live() {
    let web = Web(std::env::temp_dir().join(format!(
        "rm-discovery-{}",
        rentemester_runtime::auth::new_token().unwrap().as_str()
    )));
    std::fs::create_dir(&web.0).unwrap();
    std::fs::write(web.0.join("index.html"), "synthetic").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    let mut server = Server(
        Command::new(env!("CARGO_BIN_EXE_rentemester-server"))
            .args([
                "--listen",
                &addr.to_string(),
                "--web-dir",
                web.0.to_str().unwrap(),
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let base = format!("http://{addr}");
    for _ in 0..100 {
        assert!(server.0.try_wait().unwrap().is_none());
        if client
            .get(format!("{base}/api/agent-about"))
            .send()
            .await
            .is_ok()
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let response = client
        .get(format!("{base}/api/agent-about"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let about: Value = serde_json::from_slice(&response.bytes().await.unwrap()).unwrap();
    assert_eq!(about["workspaceConnected"], false);
    let response = client
        .get(format!(
            "{base}/api/agent-workflows/supplier-payable-handling"
        ))
        .send()
        .await
        .unwrap();
    let workflow: Value = serde_json::from_slice(&response.bytes().await.unwrap()).unwrap();
    assert_eq!(workflow["workflow"]["live"], false);
    assert!(
        !workflow["workflow"]["unresolvedOperations"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    for path in [
        "agent-capabilities?limit=0",
        "agent-capabilities?limit=1&limit=2",
        "agent-capabilities?cursor=-1",
        "agent-capabilities?extra=x",
        "agent-about?extra=x",
    ] {
        assert_eq!(
            client
                .get(format!("{base}/api/{path}"))
                .send()
                .await
                .unwrap()
                .status(),
            400,
            "{path}"
        );
    }
    assert_eq!(
        client
            .get(format!("{base}/api/agent-workflows/unknown"))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    assert_eq!(
        client
            .post(format!("{base}/api/agent-capabilities"))
            .send()
            .await
            .unwrap()
            .status(),
        405
    );
}
