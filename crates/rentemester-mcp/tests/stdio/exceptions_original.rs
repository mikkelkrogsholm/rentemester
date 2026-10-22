use super::*;
use std::{sync::Arc,sync::atomic::AtomicUsize,time::{Duration,Instant}};
fn server(status: u16, response: String, target: &'static str, body: Option<Vec<u8>>) -> (String,Arc<AtomicUsize>,thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}",listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let count = Arc::new(AtomicUsize::new(0)); let seen = count.clone();
    let handle = thread::spawn(move || {
        let start = Instant::now(); let mut last: Option<Instant> = None;
        loop {
            if let Some(t) = last { if t.elapsed() >= Duration::from_millis(150) { break; } }
            else { assert!(start.elapsed() < Duration::from_secs(5),"startup deadline exceeded"); }
            let (mut s,_) = match listener.accept() { Ok(v) => v, Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => { thread::sleep(Duration::from_millis(5)); continue; }, Err(e) => panic!("{e}") };
            seen.fetch_add(1,Ordering::SeqCst);
            s.set_nonblocking(false).unwrap(); s.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
            let mut bytes = Vec::new();
            let end = loop { let mut chunk = [0;4096]; let n = s.read(&mut chunk).unwrap(); assert!(n>0); bytes.extend_from_slice(&chunk[..n]); if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") { break i+4; } };
            let headers = String::from_utf8(bytes[..end].to_vec()).unwrap();
            assert!(headers.starts_with(target),"{headers}");
            assert!(headers.to_lowercase().contains("cookie: session=never-print-this"));
            let length = headers.lines().find_map(|l| l.to_lowercase().strip_prefix("content-length:").map(|s| s.trim().parse::<usize>().unwrap())).unwrap_or(0);
            while bytes.len() < end+length { let mut chunk = [0;4096]; let n = s.read(&mut chunk).unwrap(); assert!(n>0); bytes.extend_from_slice(&chunk[..n]); }
            assert_eq!(&bytes[end..],body.as_deref().unwrap_or_default());
            write!(s,"HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()).unwrap();
            last = Some(Instant::now());
        }
    });
    (origin,count,handle)
}
fn once(count: Arc<AtomicUsize>,handle: thread::JoinHandle<()>) { handle.join().unwrap(); assert_eq!(count.load(Ordering::SeqCst),1); }


#[test]
fn exception_original_mcp_confirm_and_invalid_args_before_http() {
    let rows = parsed(&run("http://127.0.0.1:9",&[
        init("2025-11-25"),ready(),
        call(2,"exception_resolve",json!({"company":"synthetic","id":9})),
        call(3,"exception_resolve",json!({"company":"synthetic","id":9,"confirm":false})),
        call(4,"exception_resolve",json!({"company":"synthetic","id":9,"confirm":true,"note":null})),
        call(5,"exception_resolve",json!({"company":"synthetic","id":9,"confirm":"true"})),
        call(6,"exception_resolve",json!({"company":"synthetic","id":9,"confirm":true,"authority":"reviewer"})),
    ]));
    for row in &rows[1..3] {
        assert_eq!(row["result"]["structuredContent"],json!({"ok":false,"code":"CONFIRM_REQUIRED","errors":["confirm: true required for write tool exception_resolve"]}));
    }
    for row in &rows[3..] { assert_eq!(row["error"]["code"],-32602); }
}
#[test]
fn exception_original_mcp_flat_transport_source_envelope_and_no_retry() {
    for (name,args,target,body,status,value,envelope) in [
        ("exceptions_list",json!({"company":"synthetic"}),"GET /api/companies/synthetic/exceptions/original?includeArchived=false HTTP/1.1",None,200,json!({"ok":true,"count":0,"rows":[],"errors":[]}),json!({"ok":true,"data":{"count":0,"rows":[]},"errors":[]})),
        ("exception_resolve",json!({"company":"synthetic","id":9,"confirm":true,"note":"review"}),"POST /api/companies/synthetic/exceptions/original/9/resolve HTTP/1.1",Some(br#"{"note":"review"}"#.to_vec()),200,json!({"ok":true,"resolved":false,"errors":[]}),json!({"ok":true,"data":{"resolved":false},"errors":[]})),
        ("exception_resolve",json!({"company":"synthetic","id":9,"confirm":true}),"POST /api/companies/synthetic/exceptions/original/9/resolve HTTP/1.1",Some(b"{}".to_vec()),422,json!({"ok":false,"resolved":false,"code":"NOT_FOUND","errors":["exception 9 does not exist"],"appliedRules":["REVIEW"]}),json!({"ok":false,"code":"NOT_FOUND","errors":["exception 9 does not exist"],"appliedRules":["REVIEW"]})),
    ] {
        let (origin,count,t) = server(status,value.to_string(),target,body);
        let rows = parsed(&run(&origin,&[init("2025-11-25"),ready(),call(2,name,args)]));
        assert_eq!(rows[1]["result"]["structuredContent"],envelope);
        assert_eq!(serde_json::from_str::<Value>(rows[1]["result"]["content"][0]["text"].as_str().unwrap()).unwrap(),envelope);
        assert_eq!(rows[1]["result"]["isError"],envelope["ok"] != true);
        once(count,t);
    }
}
