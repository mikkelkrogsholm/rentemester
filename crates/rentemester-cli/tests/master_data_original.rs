#[path = "../../rentemester-client/tests/common/mod.rs"]
mod common;
use common::TcpListener;
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
fn server(
    status: u16,
    response: String,
    target: &'static str,
    body: Option<Vec<u8>>,
) -> (String, Arc<AtomicUsize>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let seen = count.clone();
    let handle = thread::spawn(move || {
        let start = Instant::now();
        let mut last: Option<Instant> = None;
        loop {
            if let Some(t) = last {
                if t.elapsed() >= Duration::from_millis(150) {
                    break;
                }
            } else {
                assert!(
                    start.elapsed() < Duration::from_secs(5),
                    "startup deadline exceeded"
                );
            }
            let (mut s, _) = match listener.accept() {
                Ok(v) => v,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                    continue;
                }
                Err(e) => panic!("{e}"),
            };
            seen.fetch_add(1, Ordering::SeqCst);
            s.set_nonblocking(false).unwrap();
            s.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
            let mut bytes = Vec::new();
            let end = loop {
                let mut chunk = [0; 4096];
                let n = s.read(&mut chunk).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&chunk[..n]);
                if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    break i + 4;
                }
            };
            let headers = String::from_utf8(bytes[..end].to_vec()).unwrap();
            assert!(headers.starts_with(target), "{headers}");
            assert!(headers.to_lowercase().contains("cookie: session=synthetic"));
            let length = headers
                .lines()
                .find_map(|l| {
                    l.to_lowercase()
                        .strip_prefix("content-length:")
                        .map(|s| s.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            while bytes.len() < end + length {
                let mut chunk = [0; 4096];
                let n = s.read(&mut chunk).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&chunk[..n]);
            }
            assert_eq!(&bytes[end..], body.as_deref().unwrap_or_default());
            write!(s,"HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()).unwrap();
            last = Some(Instant::now());
        }
    });
    (origin, count, handle)
}
fn once(count: Arc<AtomicUsize>, handle: thread::JoinHandle<()>) {
    handle.join().unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 1);
}

#[test]
fn master_data_original_six_pure_help_and_usage() {
    for (root, cmd) in [
        ("customer", "create"),
        ("customer", "list"),
        ("customer", "validate-vat"),
        ("customer", "cvr-lookup"),
        ("vendor", "create"),
        ("vendor", "list"),
    ] {
        let out = Command::new(BIN)
            .args([
                "--cookie-file=/missing",
                "--company=synthetic",
                root,
                cmd,
                "--help",
            ])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.stderr.is_empty());
    }
    for args in [
        vec!["customer", "list", "--archived=true"],
        vec!["vendor", "create", "--confirm"],
        vec!["customer", "create", "--actor=caller"],
        vec!["customer", "list", "--unknown=x"],
        vec!["customer", "validate-vat"],
        vec!["customer", "cvr-lookup"],
    ] {
        let out = Command::new(BIN).args(args).output().unwrap();
        assert_eq!(out.status.code(), Some(2));
        assert!(out.stdout.is_empty());
        assert!(!out.stderr.is_empty());
    }
}
#[test]
fn master_data_original_actual_exits_global_positions_pipe_human_json_precedence() {
    let path =
        std::env::temp_dir().join(format!("rm164-master-data-cookie-{}", std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(&path)
        .unwrap()
        .write_all(b"session=synthetic")
        .unwrap();
    for (case,(args,target,body,value,status,exit,human)) in [
        (vec!["customer","list"],"GET /api/companies/synthetic/master-data/original/customers?archived=false HTTP/1.1",None,json!({"ok":true,"count":0,"rows":[],"errors":[]}),200,0,None),
        (vec!["vendor","list","--archived","--format=human"],"GET /api/companies/synthetic/master-data/original/vendors?archived=true HTTP/1.1",None,json!({"ok":true,"count":0,"rows":[],"errors":[]}),200,0,Some("Leverandører (0)\nIngen leverandører registreret.\n")),
        (vec!["customer","list","--json","--format=human"],"GET /api/companies/synthetic/master-data/original/customers?archived=false HTTP/1.1",None,json!({"ok":true,"count":0,"rows":[],"errors":[]}),200,0,None),
        (vec!["customer","create","--name=Synthetic","--payment-terms=NaN"],"POST /api/companies/synthetic/master-data/original/customers HTTP/1.1",Some(br#"{"confirm":true,"input":{"name":"Synthetic"}}"#.to_vec()),json!({"ok":true,"customerId":1,"appliedRules":["DK-MASTER-DATA-CUSTOMER-001"],"errors":[]}),200,0,None),
        (vec!["customer","create","--from-cvr=55555555"],"POST /api/companies/synthetic/master-data/original/customers HTTP/1.1",Some(br#"{"confirm":true,"fromCvr":"55555555","input":{}}"#.to_vec()),json!({"ok":false,"errors":["actual provider failure"]}),200,1,None),
        (vec!["vendor","create","--name=Synthetic"],"POST /api/companies/synthetic/master-data/original/vendors HTTP/1.1",Some(br#"{"confirm":true,"input":{"name":"Synthetic"}}"#.to_vec()),json!({"ok":false,"code":"REFUSED","errors":["actual account refusal"]}),422,1,None),
    ].into_iter().enumerate() {
        let (origin,count,t)=server(status,value.to_string(),target,body);
        let global=["--company=synthetic","--server",&origin,"--cookie-file",path.to_str().unwrap(),"--allow-loopback-http"];
        let mut invocation: Vec<&str>=args.clone();
        let position=match case%3 {0=>0,1=>1,_=>2};
        invocation.splice(position..position,global);
        let out=Command::new(BIN).args(invocation).output().unwrap();
        assert_eq!(out.status.code(),Some(exit),"{}",String::from_utf8_lossy(&out.stderr));assert!(out.stderr.is_empty());
        if let Some(s)=human {assert_eq!(String::from_utf8_lossy(&out.stdout),s);} else {assert_eq!(serde_json::from_slice::<Value>(&out.stdout).unwrap(),value);}
        once(count,t);
    }
    for n in ["0", "-1", "1.5"] {
        let out = Command::new(BIN)
            .args([
                "customer",
                "create",
                "--company=synthetic",
                "--server=http://127.0.0.1:9",
                "--cookie-file",
                path.to_str().unwrap(),
                "--allow-loopback-http",
                "--json",
                &format!("--payment-terms={n}"),
            ])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(1));
        assert_eq!(
            serde_json::from_slice::<Value>(&out.stdout).unwrap()["errors"][0],
            "paymentTermsDays must be a positive integer"
        );
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn master_data_original_sdk_schema_usage_is_stderr_before_http() {
    let cookie = std::env::temp_dir().join(format!("rm164-schema-cookie-{}", std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.create_new(true).write(true);
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
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let out = Command::new(BIN)
        .args([
            "vendor",
            "create",
            "--name",
            "Synthetic",
            "--identifier-kind=unknown",
            "--json",
            "--server",
            &origin,
            "--allow-loopback-http",
            "--company",
            "synthetic",
            "--cookie-file",
            cookie.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    assert!(!out.stderr.is_empty());
    assert!(matches!(listener.accept(),Err(e) if e.kind()==std::io::ErrorKind::WouldBlock));
    std::fs::remove_file(cookie).unwrap();
}
