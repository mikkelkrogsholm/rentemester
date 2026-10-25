//! Work-Issue: #717. Compiled stdio original nine-operation proof.
use serde_json::{Value, json};
use std::process::{Command, Stdio};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::{Duration, Instant},
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-mcp");
fn run(origin: &str, messages: Vec<Value>) -> Vec<Value> {
    let cookie = std::env::temp_dir().join(format!(
        "original717-mcp-{}-{:?}.cookie",
        std::process::id(),
        thread::current().id()
    ));
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    o.open(&cookie)
        .unwrap()
        .write_all(b"session=synthetic")
        .unwrap();
    let mut child = Command::new(BIN)
        .args([
            "--server",
            origin,
            "--allow-loopback-http",
            "--cookie-file",
            cookie.to_str().unwrap(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let writer = thread::spawn(move || {
        for m in messages {
            writeln!(stdin, "{m}").unwrap();
        }
    });
    let output = child.wait_with_output().unwrap();
    writer.join().unwrap();
    std::fs::remove_file(cookie).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect()
}

fn init() -> Value {
    json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"synthetic","version":"1"}}})
}
fn call(id: u64, name: &str, args: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":name,"arguments":args}})
}
fn session(calls: Vec<Value>) -> Vec<Value> {
    let mut m = vec![
        init(),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    ];
    m.extend(calls);
    m
}
fn envelope(v: &Value) -> Value {
    let r = &v["result"];
    assert_eq!(r["content"].as_array().unwrap().len(), 1);
    let e: Value = serde_json::from_str(r["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(e, r["structuredContent"]);
    assert_eq!(r["isError"], e["ok"] != true);
    e
}
// Based on registry_original717::fixture: actual committed manifest, bare discovery,
// then authenticated mutation, with a bounded post-response observation window.
fn fixture(
    status: u16,
    text: String,
    truncated: bool,
) -> (String, thread::JoinHandle<Vec<(String, Value)>>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    l.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", l.local_addr().unwrap());
    let t = thread::spawn(move || {
        let mut requests = vec![];
        let mut idle = Instant::now();
        let mut posted = false;
        while idle.elapsed()
            < if posted {
                Duration::from_millis(400)
            } else {
                Duration::from_secs(5)
            }
        {
            let (mut s, _) = match l.accept() {
                Ok(s) => s,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                    continue;
                }
                Err(e) => panic!("{e}"),
            };
            idle = Instant::now();
            s.set_nonblocking(false).unwrap();
            s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut bytes = vec![];
            while !bytes.ends_with(b"\r\n\r\n") {
                let mut b = [0];
                s.read_exact(&mut b).unwrap();
                bytes.push(b[0]);
            }
            let h = String::from_utf8(bytes).unwrap();
            let lower = h.to_lowercase();
            let len = lower
                .lines()
                .find_map(|l| {
                    l.strip_prefix("content-length:")
                        .map(|n| n.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            let mut body = vec![0; len];
            s.read_exact(&mut body).unwrap();
            let line = h.lines().next().unwrap().to_owned();
            let (code, response, cut) = if line == "GET /api/protocol HTTP/1.1" {
                assert!(requests.is_empty());
                assert!(!lower.contains("cookie:") && !lower.contains("authorization:"));
                requests.push((line, Value::Null));
                (
                    200,
                    include_str!("../../../protocol/http-v1.json").to_owned(),
                    false,
                )
            } else {
                assert!(line.starts_with("POST "), "no readback: {line}");
                assert!(lower.contains("cookie: session=synthetic"));
                assert!(lower.contains("x-rentemester-manifest:"));
                requests.push((line, serde_json::from_slice(&body).unwrap()));
                posted = true;
                (status, text.clone(), truncated)
            };
            write!(s,"HTTP/1.1 {code} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()+if cut {30} else {0}).unwrap();
        }
        requests
    });
    (origin, t)
}
fn arguments(name: &str) -> Value {
    match name {
        "system_restore_backup" => {
            json!({"backupDir":"/synthetic/backup","targetCompany":"/synthetic/target","confirmText":"RESTORE /synthetic/target","confirm":true})
        }
        "system_backup_place" => {
            json!({"company":"synthetic","archivePath":"/synthetic/a.tar","destinationId":"cold","confirm":true})
        }
        "system_backup_verify_remote_placement" => {
            json!({"company":"synthetic","destinationId":"cold","backupId":"b1","remoteObjectId":"object1","confirm":true})
        }
        "mail_intake_ingest" => {
            json!({"company":"synthetic","source":"/synthetic/nonexistent.eml","confirm":true})
        }
        "efaktura_konfigurer" => {
            json!({"company":"synthetic","apiLicenseKey":"synthetic-private-secret","confirm":true})
        }
        _ => json!({"company":"synthetic","confirm":true}),
    }
}
fn exercise(name: &str, core: Value) {
    let mut a = arguments(name);
    if name == "system_restore_backup" {
        let out = run(
            "http://127.0.0.1:9",
            session(vec![call(2, name, a.clone())]),
        );
        assert_eq!(envelope(&out[1])["code"], "invalid_input");
        a.as_object_mut().unwrap().remove("confirmText");
        let out = run(
            "http://127.0.0.1:9",
            session(vec![call(2, name, a.clone())]),
        );
        assert_eq!(envelope(&out[1])["code"], "CONFIRMTEXT_MISMATCH");
        return;
    }
    let file = std::env::temp_dir().join(format!(
        "mcp717-mail-{}-{:?}.eml",
        std::process::id(),
        thread::current().id()
    ));
    if name == "mail_intake_ingest" {
        std::fs::write(&file, b"Message-ID: <synthetic>\r\n\r\nsynthetic").unwrap();
        a["source"] = json!(file.to_str().unwrap());
    }
    for (status, response, expected) in [
        (200, json!({"ok":true,"result":core}), "success"),
        (
            200,
            json!({"ok":true,"result":{"ok":false,"errors":["synthetic refusal"]}}),
            "domain",
        ),
        (200, json!({"ok":true,"result":{"ok":true}}), "protocol"),
        (403, json!({"ok":false}), "server_refused"),
        (503, json!({"ok":false}), "outcome_unknown"),
    ] {
        let (origin, worker) = fixture(status, response.to_string(), false);
        let out = run(&origin, session(vec![call(2, name, a.clone())]));
        let e = envelope(&out[1]);
        assert_eq!(e["ok"], expected == "success");
        if expected == "success" {
            assert!(e["data"].is_object());
            assert_eq!(e["errors"], json!([]));
        } else if expected == "domain" {
            assert_eq!(e["errors"], json!(["synthetic refusal"]));
            assert!(e.get("data").is_none());
        } else {
            assert_eq!(e["code"], expected);
        }
        assert!(!out[1].to_string().contains("synthetic-private-secret"));
        let requests = worker.join().unwrap();
        assert_eq!(requests.len(), 2, "no automatic retry/readback");
        let body = &requests[1].1;
        assert!(body.get("company").is_none());
        assert_eq!(body["confirm"], true);
        assert!(body.get("idempotencyKey").is_none());
        if name == "mail_intake_ingest" {
            assert!(body.get("source").is_none());
            assert_eq!(body["messages"].as_array().unwrap().len(), 1);
        }
    }
    for field in ["confirm", "actor", "_meta"] {
        let mut bad = a.clone();
        bad[field] = json!("forged");
        let out = run("http://127.0.0.1:9", session(vec![call(2, name, bad)]));
        assert_eq!(out[1]["result"]["isError"], true);
        assert!(out[1]["result"].get("structuredContent").is_none());
    }
    a.as_object_mut().unwrap().remove("confirm");
    let out = run("http://127.0.0.1:9", session(vec![call(2, name, a)]));
    assert_eq!(envelope(&out[1])["code"], "CONFIRM_REQUIRED");
    if name == "mail_intake_ingest" {
        std::fs::remove_file(file).unwrap();
    }
}
#[test]
fn original717_system_backup() {
    exercise(
        "system_backup",
        json!({"ok":true,"backupId":"b1","backupDir":"backups/b1","manifestPath":"backups/b1/manifest.json","dbSnapshotPath":"backups/b1/ledger.sqlite","errors":[]}),
    );
}
#[test]
fn original717_system_backup_archive() {
    exercise(
        "system_backup_archive",
        json!({"ok":true,"backupId":"b1","archivePath":"b1.tar","archiveSha256":"a".repeat(64),"sha256Path":"b1.tar.sha256","archiveSizeBytes":1024,"errors":[]}),
    );
}
#[test]
fn original717_system_backup_place() {
    exercise(
        "system_backup_place",
        json!({"ok":true,"placement":{"backupId":"b1","actorKind":"agent"},"copiedTo":"cold/b1.tar","errors":[]}),
    );
}
#[test]
fn original717_system_backup_verify_remote_placement() {
    exercise(
        "system_backup_verify_remote_placement",
        json!({"ok":true,"placement":{"backupId":"b1","verified":true},"errors":[]}),
    );
}
#[test]
fn original717_system_restore_backup() {
    exercise("system_restore_backup", json!({}));
}
#[test]
fn original717_mail_intake_ingest() {
    exercise(
        "mail_intake_ingest",
        json!({"ok":true,"messagesProcessed":1,"attachmentsIngested":1,"attachmentsSkipped":0,"exceptionsCreated":0,"documents":[]}),
    );
}
#[test]
fn original717_imap_intake_poll() {
    exercise(
        "imap_intake_poll",
        json!({"ok":true,"messagesFetched":1,"messagesProcessed":1,"attachmentsIngested":1,"attachmentsSkipped":0,"exceptionsCreated":0,"documents":[]}),
    );
}
#[test]
fn original717_invoice_send_email() {
    exercise(
        "invoice_send_email",
        json!({"ok":true,"duplicate":false,"invoiceNumber":"S1","kind":"invoice","subject":"Synthetic invoice","recipient":"synthetic@example.invalid","messageId":"synthetic","errors":[]}),
    );
}
#[test]
fn original717_efaktura_konfigurer() {
    exercise(
        "efaktura_konfigurer",
        json!({"ok":true,"configPath":"config/digisense.json","environment":"test"}),
    );
}
#[test]
fn original717_catalogue_matches_frozen_source_metadata() {
    let out = Command::new(BIN).arg("--catalogue").output().unwrap();
    assert!(out.status.success());
    let catalogue: Value = serde_json::from_slice(&out.stdout).unwrap();
    let baseline: Value = serde_json::from_str(include_str!(
        "../../../docs/architecture/rust-target/implementation/baseline/inventory.json"
    ))
    .unwrap();
    let tools = catalogue["tools"].as_array().unwrap();
    for name in [
        "system_backup",
        "system_backup_archive",
        "system_backup_place",
        "system_backup_verify_remote_placement",
        "system_restore_backup",
        "mail_intake_ingest",
        "imap_intake_poll",
        "invoice_send_email",
        "efaktura_konfigurer",
    ] {
        let expected = baseline["mcp"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == name)
            .unwrap();
        let actual = tools
            .iter()
            .filter(|t| t["name"] == name)
            .collect::<Vec<_>>();
        assert_eq!(actual.len(), 1);
        for key in [
            "name",
            "title",
            "description",
            "annotations",
            "inputSchema",
            "outputSchema",
        ] {
            assert_eq!(actual[0][key], expected[key]);
        }
    }
}
