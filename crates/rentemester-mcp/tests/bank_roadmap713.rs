//! R40 boundary qualification, not an implementation of the pending BYO roadmap.
use serde_json::{Value, json};
use std::io::Write;
use std::net::TcpListener;
use std::process::{Command, Stdio};
fn run(calls: Vec<Value>) -> Vec<Value> {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let cookie = std::env::temp_dir().join(format!(
        "bank713-{}-{:?}.cookie",
        std::process::id(),
        std::thread::current().id()
    ));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(&cookie)
        .unwrap()
        .write_all(b"session=synthetic")
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_rentemester-mcp"))
        .args([
            "--server",
            &format!("http://{}", listener.local_addr().unwrap()),
            "--allow-loopback-http",
            "--cookie-file",
            cookie.to_str().unwrap(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let writer = std::thread::spawn(move || {
        writeln!(input, "{}", json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"synthetic-bank-contract","version":"1"}}})).unwrap();
        writeln!(
            input,
            "{}",
            json!({"jsonrpc":"2.0","method":"notifications/initialized"})
        )
        .unwrap();
        for value in calls {
            writeln!(input, "{value}").unwrap();
        }
    });
    let out = child.wait_with_output().unwrap();
    writer.join().unwrap();
    std::fs::remove_file(cookie).unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stderr.is_empty());
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock,
        "Neither protocol probe nor provider request is allowed"
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(!stdout.contains("synthetic-private-bank-secret"));
    stdout
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect()
}
fn call(id: usize, name: &str, arguments: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":name,"arguments":arguments}})
}
#[test]
fn pending_byo_provider_operations_are_neither_advertised_nor_dispatched() {
    let names = [
        "bank_connection_providers",
        "bank_connection_plan",
        "bank_connection_apply",
        "bank_connection_authorize",
        "bank_connection_status",
        "bank_connection_revoke",
        "bank_sync_prepare",
        "bank_sync_plan",
        "bank_sync_apply",
        "bank_sync_status",
    ];
    let mut calls = vec![json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}})];
    for (i, name) in names.iter().enumerate() {
        calls.push(call(i+3,name,json!({"company":"synthetic","confirm":true,"connectionGeneration":1,"consentGeneration":1,"credentialGeneration":1,"apiKey":"synthetic-private-bank-secret"})));
    }
    let replies = run(calls);
    let tools = replies[1]["result"]["tools"].as_array().unwrap();
    assert!(tools.iter().any(|t| t["name"] == "bank_import"));
    for name in names {
        assert!(!tools.iter().any(|t| t["name"] == name));
    }
    for value in &replies[2..] {
        assert_eq!(
            value["error"],
            json!({"code":-32602,"message":"unknown tool"})
        );
    }
    assert_eq!(replies.len(), 12);
}
#[test]
fn csv_import_never_silently_accepts_provider_generations_or_credentials() {
    let fields = [
        ("provider", json!("synthetic-fake-provider")),
        ("connectionGeneration", json!(1)),
        ("consentGeneration", json!(1)),
        ("credentialGeneration", json!(1)),
        ("preparedPlanHash", json!("synthetic-plan")),
        ("apiKey", json!("synthetic-private-bank-secret")),
    ];
    let calls=fields.into_iter().enumerate().map(|(i,(key,value))|{
        let mut args=json!({"company":"synthetic","csvContent":"date,text,amount\n2026-01-01,Synthetic,1.23\n","confirm":true});args[key]=value;call(i+2,"bank_import",args)
    }).collect();
    let replies = run(calls);
    assert_eq!(replies.len(), 7);
    for value in &replies[1..] {
        assert_eq!(
            value["error"],
            json!({"code":-32602,"message":"unknown bank_import argument"})
        );
    }
}
