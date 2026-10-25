use common::TcpListener;
#[path = "../../rentemester-client/tests/common/mod.rs"]
mod common;
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    process::Command,
    sync::mpsc,
    thread,
    time::Duration,
};
fn cli(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_rentemester-rust"))
        .args(args)
        .env_remove("RENTEMESTER_COMPANY")
        .env_remove("RENTEMESTER_WORKSPACE")
        .output()
        .unwrap()
}
fn server(
    status: &'static str,
    reply: String,
    mime: &'static str,
) -> (String, mpsc::Receiver<Vec<u8>>, thread::JoinHandle<usize>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", l.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();
    let t = thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        let mut bytes = vec![];
        loop {
            let mut b = [0; 4096];
            let n = s.read(&mut b).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&b[..n]);
            if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let h = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                let len = h
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length: "))
                    .map(|n| n.parse::<usize>().unwrap())
                    .unwrap_or(0);
                if bytes.len() >= end + 4 + len {
                    break;
                }
            }
        }
        tx.send(bytes).unwrap();
        write!(
            s,
            "HTTP/1.1 {status}\r\nContent-Type:{mime}\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{reply}",
            reply.len()
        )
        .unwrap();
        drop(s);
        let l = l.into_inner();
        l.set_nonblocking(true).unwrap();
        thread::sleep(Duration::from_millis(60));
        1 + l.incoming().take_while(Result::is_ok).count()
    });
    (origin, rx, t)
}

#[test]
fn six_offline_discovery_and_usage_before_credentials() {
    for op in ["plan", "persist", "dry-run", "approve", "apply", "status"] {
        for discovery in ["--help", "--example"] {
            let o = cli(&[
                "--cookie-file=/missing/cookie",
                "--local-profile=/missing/profile",
                "bookkeeping-batch",
                op,
                discovery,
            ]);
            assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
            assert!(String::from_utf8_lossy(&o.stdout).contains("expected-plan-hash"));
        }
    }
    for flags in [
        vec!["persist"],
        vec!["dry-run", "--confirm=yes"],
        vec!["approve", "--run-id=7", "--plan-hash=bad", "--confirm=yes"],
        vec!["apply", "--run-id=7", "--plan-hash=bad", "--confirm"],
        vec!["status", "--run-id=0"],
        vec!["status", "--run-id=7", "--run-id=8"],
        vec!["status", "--run-id=7", "--run-key=hidden"],
        vec!["status", "--run-id=7", "--actor=user:forged"],
        vec!["plan", "--input={}"],
        vec!["plan", "--company-id=0"],
        vec!["status", "--run-id=7", "--", "--json"],
    ] {
        let mut a = vec!["bookkeeping-batch"];
        a.extend(flags);
        a.extend([
            "--cookie-file=/missing/cookie",
            "--local-profile=/missing/profile",
        ]);
        let o = cli(&a);
        assert_eq!(
            o.status.code(),
            Some(2),
            "{}",
            String::from_utf8_lossy(&o.stdout)
        );
        assert!(!String::from_utf8_lossy(&o.stderr).contains("Could not validate local profile"));
    }
}
#[test]
fn exact_confirmed_persist_flat_projection_and_pipe_format() {
    let hash = "a".repeat(64);
    let plan = json!({"planHash":hash,"items":[],"executionPolicy":"atomic_v1"});
    let state = json!({"run":{"runId":7,"planHash":hash,"plan":plan.to_string()},"revisions":[{"plannerActor":"system:legacy"}],"attempts":[],"receipts":[],"finalChecks":[],"legacyEvents":[],"legacyFinalChecks":[]});
    let reply =
        json!({"ok":true,"dryRun":true,"runId":7,"duplicate":false,"plan":plan,"state":state});
    let (origin, rx, t) = server("200 OK", reply.to_string(), "application/json");
    let dir = std::env::temp_dir().join(format!("original-batch-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cookie = dir.join("cookie");
    std::fs::write(&cookie, "session=synthetic").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&cookie, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let o = cli(&[
        "--server",
        &origin,
        "bookkeeping-batch",
        "--company=synthetic",
        "persist",
        "--cookie-file",
        cookie.to_str().unwrap(),
        "--allow-loopback-http",
        "--accounting-from=2026-01-01",
        "--accounting-to=2026-12-31",
        "--bank-from=2026-01-01",
        "--bank-to=2026-12-31",
        "--run-key= exact key=one ",
        "--expected-plan-hash",
        &hash,
        "--confirm=yes",
        "--format=human",
        "--json",
    ]);
    assert!(
        o.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    );
    let got: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert!(got.get("dryRun").is_none());
    assert_eq!(got["state"], state);
    let bytes = rx.recv_timeout(Duration::from_secs(3)).unwrap();
    let end = bytes.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
    let body: Value = serde_json::from_slice(&bytes[end..]).unwrap();
    assert_eq!(body["runKey"], " exact key=one ");
    assert_eq!(body["expectedPlanHash"], hash);
    assert!(body.get("actor").is_none());
    assert!(body.get("companyId").is_none());
    assert_eq!(t.join().unwrap(), 1);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn actor_assertion_reads_current_user_and_never_forwards_forgery() {
    let (origin, rx, t) = server(
        "200 OK",
        json!({"ok":true,"user":{"id":"actual-human"},"companies":[]}).to_string(),
        "application/json",
    );
    let dir = std::env::temp_dir().join(format!("original-batch-actor-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cookie = dir.join("cookie");
    std::fs::write(&cookie, "session=synthetic").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&cookie, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let hash = "a".repeat(64);
    let o = cli(&[
        "bookkeeping-batch",
        "approve",
        "--run-id=7",
        "--plan-hash",
        &hash,
        "--confirm=yes",
        "--actor=user:forged",
        "--server",
        &origin,
        "--company=synthetic",
        "--cookie-file",
        cookie.to_str().unwrap(),
        "--allow-loopback-http",
        "--json",
    ]);
    assert_eq!(o.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&o.stderr).contains("actor differs"));
    let wire = rx.recv_timeout(Duration::from_secs(3)).unwrap();
    assert!(wire.starts_with(b"GET /api/me HTTP/1.1"));
    assert_eq!(t.join().unwrap(), 1);
    std::fs::remove_dir_all(dir).unwrap();
}
