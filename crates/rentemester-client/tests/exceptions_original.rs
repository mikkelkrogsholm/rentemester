use rentemester_client::{ApiClient,Error};
use serde_json::{Value,json};
use std::{io::{Read,Write},net::TcpListener,sync::{Arc,atomic::{AtomicUsize,Ordering}},thread,time::{Duration,Instant}};
fn row() -> Value {
    json!({"id":9,"type":"bank_unmatched","severity":"warning","status":"open","relatedBankTransactionId":4,"relatedDocumentId":null,"message":"Synthetic review task","requiredAction":"Review evidence","sourceEvidence":{"bankTransactionId":4},"postingPreview":null,"createdAt":"2025-04-01 10:00:00","resolvedAt":null,"resolvedBy":null,"resolutionNote":null,"archived":false})
}
fn server(status: u16, response: String, target: &'static str, body: Option<Vec<u8>>) -> (ApiClient,Arc<AtomicUsize>,thread::JoinHandle<()>) {
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
            assert!(headers.to_lowercase().contains("cookie: session=synthetic"));
            let length = headers.lines().find_map(|l| l.to_lowercase().strip_prefix("content-length:").map(|s| s.trim().parse::<usize>().unwrap())).unwrap_or(0);
            while bytes.len() < end+length { let mut chunk = [0;4096]; let n = s.read(&mut chunk).unwrap(); assert!(n>0); bytes.extend_from_slice(&chunk[..n]); }
            assert_eq!(&bytes[end..],body.as_deref().unwrap_or_default());
            write!(s,"HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()).unwrap();
            last = Some(Instant::now());
        }
    });
    (ApiClient::new(&origin,true,Some("session=synthetic".into())).unwrap(),count,handle)
}
fn once(count: Arc<AtomicUsize>,handle: thread::JoinHandle<()>) { handle.join().unwrap(); assert_eq!(count.load(Ordering::SeqCst),1); }
#[tokio::test]
async fn exception_original_complete_flat_rows_and_business_status() {
    let value = json!({"ok":true,"count":1,"rows":[row()],"errors":[]});
    let (api,count,t) = server(200,value.to_string(),"GET /api/companies/synthetic/exceptions/original?includeArchived=false HTTP/1.1",None);
    assert_eq!(api.exceptions_original_list("synthetic",None,false).await.unwrap(),value); once(count,t);
    let value = json!({"ok":false,"count":0,"rows":[],"errors":["status must be open, resolved, or all when present"]});
    let (api,count,t) = server(200,value.to_string(),"GET /api/companies/synthetic/exceptions/original?includeArchived=false&status=bad HTTP/1.1",None);
    assert_eq!(api.exceptions_original_list("synthetic",Some("bad"),false).await.unwrap(),value); once(count,t);
}
#[tokio::test]
async fn exception_original_resolve_known_errors_and_no_retry_unknown() {
    for (status,value,expected) in [
        (200,json!({"ok":true,"resolved":true,"errors":[]}),None),
        (200,json!({"ok":true,"resolved":false,"errors":[]}),None),
        (422,json!({"ok":false,"resolved":false,"code":"NOT_FOUND","errors":["exception 9 does not exist"]}),None),
        (422,json!({"ok":false,"resolved":false,"code":" ","errors":["secret"]}),Some(Error::Refused{status:422})),
        (403,json!({"secret":"private"}),Some(Error::Refused{status:403})),
        (500,json!({"secret":"private"}),Some(Error::OutcomeUnknown)),
        (200,json!({"ok":true,"data":{"resolved":true}}),Some(Error::OutcomeUnknown)),
    ] {
        let (api,count,t) = server(status,value.to_string(),"POST /api/companies/synthetic/exceptions/original/9/resolve HTTP/1.1",Some(b"{}".to_vec()));
        let result = api.exceptions_original_resolve("synthetic",9,b"{}".to_vec(),true).await;
        if let Some(e) = expected { assert_eq!(result.unwrap_err(),e); } else { assert_eq!(result.unwrap(),value); }
        once(count,t);
    }
    let (api,count,t) = server(200,"{".into(),"POST /api/companies/synthetic/exceptions/original/9/resolve HTTP/1.1",Some(b"{}".to_vec()));
    assert_eq!(api.exceptions_original_resolve("synthetic",9,b"{}".to_vec(),true).await.unwrap_err(),Error::OutcomeUnknown); once(count,t);
}
#[tokio::test]
async fn exception_original_strict_rows_and_inputs() {
    for bad in [json!({}),{ let mut r = row(); r["archived"] = json!(true); r },{ let mut r = row(); r["id"] = json!(0); r }] {
        let (api,count,t) = server(200,json!({"ok":true,"count":1,"rows":[bad],"errors":[]}).to_string(),"GET /api/companies/synthetic/exceptions/original?includeArchived=false HTTP/1.1",None);
        assert_eq!(api.exceptions_original_list("synthetic",None,false).await.unwrap_err(),Error::Protocol); once(count,t);
    }
    let api = ApiClient::new("http://127.0.0.1:9",true,None).unwrap();
    for body in [json!({"note":null}),json!({"actor":"caller"}),json!({"confirm":true}),json!([])] {
        assert!(matches!(api.exceptions_original_resolve("synthetic",1,body.to_string().into_bytes(),true).await,Err(Error::InvalidInput(_))));
    }
    assert!(matches!(api.exceptions_original_resolve("synthetic",1,b"{}".to_vec(),false).await,Err(Error::InvalidInput(_))));
}
