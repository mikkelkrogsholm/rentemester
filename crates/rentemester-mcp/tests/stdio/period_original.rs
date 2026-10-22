use super::*;
#[test]
fn period_original_five_schemas_annotations_confirmation_before_http_and_native_compatibility(){
    let rows=parsed(&run("http://127.0.0.1:9",&[init("2025-11-25"),ready(),json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),call(3,"period_close_review",json!({"company":"synthetic","from":"2025-01-01","to":"2025-03-31"})),call(4,"period_close",json!({"company":"synthetic","from":"2025-01-01","to":"2025-03-31","confirm":false})),call(5,"period_close",json!({"company":"synthetic","from":"2025-01-01","to":"2025-03-31","confirm":"true"})),call(6,"period_close_review",json!({"company":"synthetic","from":"2025-02-30","to":"2025-03-31"}))]));
    let tools=rows[1]["result"]["tools"].as_array().unwrap();
    for name in ["period_close_readiness","period_close_status","period_list","period_close_review","period_close"] {
        let tool=tools.iter().find(|v|v["name"]==name).unwrap();let read=!matches!(name,"period_close_review"|"period_close");
        assert_eq!(tool["annotations"],json!({"readOnlyHint":read,"destructiveHint":false,"idempotentHint":read,"openWorldHint":false}));
        assert_eq!(tool["inputSchema"]["additionalProperties"],false);
        for key in ["actor","principal","authority","key","idempotencyKey","body"] {assert!(tool["inputSchema"]["properties"].get(key).is_none());}
        assert_eq!(tool["outputSchema"]["required"],json!(["ok","errors"]));
    }
    for (row,name) in [(&rows[2],"period_close_review"),(&rows[3],"period_close")] {
        let expected=json!({"ok":false,"code":"CONFIRM_REQUIRED","errors":[format!("confirm: true required for write tool {name}")]});
        assert_eq!(row["result"]["structuredContent"],expected);assert_eq!(row["result"]["isError"],true);
        assert_eq!(serde_json::from_str::<Value>(row["result"]["content"][0]["text"].as_str().unwrap()).unwrap(),expected);
    }
    assert_eq!(rows[4]["error"]["code"],-32602);assert_eq!(rows[5]["error"]["code"],-32602);
    for name in ["periods","review_period","close_period","reopen_period","report_period"] {assert!(tools.iter().any(|v|v["name"]==name));}
    let close=tools.iter().find(|v|v["name"]=="period_close").unwrap();
    assert_eq!(close["inputSchema"]["properties"]["kind"]["enum"],json!(["vat_period","vat_quarter","fiscal_year","custom"]));
    assert_eq!(close["inputSchema"]["properties"]["reviewId"]["maximum"],9007199254740991_u64);
}
#[test]
fn period_original_review_body_mapping_and_source_envelope(){
    let packet=json!({"version":4,"format":"rentemester.native-period-close.v1","sourceRevision":{"journal":[],"journalCount":[{"n":0}],"audit":[{"n":0,"head":null}]},"periodStart":"2025-01-01","periodEnd":"2025-03-31","cutoff":"2025-03-31","controlsRun":["AUDIT_CHAIN"],"items":[{"code":"AUDIT_CHAIN","status":"passed","waivable":false,"count":0,"amount":0,"evidence":[],"sourceHash":"b".repeat(64)}],"blockers":0,"warnings":0,"hash":"a".repeat(64)});
    let review=json!({"id":17,"packet":packet,"reviewerActor":"agent:synthetic","reviewerPrincipal":{"kind":"service-account","subjectId":"synthetic"},"createdAt":"2025-04-01 10:00:00"});
    let core=json!({"ok":true,"review":review});let response=format!("HTTP/1.1 200 OK\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{core}",core.to_string().len());
    let (origin,requests,task)=http(Box::leak(response.into_boxed_str()));
    let rows=parsed(&run(&origin,&[init("2025-11-25"),ready(),call(2,"period_close_review",json!({"company":"synthetic","from":"2025-01-01","to":"2025-03-31","confirm":true}))]));
    let wire=String::from_utf8(requests.recv_timeout(std::time::Duration::from_secs(5)).unwrap()).unwrap();task.join().unwrap();
    assert!(wire.starts_with("POST /api/companies/synthetic/periods/original/review-current HTTP/1.1"));
    assert_eq!(serde_json::from_str::<Value>(wire.split("\r\n\r\n").nth(1).unwrap()).unwrap(),json!({"periodStart":"2025-01-01","periodEnd":"2025-03-31","confirm":true}));
    let expected=json!({"ok":true,"errors":[],"data":{"review":review}});assert_eq!(rows[1]["result"]["structuredContent"],expected);assert_eq!(rows[1]["result"]["isError"],false);
    assert_eq!(serde_json::from_str::<Value>(rows[1]["result"]["content"][0]["text"].as_str().unwrap()).unwrap(),expected);
}

// Family-local countable mock: bounded startup and after-response observation, blocking accepted sockets.
fn http(response: &'static str) -> (String, mpsc::Receiver<Vec<u8>>, thread::JoinHandle<()>) {
    let listener=TcpListener::bind("127.0.0.1:0").unwrap(); let origin=format!("http://{}",listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap(); let (send,recv)=mpsc::channel();
    let task=thread::spawn(move || {
        let start=std::time::Instant::now();let mut responded=None;let mut count=0;
        loop {
            if responded.is_some_and(|t:std::time::Instant|t.elapsed()>std::time::Duration::from_millis(150)) {break;}
            assert!(start.elapsed()<std::time::Duration::from_secs(5),"mock startup deadline exceeded");
            let (mut stream,_)=match listener.accept(){Ok(v)=>v,Err(e) if e.kind()==std::io::ErrorKind::WouldBlock=>{thread::sleep(std::time::Duration::from_millis(5));continue;},Err(e)=>panic!("{e}")};
            stream.set_nonblocking(false).unwrap();stream.set_read_timeout(Some(std::time::Duration::from_secs(2))).unwrap();count+=1;
            let mut bytes=Vec::new();loop {
                let mut buf=[0;4096];let n=stream.read(&mut buf).unwrap();assert!(n>0);bytes.extend_from_slice(&buf[..n]);
                if let Some(end)=bytes.windows(4).position(|w|w==b"\r\n\r\n") {
                    let headers=String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                    let len=headers.lines().find_map(|l|l.strip_prefix("content-length: ")).map(|v|v.parse::<usize>().unwrap()).unwrap_or(0);
                    if bytes.len()>=end+4+len {break;}
                }
            }
            send.send(bytes).unwrap();stream.write_all(response.as_bytes()).unwrap();responded=Some(std::time::Instant::now());
        }
        assert_eq!(count,1,"automatic retry observed");
    });(origin,recv,task)
}
