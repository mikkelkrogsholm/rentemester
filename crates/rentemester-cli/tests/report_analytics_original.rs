#![cfg(unix)]
//! Original argv/output over a private Unix runtime, using the existing protocol fixture.
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    os::unix::{
        fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
        net::UnixListener,
    },
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
    thread,
    time::{Duration, Instant},
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
static SEQ: AtomicUsize = AtomicUsize::new(0);
struct Fixture {
    root: PathBuf,
    listener: UnixListener,
}
impl Fixture {
    fn new() -> Self {
        let root = fs::canonicalize(std::env::var_os("HOME").unwrap())
            .unwrap()
            .join(format!(
                ".rm110-{}-{}",
                std::process::id(),
                SEQ.fetch_add(1, Ordering::Relaxed)
            ));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        fs::DirBuilder::new()
            .mode(0o700)
            .create(root.join("workspace"))
            .unwrap();
        let listener = UnixListener::bind(root.join("r.sock")).unwrap();
        fs::set_permissions(root.join("r.sock"), fs::Permissions::from_mode(0o600)).unwrap();
        let d = json!({"format":"rentemester-local-runtime/1","origin":"http://127.0.0.1:9","socket":"r.sock","workspaceRoot":root.join("workspace"),"workspaceId":"synthetic-workspace"});
        for (name, text) in [
            ("r.json", d.to_string()),
            ("cookie", "session=synthetic".into()),
        ] {
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(root.join(name))
                .unwrap()
                .write_all(text.as_bytes())
                .unwrap();
        }
        listener.set_nonblocking(true).unwrap();
        Self { root, listener }
    }
    fn command(&self) -> Command {
        let mut c = Command::new(BIN);
        // Globals before command deliberately exercise original argv dispatch.
        c.args([
            "--cookie-file",
            self.root.join("cookie").to_str().unwrap(),
            "--local-profile",
            self.root.to_str().unwrap(),
            "report",
            "analytics",
            "--workspace",
            "synthetic-workspace",
            "--from",
            "2026-01-01",
            "--to",
            "2026-06-30",
        ]);
        c
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}
fn accept(l: &UnixListener) -> std::os::unix::net::UnixStream {
    let start = Instant::now();
    loop {
        match l.accept() {
            Ok((s, _)) => {
                s.set_nonblocking(false).unwrap();
                s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                return s;
            }
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    && start.elapsed() < Duration::from_secs(5) =>
            {
                thread::sleep(Duration::from_millis(5))
            }
            e => panic!("expected one bounded request: {e:?}"),
        }
    }
}
fn reply(s: &mut (impl Read + Write), value: &Value, status: u16) -> String {
    let mut b = vec![];
    while !b.ends_with(b"\r\n\r\n") {
        let mut buf = [0; 4096];
        let n = s.read(&mut buf).unwrap();
        assert!(n > 0);
        b.extend_from_slice(&buf[..n]);
    }
    let request = String::from_utf8(b).unwrap();
    assert!(!request.starts_with("POST "));
    let body = value.to_string();
    write!(
        s,
        "HTTP/1.1 {status} Test\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
    request
}
fn run(flags: &[&str], body: Value, status: u16) -> (Output, String) {
    let f = Fixture::new();
    let l = f.listener.try_clone().unwrap();
    let t = thread::spawn(move || {
        let mut s = accept(&l);
        let manifest: Value =
            serde_json::from_str(include_str!("../../../protocol/http-v1.json")).unwrap();
        let r = reply(&mut s, &manifest, 200);
        assert!(r.starts_with("GET /api/protocol HTTP/1.1"));
        assert!(!r.to_lowercase().contains("cookie:"));
        let mut s = accept(&l);
        let r = reply(&mut s, &body, status);
        assert!(r.to_lowercase().contains("cookie: session=synthetic"));
        assert!(r.to_lowercase().contains("x-rentemester-manifest:"));
        r
    });
    let out = f.command().args(flags).output().unwrap();
    let request = t.join().unwrap();
    assert!(
        matches!(f.listener.accept(),Err(e) if e.kind()==std::io::ErrorKind::WouldBlock),
        "no extra business read or retry"
    );
    (out, request)
}
fn canonical(scope: &str) -> Value {
    json!({"ok":true,"schemaVersion":"rentemester-cfo-analytics-v1","scope":scope,"status":"partial","rows":[{"amount":"9007199254740993.01","sourceHash":"exact-source","sourceId":"synthetic-journal"}],"page":{"nextCursor":"opaque/+"},"reconciliation":{"status":"blocked","errors":["missing original"]},"future":{"exact":true}})
}
#[test]
fn company_exact_projection_filters_and_json_precedence() {
    let body = canonical("company");
    let (o, r) = run(
        &[
            "--scope=company",
            "--company-slug",
            " synthetic ",
            "--company-slugs",
            "ignored",
            "--group-profile-id",
            "ignored",
            "--account",
            " 1000 ",
            "--party",
            " p ",
            "--dimension",
            " x:y ",
            "--currency",
            " dkk ",
            "--aggregate",
            "sum",
            "--cursor",
            "opaque/+",
            "--limit",
            "1e2",
            "--json",
            "--format",
            "human",
        ],
        body.clone(),
        200,
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(o.stderr.is_empty());
    let mut analytics = body;
    analytics.as_object_mut().unwrap().remove("ok");
    assert_eq!(
        serde_json::from_slice::<Value>(&o.stdout).unwrap(),
        json!({"ok":true,"analytics":analytics})
    );
    assert!(r.starts_with("GET /api/cfo-analytics?scope=company&from=2026-01-01&to=2026-06-30&companySlug=synthetic&account=1000&party=p&dimension=x%3Ay&currency=DKK&aggregate=sum&cursor=opaque%2F%2B&limit=100 HTTP/1.1"),"{r}");
}
#[test]
fn portfolio_group_selection_and_source_human_rendering() {
    for scope in ["portfolio", "group"] {
        let mut body = canonical(scope);
        if scope == "group" {
            body["group"] = json!({"status":"blocked","errors":["unapproved"]});
        }
        let (o, r) = run(
            &[
                "--scope",
                scope,
                "--company-slug",
                "ignored",
                "--company-slugs",
                " z, a,z,, ",
                "--group-profile-id",
                "profile",
                "--limit",
                "0x10",
                "--format",
                "json",
                "--format=human",
            ],
            body,
            200,
        );
        assert!(o.status.success());
        assert_eq!(o.stdout, "✔ report-analytics\n".as_bytes());
        assert!(o.stderr.is_empty());
        let suffix = if scope == "group" {
            "&groupProfileId=profile"
        } else {
            ""
        };
        assert!(r.starts_with(&format!("GET /api/cfo-analytics?scope={scope}&from=2026-01-01&to=2026-06-30&companySlug=a&companySlug=z{suffix}&limit=16 HTTP/1.1")),"{r}");
    }
}
#[test]
fn known_denial_and_malformed_success_are_flat_failure_not_empty_success() {
    let denial = json!({"ok":false,"code":"forbidden","errors":["workspace visibility denied"]});
    let (o, _) = run(
        &["--scope", "company", "--company-slug", "a", "--json"],
        denial.clone(),
        403,
    );
    assert_eq!(o.status.code(), Some(1));
    assert!(o.stderr.is_empty());
    assert_eq!(
        serde_json::from_slice::<Value>(&o.stdout).unwrap(),
        json!({"ok":false,"errors":["workspace visibility denied"]})
    );
    let (o, _) = run(
        &[
            "--scope",
            "company",
            "--company-slug",
            "a",
            "--format",
            "human",
        ],
        denial,
        403,
    );
    assert_eq!(o.status.code(), Some(1));
    assert!(o.stdout.is_empty());
    assert_eq!(
        o.stderr,
        "✘ report-analytics failed\n  → workspace visibility denied\n".as_bytes()
    );
    let mut wrong = canonical("portfolio");
    wrong["ok"] = json!(true);
    let (o, _) = run(&["--scope", "company", "--company-slug", "a"], wrong, 200);
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(
        serde_json::from_slice::<Value>(&o.stdout).unwrap()["ok"],
        false
    );
}
#[test]
fn parse_help_and_verified_workspace_refuse_before_any_transport() {
    let o = Command::new(BIN)
        .args([
            "--cookie-file",
            "/never-opened",
            "report",
            "analytics",
            "--help",
        ])
        .output()
        .unwrap();
    assert!(o.status.success());
    assert!(String::from_utf8_lossy(&o.stdout).contains("verified"));
    for flags in [
        vec![],
        vec!["--scope", "invalid"],
        vec!["--scope", "company", "--limit", "0"],
        vec!["--scope", "group", "--limit", "1.1"],
        vec!["--scope", "group", "--aggregate", "average"],
        vec!["--scope", "portfolio", "--confirm"],
    ] {
        let o = Command::new(BIN)
            .args([
                "report",
                "analytics",
                "--workspace",
                "synthetic-workspace",
                "--from",
                "2026-01-01",
                "--to",
                "2026-06-30",
            ])
            .args(flags)
            .output()
            .unwrap();
        assert_eq!(o.status.code(), Some(2));
        assert!(o.stdout.is_empty());
    }
    let f = Fixture::new();
    let o = f
        .command()
        .args([
            "--scope",
            "company",
            "--company-slug",
            "a",
            "--workspace=other",
        ])
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(2)); // duplicate grammar, no I/O
    let mut c = f.command();
    // Rebuild only the assertion value without introducing a second workspace flag.
    let args: Vec<_> = c
        .get_args()
        .map(|s| {
            if s == "synthetic-workspace" {
                "other".into()
            } else {
                s.to_os_string()
            }
        })
        .collect();
    c = Command::new(BIN);
    let o = c
        .args(args)
        .args(["--scope", "company", "--company-slug", "a"])
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&o.stdout).contains("workspace differs"));
    assert!(matches!(f.listener.accept(),Err(e) if e.kind()==std::io::ErrorKind::WouldBlock));
    let o = Command::new(BIN)
        .args([
            "report",
            "analytics",
            "--workspace",
            "synthetic-workspace",
            "--scope",
            "company",
            "--from",
            "2026-01-01",
            "--to",
            "2026-06-30",
            "--server",
            "http://127.0.0.1:9",
            "--allow-loopback-http",
        ])
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&o.stdout).contains("verified --local-profile"));
}
