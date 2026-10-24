#[path = "../../rentemester-client/tests/common/mod.rs"]
mod common;
use serde_json::{Value,json};
use std::{io::{Read,Write},process::{Command,Output,Stdio},thread,time::{Duration,Instant}};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
fn call(args: &[&str]) -> Output {
    Command::new(BIN).args(args).env_remove("RENTEMESTER_COMPANY").env_remove("RENTEMESTER_WORKSPACE").stdin(Stdio::null()).output().unwrap()
}
fn report() -> Value {
    json!({"ok":true,"audit":{"format":"rentemester-native-audit-verification/1","slug":"synthetic","entries":7,"available":true,"ok":true,"errors":[],"components":(["ledger","bank","invoices","originals","physical_schema"].map(|name|json!({"name":name,"available":true,"ok":true,"checked":{},"errors":[]})))}})
}
fn failed(available: bool) -> Value {
    let mut v=report();
    v["audit"]["ok"]=json!(false); v["audit"]["available"]=json!(available);
    v["audit"]["errors"]=json!(["originals: synthetic missing original"]);
    v["audit"]["components"][3]["ok"]=json!(false);
    v["audit"]["components"][3]["available"]=json!(available);
    v["audit"]["components"][3]["errors"]=json!([{"error":"synthetic missing original"}]);v
}
fn observe(status: u16, body: Value, human: bool, native: bool, attached: bool) -> Output {
    let listener=common::TcpListener::bind("127.0.0.1:0").unwrap();
    let origin=format!("http://{}",listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let task=thread::spawn(move|| {
        let start=Instant::now();let mut count=0;let mut last=None;
        loop {
            if last.is_some_and(|t:Instant|t.elapsed()>Duration::from_millis(150)) {break;}
            assert!(start.elapsed()<Duration::from_secs(8),"request deadline");
            let (mut s,_)=match listener.accept(){Ok(s)=>s,Err(e) if e.kind()==std::io::ErrorKind::WouldBlock=>{thread::sleep(Duration::from_millis(5));continue},Err(e)=>panic!("{e}")};
            count+=1;let mut bytes=Vec::new();
            while !bytes.ends_with(b"\r\n\r\n") {let mut b=[0];s.read_exact(&mut b).unwrap();bytes.push(b[0]);}
            let request=String::from_utf8(bytes).unwrap();
            assert!(request.starts_with("GET /api/companies/synthetic/audit-verify HTTP/1.1\r\n"),"{request}");
            assert!(request.to_lowercase().contains("cookie: session=synthetic"));
            let body=body.to_string();write!(s,"HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();last=Some(Instant::now());
        }
        assert_eq!(count,1,"no retry or extra dispatch");
    });
    let path=std::env::temp_dir().join(format!("rm-audit716-{}-{}",std::process::id(),origin.rsplit(':').next().unwrap()));
    let mut options=std::fs::OpenOptions::new();options.write(true).create_new(true);
    #[cfg(unix)] { use std::os::unix::fs::OpenOptionsExt;options.mode(0o600); }
    options.open(&path).unwrap().write_all(b"session=synthetic").unwrap();
    let cookie=path.to_str().unwrap();
    let out=if native {call(&["remote","audit-verify","--server",&origin,"--company","synthetic","--cookie-file",cookie,"--allow-loopback-http"])}
    else if attached {call(&["--json","--format=human",&format!("--server={origin}"),"audit",&format!("--cookie-file={cookie}"),"verify","--company=synthetic","--allow-loopback-http"])}
    else {call(&["audit","verify","--server",&origin,"--company","synthetic","--cookie-file",cookie,"--allow-loopback-http","--format",if human{"human"}else{"json"}])};
    task.join().unwrap();std::fs::remove_file(path).unwrap();out
}
#[test]
fn discovery_and_usage_are_offline() {
    for args in [vec!["audit","--help"],vec!["audit","verify","--help"],vec!["--company=synthetic","--cookie-file=/missing","audit","verify","--help"],vec!["--help"]] {
        let o=call(&args);assert_eq!(o.status.code(),Some(0));assert!(o.stderr.is_empty());assert!(String::from_utf8_lossy(&o.stdout).contains("audit verify"));
    }
    for flags in [vec!["--actor=agent:forged"],vec!["--confirm=yes"],vec!["--unknown=x"],vec!["--company=../wrong"],vec!["--server=http://example.invalid"],vec!["--json=false"],vec!["--format=csv"],vec!["--company"],vec!["--body={}"],vec!["--local-profile=/missing","--server=https://example.invalid"]] {
        let mut args=vec!["audit","verify","--cookie-file=/missing"];args.extend(flags);
        let o=call(&args);assert_eq!(o.status.code(),Some(2),"{args:?}");assert!(o.stdout.is_empty());assert!(!o.stderr.is_empty());assert!(!String::from_utf8_lossy(&o.stderr).contains("/missing"));
    }
}
#[test]
fn clean_and_authoritative_findings_are_flat_with_source_streams() {
    for attached in [false,true] {
        let o=observe(200,report(),false,false,attached);assert_eq!(o.status.code(),Some(0));assert!(o.stderr.is_empty());
        assert_eq!(serde_json::from_slice::<Value>(&o.stdout).unwrap(),json!({"ok":true,"entries":7,"errors":[]}));
    }
    let o=observe(200,failed(true),false,false,false);assert_eq!(o.status.code(),Some(1));assert!(o.stderr.is_empty());
    assert_eq!(serde_json::from_slice::<Value>(&o.stdout).unwrap(),json!({"ok":false,"entries":7,"errors":["originals: synthetic missing original"]}));
    let o=observe(200,report(),true,false,false);assert_eq!(o.status.code(),Some(0));assert_eq!(String::from_utf8(o.stdout).unwrap(),"✔ Verificerer audit-kæde og bogføringsintegritet.\n  Posteringer kontrolleret: 7\n");assert!(o.stderr.is_empty());
    let o=observe(200,failed(true),true,false,false);assert_eq!(o.status.code(),Some(1));assert!(o.stdout.is_empty());assert_eq!(String::from_utf8(o.stderr).unwrap(),"✘ Verificerer audit-kæde og bogføringsintegritet. failed\n  → originals: synthetic missing original\n");
}
#[test]
fn unavailable_malformed_false_green_and_auth_fail_closed_redacted() {
    let mut green=failed(true);green["audit"]["ok"]=json!(true);
    let mut wrong=report();wrong["audit"]["slug"]=json!("wrong");
    for (status,v) in [(200,failed(false)),(200,green),(200,wrong),(200,json!({"secret":"RAW_SECRET"})),(401,json!({"errors":["RAW_SECRET"]})),(403,json!({"errors":["RAW_SECRET"]}))] {
        let o=observe(status,v,false,false,false);assert_eq!(o.status.code(),Some(1));assert!(o.stderr.is_empty());let v:Value=serde_json::from_slice(&o.stdout).unwrap();
        assert_eq!(v["ok"],false);assert!(v.get("entries").is_none());assert!(v.get("schema").is_none());assert_eq!(v.as_object().unwrap().len(),2);assert!(!String::from_utf8_lossy(&o.stdout).contains("RAW_SECRET"));
    }
    let o=call(&["audit","verify","--server=http://127.0.0.1:9","--allow-loopback-http","--company=synthetic","--cookie-file=/missing","--json"]);
    assert_eq!(o.status.code(),Some(1));assert_eq!(serde_json::from_slice::<Value>(&o.stdout).unwrap()["ok"],false);assert!(o.stderr.is_empty());
}
#[test]
fn native_observation_contract_is_unchanged() {
    let o=observe(200,failed(true),false,true,false);assert_eq!(o.status.code(),Some(0));let v:Value=serde_json::from_slice(&o.stdout).unwrap();assert_eq!(v["ok"],true);assert_eq!(v["data"]["ok"],true);assert_eq!(v["data"]["audit"]["ok"],false);
}
#[test]
fn protocol_refusal_never_sends_credentials_or_business_request() {
    let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let origin=format!("http://{}",listener.local_addr().unwrap());
    let task=thread::spawn(move|| {
        let (mut s,_)=listener.accept().unwrap();s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut bytes=Vec::new();while !bytes.ends_with(b"\r\n\r\n") {let mut b=[0];s.read_exact(&mut b).unwrap();bytes.push(b[0]);}
        let wire=String::from_utf8(bytes).unwrap().to_lowercase();assert!(wire.starts_with("get /api/protocol "));assert!(!wire.contains("cookie:"));assert!(!wire.contains("authorization:"));
        let body="{\"secret\":\"RAW_SECRET\"}";write!(s,"HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();drop(s);
        listener.set_nonblocking(true).unwrap();thread::sleep(Duration::from_millis(150));assert!(listener.accept().is_err());
    });
    let path=std::env::temp_dir().join(format!("rm-audit716-protocol-{}",std::process::id()));
    let mut options=std::fs::OpenOptions::new();options.write(true).create_new(true);
    #[cfg(unix)] {use std::os::unix::fs::OpenOptionsExt;options.mode(0o600);}
    options.open(&path).unwrap().write_all(b"session=synthetic").unwrap();
    let o=call(&["audit","verify","--company=synthetic","--server",&origin,"--allow-loopback-http","--cookie-file",path.to_str().unwrap()]);
    task.join().unwrap();std::fs::remove_file(path).unwrap();assert_eq!(o.status.code(),Some(1));assert!(o.stderr.is_empty());assert!(!String::from_utf8_lossy(&o.stdout).contains("RAW_SECRET"));
}
#[cfg(unix)]
#[test]
fn verified_local_company_path_uses_unix_not_origin_tcp() {
    use std::{fs,os::unix::{fs::{DirBuilderExt,PermissionsExt},net::UnixListener}};
    let home=fs::canonicalize(std::env::var_os("HOME").unwrap()).unwrap();
    let path=home.join(format!(".rm-audit716-{}",std::process::id()));
    fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
    fs::DirBuilder::new().mode(0o700).create(path.join("workspace")).unwrap();
    fs::create_dir(path.join("workspace/synthetic")).unwrap();
    let tcp=std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let origin=format!("http://{}",tcp.local_addr().unwrap());tcp.set_nonblocking(true).unwrap();
    let listener=UnixListener::bind(path.join("r.sock")).unwrap();fs::set_permissions(path.join("r.sock"),fs::Permissions::from_mode(0o600)).unwrap();
    fs::write(path.join("r.json"),json!({"format":"rentemester-local-runtime/1","origin":origin,"socket":"r.sock","workspaceRoot":path.join("workspace"),"workspaceId":"synthetic"}).to_string()).unwrap();
    fs::set_permissions(path.join("r.json"),fs::Permissions::from_mode(0o600)).unwrap();
    fs::write(path.join("cookie"),"session=synthetic").unwrap();fs::set_permissions(path.join("cookie"),fs::Permissions::from_mode(0o600)).unwrap();
    let task=thread::spawn(move|| {
        let (mut s,_)=common::unix_accept(&listener).unwrap();s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();let mut bytes=Vec::new();
        while !bytes.ends_with(b"\r\n\r\n") {let mut b=[0];s.read_exact(&mut b).unwrap();bytes.push(b[0]);}
        let request=String::from_utf8(bytes).unwrap();assert!(request.starts_with("GET /api/companies/synthetic/audit-verify HTTP/1.1"));assert!(request.to_lowercase().contains("cookie: session=synthetic"));
        let body=report().to_string();write!(s,"HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
    });
    let o=call(&["--local-profile",path.to_str().unwrap(),"audit","verify","--company",path.join("workspace/synthetic").to_str().unwrap(),"--cookie-file",path.join("cookie").to_str().unwrap()]);
    task.join().unwrap();assert_eq!(o.status.code(),Some(0),"{}",String::from_utf8_lossy(&o.stderr));assert_eq!(serde_json::from_slice::<Value>(&o.stdout).unwrap(),json!({"ok":true,"entries":7,"errors":[]}));assert!(tcp.accept().is_err());fs::remove_dir_all(path).unwrap();
}
