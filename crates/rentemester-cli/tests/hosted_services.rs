use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    process::{Command, Stdio},
    thread,
    time::Duration,
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
#[test]
fn discovery_preserves_exact_scopes_confirmation_and_body_schemas() {
    let out = Command::new(BIN)
        .args(["remote", "--help"])
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    for name in [
        "service-principals",
        "create-service-principal",
        "rotate-service-principal",
        "revoke-service-principal",
        "recover-service-principal",
        "hosted-recurring-plan",
        "consent-hosted-recurring",
        "revoke-hosted-recurring",
    ] {
        assert!(v["commands"].as_array().unwrap().contains(&json!(name)));
    }
    assert_eq!(
        v["service_principals"]["bodySchemas"]["recover-service-principal"]["required"],
        json!(["confirm", "operationId"])
    );
    assert_eq!(
        v["hosted_recurring"]["bodySchemas"]["consent-hosted-recurring"]["properties"]["confirm"]["const"],
        true
    );
    for args in [
        vec!["service-principals", "--confirm"],
        vec!["service-principals", "--company", "synthetic"],
        vec!["create-service-principal"],
        vec!["rotate-service-principal", "--confirm", "--id", "7"],
        vec![
            "recover-service-principal",
            "--confirm",
            "--company",
            "synthetic",
        ],
        vec!["hosted-recurring-plan"],
        vec!["hosted-recurring-plan", "--id", "0"],
        vec!["hosted-recurring-plan", "--id", "7", "--confirm"],
        vec!["consent-hosted-recurring"],
        vec!["consent-hosted-recurring", "--confirm", "--id", "7"],
    ] {
        let out = Command::new(BIN).arg("remote").args(args).output().unwrap();
        assert_eq!(out.status.code(), Some(2));
    }
}
#[cfg(unix)]
#[test]
fn exact_one_time_receipt_unknown_hosted_result_and_explicit_recovery_have_no_replay() {
    use std::os::unix::fs::OpenOptionsExt;
    let cookie = std::env::temp_dir().join(format!("rm-hosted-cli-{}.cookie", std::process::id()));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&cookie)
        .unwrap()
        .write_all(b"session=synthetic-private")
        .unwrap();
    for (command, path, body, status, response, expected) in [
        (
            "create-service-principal",
            "/api/workspace/service-principals",
            json!({"displayName":"Synthetic executor","operationId":"12345678-1234-4123-8123-123456789abc","confirm":true}),
            "201 Created",
            r#"{"ok":true,"secret":"synthetic-one-time-output","operationId":"canonical"}"#,
            0,
        ),
        (
            "consent-hosted-recurring",
            "/api/companies/synthetic/scheduled-recurring/hosted-consent",
            json!({"templateId":7,"serviceAccountId":"service-synthetic","credentialId":"credential-synthetic","planHash":"a".repeat(64),"reason":" exact reviewed source ","confirm":true}),
            "503 Unavailable",
            r#"{"private":"must-not-leak-provider-body"}"#,
            1,
        ),
        (
            "recover-service-principal",
            "/api/workspace/service-principals/recover",
            json!({"operationId":"12345678-1234-4123-8123-123456789abc","confirm":true}),
            "200 OK",
            r#"{"ok":true,"status":"completed","recovered":false,"future":{"preserved":true}}"#,
            0,
        ),
    ] {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", l.local_addr().unwrap());
        let bytes = format!(" {body}\n");
        let expected_bytes = bytes.clone();
        let t = thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut wire = vec![];
            loop {
                let mut b = [0; 4096];
                let n = s.read(&mut b).unwrap();
                assert!(n > 0);
                wire.extend_from_slice(&b[..n]);
                if wire.ends_with(expected_bytes.as_bytes()) {
                    break;
                }
            }
            assert!(wire.starts_with(format!("POST {path} HTTP/1.1").as_bytes()));
            write!(
                s,
                "HTTP/1.1 {status}\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{response}",
                response.len()
            )
            .unwrap();
            drop(s);
            l.set_nonblocking(true).unwrap();
            thread::sleep(Duration::from_millis(60));
            assert!(l.accept().is_err());
        });
        let mut args = vec![
            "remote",
            command,
            "--server",
            &origin,
            "--allow-loopback-http",
            "--cookie-file",
            cookie.to_str().unwrap(),
            "--confirm",
        ];
        if command == "consent-hosted-recurring" {
            args.extend(["--company", "synthetic"]);
        }
        let mut child = Command::new(BIN)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(bytes.as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        assert_eq!(out.status.code(), Some(expected));
        let text = String::from_utf8(out.stdout).unwrap();
        assert!(!text.contains("synthetic-private"));
        assert!(!text.contains("must-not-leak-provider-body"));
        assert!(out.stderr.is_empty());
        let value: Value = serde_json::from_str(&text).unwrap();
        if expected == 0 {
            assert_eq!(
                value["data"],
                serde_json::from_str::<Value>(response).unwrap()
            );
        } else {
            assert!(text.contains("outcome_unknown"));
        }
        t.join().unwrap();
    }
    let mut refused = Command::new(BIN)
        .args([
            "remote",
            "create-service-principal",
            "--server",
            "http://127.0.0.1:9",
            "--allow-loopback-http",
            "--cookie-file",
            cookie.to_str().unwrap(),
            "--confirm",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    refused.stdin.take().unwrap().write_all(br#"{"displayName":"Synthetic","secret":"supplied-secret-must-not-echo","confirm":true}"#).unwrap();
    let out = refused.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(!String::from_utf8_lossy(&out.stdout).contains("supplied-secret-must-not-echo"));
    assert!(out.stderr.is_empty());
    std::fs::remove_file(cookie).unwrap();
}
