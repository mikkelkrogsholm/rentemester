//! Compiled original CLI lifecycle, production protocol admission and native producer receipts.
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::{Duration, Instant},
};
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
                        include_str!("../../../protocol/http-v1.json").to_owned()
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
                        if data.is_empty() {
                            Value::Null
                        } else {
                            serde_json::from_slice(&data).unwrap()
                        },
                    ));
                    if response == "lost" {
                        continue;
                    }
                    (status, response.to_string())
                }
            };
            write!(s,"HTTP/1.1 {code} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",text.len()).unwrap();
        }
        writes
    });
    (origin, worker)
}
use std::process::{Command, Output, Stdio};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
fn run(args: &[String], stdin: Option<&str>) -> Output {
    let mut child = Command::new(BIN)
        .args(args)
        .env("RENTEMESTER_COMPANY", "must-not-inherit")
        .env("USER", "must-not-inherit")
        .env_remove("RENTEMESTER_WORKSPACE")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(text) = stdin {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(text.as_bytes())
            .unwrap();
    } else {
        drop(child.stdin.take());
    }
    child.wait_with_output().unwrap()
}
fn cookie() -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!(
        "merge688-{}-{:?}.cookie",
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
    o.open(&p).unwrap().write_all(b"session=synthetic").unwrap();
    p
}

