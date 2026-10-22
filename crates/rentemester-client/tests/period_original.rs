use rentemester_client::{ApiClient, Error};
use serde_json::{Value,json};
use std::{io::{Read,Write},net::TcpListener,sync::{Arc,atomic::{AtomicUsize,Ordering}},thread,time::{Duration,Instant}};
fn packet(version:u32)->Value {
    let mut v=json!({"version":version,"periodStart":"2025-01-01","periodEnd":"2025-03-31","cutoff":"2025-03-31","controlsRun":["AUDIT_CHAIN"],"items":[{"code":"AUDIT_CHAIN","status":"passed","waivable":false,"count":0,"amount":0,"evidence":[],"sourceHash":"b".repeat(64)}],"blockers":0,"warnings":0,"hash":"a".repeat(64)});
    if version==4 {v["format"]=json!("rentemester.native-period-close.v1");v["sourceRevision"]=json!({"journal":[],"journalCount":[{"n":0}],"audit":[{"n":0,"head":null}]});} v
}
fn review(version:u32)->Value {json!({"id":17,"packet":packet(version),"reviewerActor":"agent:synthetic","reviewerPrincipal":{"kind":"service-account","subjectId":"synthetic-service"},"createdAt":"2025-04-01 10:00:00"})}
fn bytes()->Vec<u8>{serde_json::to_vec(&json!({"periodStart":"2025-01-01","periodEnd":"2025-03-31","confirm":true})).unwrap()}
fn server(status:u16,value:Value,target:&'static str,body:Option<Vec<u8>>)->(ApiClient,Arc<AtomicUsize>,thread::JoinHandle<()>) {
    let listener=TcpListener::bind("127.0.0.1:0").unwrap();let origin=format!("http://{}",listener.local_addr().unwrap());listener.set_nonblocking(true).unwrap();
    let count=Arc::new(AtomicUsize::new(0));let seen=count.clone();
    let task=thread::spawn(move||{
        let start=Instant::now();let mut responded=None;
        loop {
            if responded.is_some_and(|t:Instant|t.elapsed()>Duration::from_millis(150)){break;}
            assert!(start.elapsed()<Duration::from_secs(5),"mock startup deadline exceeded");
            let (mut stream,_)=match listener.accept(){Ok(v)=>v,Err(e) if e.kind()==std::io::ErrorKind::WouldBlock=>{thread::sleep(Duration::from_millis(5));continue;},Err(e)=>panic!("{e}")};
            stream.set_nonblocking(false).unwrap();stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();seen.fetch_add(1,Ordering::SeqCst);
            let mut request=Vec::new();let end=loop {let mut buf=[0;4096];let n=stream.read(&mut buf).unwrap();assert!(n>0);request.extend_from_slice(&buf[..n]);if let Some(i)=request.windows(4).position(|w|w==b"\r\n\r\n"){break i+4;}};
            let headers=String::from_utf8(request[..end].to_vec()).unwrap();assert!(headers.starts_with(target),"{headers}");assert!(headers.to_lowercase().contains("x-rentemester-service-key: synthetic-service"));assert!(!headers.to_lowercase().contains("cookie:"));
            let len=headers.lines().find_map(|line|line.to_lowercase().strip_prefix("content-length:").map(|v|v.trim().parse::<usize>().unwrap())).unwrap_or(0);
            while request.len()<end+len {let mut buf=[0;4096];let n=stream.read(&mut buf).unwrap();assert!(n>0);request.extend_from_slice(&buf[..n]);}
            assert_eq!(&request[end..],body.as_deref().unwrap_or_default());let response=value.to_string();
            write!(stream,"HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()).unwrap();responded=Some(Instant::now());
        }
    });
    (ApiClient::with_service_key(&origin,true,"synthetic-service".into()).unwrap(),count,task)
}
fn once(count:Arc<AtomicUsize>,task:thread::JoinHandle<()>) {task.join().unwrap();assert_eq!(count.load(Ordering::SeqCst),1);}
#[tokio::test]
async fn period_original_mcp_review_computes_without_hash_native4_and_historical3(){
    for version in [4,3] {
        let expected=json!({"ok":true,"review":review(version)});
        let (api,n,t)=server(200,expected.clone(),"POST /api/companies/synthetic/periods/original/review-current HTTP/1.1",Some(bytes()));
        assert_eq!(api.period_mcp_review("synthetic",bytes(),true).await.unwrap(),expected);once(n,t);
        let expected=json!({"ok":true,"packet":packet(version)});
        let (api,n,t)=server(200,expected.clone(),"GET /api/companies/synthetic/periods/original/readiness?from=2025-01-01&to=2025-03-31 HTTP/1.1",None);
        assert_eq!(api.period_mcp_readiness("synthetic","2025-01-01","2025-03-31").await.unwrap(),expected);once(n,t);
    }
}
#[tokio::test]
async fn period_original_mcp_status_null_unavailable_and_list_raw_descending(){
    for expected in [json!({"ok":true,"review":null}),json!({"ok":true,"review":null,"status":"unavailable","code":"PERIOD_CLOSE_REVIEW_SCHEMA_UNAVAILABLE"}),json!({"ok":true,"review":review(4)})] {
        let (api,n,t)=server(200,expected.clone(),"GET /api/companies/synthetic/periods/original/status?reviewId=17 HTTP/1.1",None);
        assert_eq!(api.period_mcp_status("synthetic",17).await.unwrap(),expected);once(n,t);
    }
    let expected=json!({"ok":true,"periods":[{"id":9,"periodStart":"2025-01-01","periodEnd":"2025-03-31","kind":"vat_period","status":"closed","reference":null,"createdAt":"2025-04-01 10:00:00"},{"id":8,"periodStart":"2025-01-01","periodEnd":"2025-03-31","kind":"custom","status":"reported","reference":"receipt","createdAt":"2025-04-01 09:00:00"}],"count":2});
    let (api,n,t)=server(200,expected.clone(),"GET /api/companies/synthetic/periods/original/list HTTP/1.1",None);
    assert_eq!(api.period_mcp_list("synthetic").await.unwrap(),expected);once(n,t);
}
#[tokio::test]
async fn period_original_mcp_close_business_errors_redaction_and_unknown_no_retry(){
    for (status,value,error) in [(422,json!({"ok":false,"code":"STALE","errors":["stale proof"],"appliedRules":["rule"]}),None),(422,json!({"ok":false,"code":" ","errors":["private"]}),Some(Error::Refused{status:422})),(403,json!({"ok":false,"errors":["private"]}),Some(Error::Refused{status:403})),(200,json!({"ok":true}),Some(Error::OutcomeUnknown)),(500,json!({"private":"secret"}),Some(Error::OutcomeUnknown))] {
        let (api,n,t)=server(status,value.clone(),"POST /api/companies/synthetic/periods/original/mcp-close HTTP/1.1",Some(bytes()));
        let actual=api.period_mcp_close("synthetic",bytes(),true).await;
        if let Some(error)=error {assert_eq!(actual.unwrap_err(),error);} else {assert_eq!(actual.unwrap(),value);}once(n,t);
    }
}
#[tokio::test]
async fn period_original_mcp_strict_inputs_and_cli_compatibility(){
    let api=ApiClient::with_service_key("http://127.0.0.1:9",true,"synthetic-service".into()).unwrap();
    assert!(matches!(api.period_original_review("synthetic",bytes(),true).await,Err(Error::InvalidInput(_))));
    for (key,value) in [("actor",json!("owner")),("packetHash",json!("a".repeat(64))),("confirm",json!(false)),("periodStart",json!("2025-02-30"))] {
        let mut body:Value=serde_json::from_slice(&bytes()).unwrap();body[key]=value;
        assert!(matches!(api.period_mcp_review("synthetic",serde_json::to_vec(&body).unwrap(),true).await,Err(Error::InvalidInput(_))));
    }
}
#[tokio::test]
async fn period_original_mcp_close_complete_source_success_with_null_reference(){
    let expected=json!({"ok":true,"periodId":19,"periodStart":"2025-01-01","periodEnd":"2025-03-31","kind":"vat_period","status":"closed","reference":null,"appliedRules":["DK-BOOKKEEPING-PERIOD-LOCK-001"],"errors":[],"readinessPacket":packet(4)});
    let (api,n,t)=server(200,expected.clone(),"POST /api/companies/synthetic/periods/original/mcp-close HTTP/1.1",Some(bytes()));
    assert_eq!(api.period_mcp_close("synthetic",bytes(),true).await.unwrap(),expected);once(n,t);
}
