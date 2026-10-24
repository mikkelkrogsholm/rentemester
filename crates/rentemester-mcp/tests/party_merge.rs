use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::{Duration, Instant},
};
const HASH: &str = "d157b0f801b28bbb6a6fe392fe76d5dcdfd0e2e17e7c50b8c5494d9ef7e195dd";
// Passive standalone manifest with the two frozen additions and canonical manifestId.
const MANIFEST: &str = r#"{"format":"rentemester-http-protocol/1","protocolMajor":1,"wireProfiles":["legacy-react-v1","rust-http-v1"],"operations":[{"method":"POST","path":"/api/companies/{slug}/workspace-parties/{id}/approve-merge","capability":"POST /api/companies/{slug}/workspace-parties/{id}/approve-merge"},{"method":"POST","path":"/api/companies/{slug}/workspace-parties/{id}/propose-merge","capability":"POST /api/companies/{slug}/workspace-parties/{id}/propose-merge"},{"method":"GET","path":"/api/me","capability":"GET /api/me"}],"manifestId":"f0a51ed7fa95da80416d27bf8ddbd6720d2a77e65417f786e514c12673a0b319"}"#;
fn party() -> Value {
    json!({"partyId":"source","kind":"organization","name":"Synthetic","source":"fixture","observedAt":"2026-01-01T00:00:00Z","reviewAssertion":"Reviewed","identifiers":[],"aliases":[],"assertions":[],"roles":[],"history":[
 {"event_type":"created","payload_hash":"a".repeat(64),"actor":"user:synthetic","created_at":"2026-01-01T00:00:00Z"},
 {"event_type":"proposed_merge","payload_hash":HASH,"actor":"user:synthetic","created_at":"2026-01-01T00:00:00Z"},
 {"event_type":"approved_merge","payload_hash":HASH,"actor":"user:synthetic","created_at":"2026-01-01T00:00:00Z"},
 {"event_type":"superseded","payload_hash":HASH,"actor":"user:synthetic","created_at":"2026-01-01T00:00:00Z"}]})
}
fn reply(op: &str) -> Value {
    if op == "propose" {
        json!({"ok":true,"proposalHash":HASH})
    } else {
        json!({"ok":true,"party":party()})
    }
}
fn body(op: &str) -> Value {
    if op == "propose" {
        json!({"intoPartyId":"target","reviewAssertion":"Reviewed\nsynthetic","confirm":true})
    } else {
        json!({"proposalHash":HASH,"confirm":true})
    }
}
fn fixture(
    response: Value,
    status: u16,
    protocol: bool,
) -> (String, thread::JoinHandle<Vec<(String, Value)>>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", l.local_addr().unwrap());
    l.set_nonblocking(true).unwrap();
    let worker = thread::spawn(move || {
        let mut writes = Vec::new();
        let mut idle = Instant::now();
        while idle.elapsed() < Duration::from_secs(5) {
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
            let mut bytes = Vec::new();
            while !bytes.ends_with(b"\r\n\r\n") {
                let mut b = [0];
                s.read_exact(&mut b).unwrap();
                bytes.push(b[0]);
            }
            let header = String::from_utf8(bytes.clone()).unwrap();
            let lower = header.to_lowercase();
            let n = lower
                .lines()
                .find_map(|l| {
                    l.strip_prefix("content-length:")
                        .map(|n| n.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            let mut data = vec![0; n];
            s.read_exact(&mut data).unwrap();
            let (code, text) = if header.starts_with("GET /api/protocol ") {
                assert!(
                    !lower.contains("cookie:")
                        && !lower.contains("authorization:")
                        && !lower.contains("x-rentemester-protocol:")
                );
                (
                    200,
                    if protocol {
                        MANIFEST.to_owned()
                    } else {
                        "{}".into()
                    },
                )
            } else {
                assert!(lower.contains("cookie: session=synthetic"));
                assert!(lower.contains("x-rentemester-manifest:"));
                assert!(
                    !lower.contains("authorization:")
                        && !lower.contains("x-rentemester-service-key:")
                );
                if header.starts_with("GET /api/me ") {
                    (
                        200,
                        json!({"ok":true,"user":{"id":"synthetic"}}).to_string(),
                    )
                } else {
                    writes.push((
                        header.lines().next().unwrap().into(),
                        serde_json::from_slice(&data).unwrap(),
                    ));
                    (status, response.to_string())
                }
            };
            write!(s,"HTTP/1.1 {code} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",text.len()).unwrap();
        }
        writes
    });
    (origin, worker)
}
use std::process::{Command, Stdio};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-mcp");
fn run(origin: &str, messages: Vec<Value>) -> Vec<Value> {
    let cookie = std::env::temp_dir().join(format!(
        "merge688-mcp-{}-{:?}.cookie",
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
fn messages(op: &str, args: Value) -> Vec<Value> {
    vec![
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"synthetic","version":"1"}}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":format!("{op}_workspace_party_merge"),"arguments":args}}),
    ]
}
fn arguments(op: &str) -> Value {
    json!({"company":"synthetic","partyId":"source","body":body(op).to_string(),"confirm":true})
}
#[test]
fn actual_stdio_discovery_two_writes_exact_cookie_and_protocol() {
    for op in ["propose", "approve"] {
        let (origin, server) = fixture(reply(op), 200, true);
        let responses = run(&origin, messages(op, arguments(op)));
        let list = responses.iter().find(|v| v["id"] == 2).unwrap();
        for name in [
            "propose_workspace_party_merge",
            "approve_workspace_party_merge",
        ] {
            let tool = list["result"]["tools"]
                .as_array()
                .unwrap()
                .iter()
                .find(|t| t["name"] == name)
                .unwrap();
            assert_eq!(tool["inputSchema"]["properties"]["confirm"]["const"], true);
        }
        let result = responses.iter().find(|v| v["id"] == 3).unwrap();
        assert_ne!(result["result"]["isError"], true, "{result}");
        let writes = server.join().unwrap();
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].1, body(op));
        assert!(writes[0].0.contains(&format!("/{op}-merge ")));
    }
}
#[test]
fn strict_outer_inner_confirmation_authority_and_no_retry() {
    for op in ["propose", "approve"] {
        for change in ["outer", "inner", "actor"] {
            let mut args = arguments(op);
            match change {
                "outer" => args["confirm"] = json!(false),
                "inner" => {
                    let mut b = body(op);
                    b["confirm"] = json!(false);
                    args["body"] = json!(b.to_string());
                }
                _ => args["actor"] = json!("user:caller"),
            }
            let (origin, server) = fixture(reply(op), 200, true);
            let responses = run(&origin, messages(op, args));
            let result = responses.iter().find(|v| v["id"] == 3).unwrap();
            assert!(
                result.get("error").is_some() || result["result"]["isError"] == true,
                "{result}"
            );
            assert!(server.join().unwrap().is_empty());
        }
        for protocol in [false, true] {
            let (origin, server) = fixture(reply(op), 500, protocol);
            let responses = run(&origin, messages(op, arguments(op)));
            let result = responses.iter().find(|v| v["id"] == 3).unwrap();
            assert_eq!(result["result"]["isError"], true, "{result}");
            assert_eq!(server.join().unwrap().len(), usize::from(protocol));
        }
    }
}