fn identity() -> Value {
    json!({"partyId":"synthetic-party","kind":"organization","name":"Synthetic","source":"fixture","observedAt":"2026-01-01T00:00:00Z","reviewAssertion":"Reviewed","aliases":["Alias","ALIAS"],"identifiers":[]})
}
fn party(linked: bool) -> Value {
    let mut p = identity();
    p["observedAt"] = json!("2026-01-01T00:00:00.000Z");
    p["aliases"] = json!([{"alias":"alias","source":"fixture","observed_at":"2026-01-01T00:00:00.000Z","review_state":"approved","payload_hash":"a".repeat(64)}]);
    p["assertions"] = json!([{"field":"name","value":"Synthetic","source":"fixture","observed_at":"2026-01-01T00:00:00.000Z","review_state":"approved","payload_hash":"b".repeat(64)}]);
    p["roles"] = if linked {
        json!([{"companySlug":"synthetic","role":"supplier","defaults":{}}])
    } else {
        json!([])
    };
    p["history"] = json!([{"event_type":"created","payload_hash":"c".repeat(64),"actor":"user:synthetic","created_at":"2026-01-01T00:00:00.000Z"}]);
    if linked {
        p["history"].as_array_mut().unwrap().push(json!({"event_type":"linked","payload_hash":"d".repeat(64),"actor":"user:synthetic","created_at":"2026-01-01T00:00:00.000Z"}));
    }
    p
}
fn observe(
    op: &str,
    response: Value,
    extra: &[&str],
    protocol: bool,
) -> (Output, Vec<(String, Value)>) {
    let (origin, worker) = fixture(response, 200, protocol);
    let cookie = cookie();
    let input = cookie.with_extension("json");
    std::fs::write(&input, identity().to_string()).unwrap();
    let mut a = vec![
        "party".to_owned(),
        op.into(),
        "--format=human".into(),
        "--server".into(),
        origin,
        "--company=synthetic".into(),
        "--allow-loopback-http".into(),
        "--cookie-file".into(),
        cookie.to_str().unwrap().into(),
    ];
    if op == "create" {
        a.extend([
            "--input".into(),
            input.to_str().unwrap().into(),
            "--confirm=yes".into(),
        ]);
    }
    if matches!(op, "inspect" | "link-role") {
        a.push("--party-id=synthetic-party".into());
    }
    if op == "link-role" {
        a.extend(["--role=supplier".into(), "--confirm=yes".into()]);
    }
    a.extend(extra.iter().map(|s| s.to_string()));
    let o = run(&a, None);
    let requests = worker.join().unwrap();
    std::fs::remove_file(cookie).unwrap();
    std::fs::remove_file(input).unwrap();
    (o, requests)
}
#[test]
fn lifecycle_flat_receipts_and_explicit_role() {
    for (op, response, path) in [
        (
            "create",
            json!({"ok":true,"party":party(false)}),
            "POST /api/companies/synthetic/workspace-parties/create-identity HTTP/1.1",
        ),
        (
            "inspect",
            json!({"ok":true,"party":party(false)}),
            "GET /api/companies/synthetic/workspace-parties/synthetic-party HTTP/1.1",
        ),
        (
            "link-role",
            json!({"ok":true,"party":party(true)}),
            "POST /api/companies/synthetic/workspace-parties/synthetic-party/role HTTP/1.1",
        ),
        (
            "search",
            json!({"ok":true,"rows":[],"count":0,"nextCursor":null}),
            "GET /api/companies/synthetic/workspace-parties?cursor=0&limit=25 HTTP/1.1",
        ),
    ] {
        let (o, requests) = observe(op, response.clone(), &["--json"], true);
        assert_eq!(
            o.status.code(),
            Some(0),
            "{op}: {} {}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        );
        assert!(o.stderr.is_empty());
        assert_eq!(
            serde_json::from_slice::<Value>(&o.stdout).unwrap(),
            response
        );
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].0, path);
        if op == "create" {
            let mut expected = identity();
            expected["confirm"] = json!(true);
            assert_eq!(requests[0].1, expected);
        }
        if op == "link-role" {
            assert_eq!(requests[0].1, json!({"role":"supplier","confirm":true}));
        }
    }
    let mut p = party(true);
    p["history"]
        .as_array_mut()
        .unwrap()
        .retain(|e| e["event_type"] != "linked");
    let (o, r) = observe(
        "search",
        json!({"ok":true,"rows":[p],"count":1,"nextCursor":null}),
        &[],
        true,
    );
    assert_eq!(o.status.code(), Some(0));
    assert!(o.stderr.is_empty());
    assert_eq!(
        String::from_utf8(o.stdout).unwrap(),
        "✔ Søger deterministisk blandt parter synlige i den angivne…\n  Antal: 1\n"
    );
    assert_eq!(r.len(), 1);
}
#[test]
fn refusal_unknown_and_no_retry() {
    for response in [
        json!({"ok":true,"party":party(true)}),
        json!("lost"),
        json!({"ok":true}),
    ] {
        let (o, r) = observe("create", response, &["--json"], true);
        assert_eq!(o.status.code(), Some(1));
        assert!(o.stderr.is_empty());
        let e: Value = serde_json::from_slice(&o.stdout).unwrap();
        assert_eq!(e["code"], "outcome_unknown");
        assert_eq!(r.len(), 1);
    }
    let (o, r) = observe("create", json!({}), &["--json"], false);
    assert_eq!(o.status.code(), Some(1));
    assert!(r.is_empty());
    assert_eq!(
        serde_json::from_slice::<Value>(&o.stdout).unwrap()["code"],
        "protocol"
    );
    let (o, r) = observe(
        "inspect",
        json!({"ok":false,"errors":["party not found"]}),
        &[],
        true,
    );
    assert_eq!(o.status.code(), Some(1));
    assert!(o.stdout.is_empty());
    assert!(String::from_utf8_lossy(&o.stderr).contains("party not found"));
    assert_eq!(r.len(), 1);
}
#[test]
fn strict_flags_and_actor_no_mutation() {
    for extra in [
        vec!["--unknown"],
        vec!["--confirm=no"],
        vec!["--workspace=/tmp"],
        vec!["--company=other"],
        vec!["--actor=user:other"],
    ] {
        let (o, r) = observe("create", json!({}), &extra, true);
        assert_eq!(o.status.code(), Some(2), "{:?}", extra);
        assert!(o.stdout.is_empty());
        assert!(r.is_empty());
    }
    for a in [
        vec!["party", "create", "--input=missing"],
        vec!["party", "inspect", "--party-id=.."],
        vec!["party", "search", "--limit=101"],
    ] {
        let o = run(&a.iter().map(|s| s.to_string()).collect::<Vec<_>>(), None);
        assert_eq!(o.status.code(), Some(2));
    }
    let o = run(&["party".into(), "--help".into()], None);
    assert_eq!(o.status.code(), Some(0));
    let h: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(h["commands"].as_array().unwrap().len(), 6);
    let o = run(&["--help".into()], None);
    let h: Value = serde_json::from_slice(&o.stdout).unwrap();
    for n in [
        "party propose-merge|approve-merge",
        "party create|search|inspect|link-role",
    ] {
        assert!(
            h["commands"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["name"] == n)
        );
    }
}
