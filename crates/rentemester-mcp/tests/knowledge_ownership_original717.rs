use serde_json::{json,Value};
use std::{
    io::{Read, Write},
    net::TcpListener,
    path::PathBuf,
    process::{Command, Output, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
    },
    thread,
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-mcp");
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Cookie(PathBuf);
impl Cookie {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "rm-mcp-{}-{}.cookie",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options
            .open(&path)
            .unwrap()
            .write_all(b"session=never-print-this\n")
            .unwrap();
        Self(path)
    }
}
impl Drop for Cookie {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
fn run(origin: &str, messages: &[Value]) -> Output {
    let lines: String = messages.iter().map(|v| format!("{v}\n")).collect();
    run_raw(origin, lines.as_bytes())
}
fn run_raw(origin: &str, bytes: &[u8]) -> Output {
    let cookie = Cookie::new();
    let mut child = Command::new(BIN)
        .args([
            "--server",
            origin,
            "--allow-loopback-http",
            "--cookie-file",
            cookie.0.to_str().unwrap(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Drain stdout while feeding stdin: discovery and document frames can exceed pipe capacity.
    let bytes = bytes.to_vec();
    let mut stdin = child.stdin.take().unwrap();
    let writer = thread::spawn(move || stdin.write_all(&bytes));
    let output = child.wait_with_output().unwrap();
    writer.join().unwrap().unwrap();
    output
}
fn parsed(output: &Output) -> Vec<Value> {
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("never-print-this"));
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn init(version: &str) -> Value {
    json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":version,"capabilities":{},"clientInfo":{"name":"test","version":"1"}}})
}
fn ready() -> Value {
    json!({"jsonrpc":"2.0","method":"notifications/initialized"})
}
fn call(id: i32, name: &str, args: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":name,"arguments":args}})
}
use std::time::{Duration,Instant};
fn invoke(name:&str,args:Value,body:Option<Value>,status:&'static str)->(Value,String){
    let listener=TcpListener::bind("127.0.0.1:0").unwrap();
    let origin=format!("http://{}",listener.local_addr().unwrap());
    let task=thread::spawn(move||{
        listener.set_nonblocking(true).unwrap();
        let mut business=String::new();
        let deadline=Instant::now()+Duration::from_secs(8);
        let mut done=None;
        loop {
            match listener.accept(){
                Ok((mut socket,_))=>{
                    assert!(done.is_none(),"unexpected retry");
                    socket.set_nonblocking(false).unwrap();socket.set_read_timeout(Some(Duration::from_secs(4))).unwrap();
                    let mut bytes=vec![];
                    loop {let mut b=[0;4096];let n=socket.read(&mut b).unwrap();assert!(n>0);bytes.extend_from_slice(&b[..n]);
                        if let Some(i)=bytes.windows(4).position(|w|w==b"\r\n\r\n"){
                            let len=String::from_utf8_lossy(&bytes[..i]).lines().find_map(|l|l.to_lowercase().strip_prefix("content-length: ").map(|v|v.parse::<usize>().unwrap())).unwrap_or(0);
                            if bytes.len()>=i+4+len {break}
                        }
                    }
                    let request=String::from_utf8(bytes).unwrap();
                    let protocol=request.starts_with("GET /api/protocol ");
                    let response=if protocol {
                        assert!(!request.to_lowercase().contains("cookie:"));
                        Some(include_str!("../../../protocol/http-v1.json").to_string())
                    }else{business=request;done=Some(Instant::now());body.as_ref().map(Value::to_string)};
                    if let Some(response)=response{let status=if protocol{"200 OK"}else{status};socket.write_all(format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()).as_bytes()).unwrap();}
                },
                Err(e) if e.kind()==std::io::ErrorKind::WouldBlock=>{if done.is_some_and(|t:Instant|t.elapsed()>Duration::from_millis(100)){break}assert!(Instant::now()<deadline,"no business request");thread::sleep(Duration::from_millis(3));},
                Err(e)=>panic!("{e}"),
            }
        }
        business
    });
    let rows=parsed(&run(&origin,&[init("2025-11-25"),ready(),call(2,name,args)]));
    let request=task.join().unwrap();
    let r=&rows[1]["result"];let envelope=r["structuredContent"].clone();
    assert_eq!(r["content"].as_array().unwrap().len(),1);assert_eq!(r["content"][0]["type"],"text");
    assert_eq!(serde_json::from_str::<Value>(r["content"][0]["text"].as_str().unwrap()).unwrap(),envelope);
    assert_eq!(r["isError"],envelope["ok"]!=true);
    (envelope,request)
}
const AT:&str="2026-10-22T00:00:00.000Z";
fn event(state:&str)->Value {json!({"event_type":state,"reason":null,"supersedes_assertion_id":null,"actor":"user:fixture","principal_kind":"user","principal_id":"fixture","created_at":AT})}
fn assertion(id:&str,state:&str)->Value{
    let mut history=vec![event("proposed")];if state!="proposed"{history.push(event(state));}
    // SHA256 for canonical JSON null, not an invented source proof.
    json!({"assertionId":id,"companySlug":"synthetic","predicate":"business_description","value":null,"valueHash":"74234e98afe7498fb5daf1f36ac2d78acc339464f950703b8c019892f982b90b","source":{"kind":"user","ref":"fixture"},"validFrom":AT,"validToExclusive":null,"certainty":"confirmed","actor":"user:fixture","principal":{"kind":"user","id":"fixture"},"reviewState":state,"history":history})
}
fn proposal()->Value {json!({"company":"synthetic","assertionId":"knowledge-fixture","predicate":"business_description","value":null,"source":{"kind":"user","ref":"fixture"},"validFrom":AT,"confirm":true})}
fn fact()->Value {json!({"owner":{"kind":"party","partyId":"party-fixture"},"ownedCompanySlug":"synthetic","validFrom":"2026-10-22","economicBasisPoints":10000,"controlType":"equity","jurisdiction":"DK","evidenceRefs":["fixture"]})}
// Canonical proof values copied from the immutable production producer algorithm for
// this synthetic observation; test computes them outside the adapter using source vectors.
fn snapshot(state:&str)->Value {
    let mut history=vec![event("proposed")];if state!="proposed"{let mut review=event("approved");review["actor"]=json!("user:reviewer");review["principal_id"]=json!("reviewer");history.push(review);}if state=="applied"{history.push(event("applied"));}
    json!({"snapshotId":"snapshot-fixture","source":"fixture","observedAt":AT,"snapshotHash":SNAPSHOT_HASH,"diffHash":DIFF_HASH,"actor":"user:fixture","principal":{"kind":"user","id":"fixture"},"state":state,"history":history,"diff":{"version":2,"add":[fact()],"unchanged":[],"conflicts":[],"endings":[],"note":"Only an explicit later fact can end an approved legal fact"}})
}
const SNAPSHOT_HASH:&str="df49adbe67b11859e06825735ce2ffdfc3fc4fcdf8e4321b166896ce57786e2f";
const DIFF_HASH:&str="386e967d927077b056a9dd310876cd992800c0e4fab4879b857d57c8cebbdad8";
fn cases()->Vec<(&'static str,Value,Value)> {
    let a=assertion("knowledge-fixture","proposed");
    let mut old=assertion("knowledge-fixture","superseded");old["history"].as_array_mut().unwrap().insert(1,event("approved"));old["history"][2]["supersedes_assertion_id"]=json!("knowledge-replacement");
    vec![
        ("company_knowledge_context",json!({"company":"synthetic","asOf":AT}),json!({"ok":true,"context":{"companySlug":"synthetic","asOf":AT,"assertions":[],"conflicts":[],"safeForProductBehavior":true},"history":[a.clone()]})),
        ("company_knowledge_propose",proposal(),json!({"ok":true,"assertion":a})),
        ("company_knowledge_review",json!({"company":"synthetic","assertionId":"knowledge-fixture","decision":"approved","confirm":true}),json!({"ok":true,"assertion":assertion("knowledge-fixture","approved")})),
        ("company_knowledge_supersede",{let mut p=proposal();p.as_object_mut().unwrap().remove("company");p.as_object_mut().unwrap().remove("confirm");p["assertionId"]=json!("knowledge-replacement");json!({"company":"synthetic","assertionId":"knowledge-fixture","replacement":p,"confirm":true})},json!({"ok":true,"superseded":old,"replacement":assertion("knowledge-replacement","proposed")})),
        ("ownership_graph_query",json!({"company":"synthetic","asOf":"2026-10-22"}),json!({"ok":true,"asOf":"2026-10-22","facts":[fact()],"partial":false,"consolidation":{"eligible":false,"reason":"party, interval or non-exact ownership requires explicit supported scope"}})),
        ("ownership_snapshot_history",json!({"company":"synthetic","snapshotId":"snapshot-fixture"}),json!({"ok":true,"history":[snapshot("proposed")]})),
        ("ownership_snapshot_propose",json!({"company":"synthetic","snapshotId":"snapshot-fixture","source":"fixture","observedAt":AT,"facts":[fact()],"confirm":true}),json!({"ok":true,"snapshot":snapshot("proposed")})),
        ("ownership_snapshot_review",json!({"company":"synthetic","snapshotId":"snapshot-fixture","decision":"approved","confirm":true}),json!({"ok":true,"snapshot":snapshot("approved")})),
        ("ownership_snapshot_apply",json!({"company":"synthetic","snapshotId":"snapshot-fixture","snapshotHash":SNAPSHOT_HASH,"diffHash":DIFF_HASH,"confirm":true}),json!({"ok":true,"snapshot":snapshot("applied"),"status":"applied"})),
    ]
}
#[test]
fn all_original_commands_and_native_coexistence(){
    let rows=parsed(&run("http://127.0.0.1:9",&[init("2025-11-25"),ready(),json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})]));
    let tools=rows[1]["result"]["tools"].as_array().unwrap();
    for (name,args,mut native) in cases(){
        let tool=tools.iter().find(|t|t["name"]==name).unwrap();let read=!args.as_object().unwrap().contains_key("confirm");
        assert_eq!(tool["annotations"],json!({"readOnlyHint":read,"destructiveHint":false,"idempotentHint":read,"openWorldHint":false}));
        assert!(!tool["inputSchema"]["required"].as_array().unwrap().contains(&json!("confirm")));
        let (got,request)=invoke(name,args.clone(),Some(native.clone()),"200 OK");
        native.as_object_mut().unwrap().remove("ok");assert_eq!(got,json!({"ok":true,"errors":[],"data":native}),"{name}");
        assert!(request.contains("/api/companies/synthetic/"));
        if !read {let sent:Value=serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap();let mut expected=args;expected.as_object_mut().unwrap().remove("company");assert_eq!(sent,expected);assert!(request.starts_with("POST "));}
    }
    for n in ["company_knowledge","propose_company_knowledge","ownership","propose_ownership"] {assert!(tools.iter().any(|t|t["name"]==n),"missing native {n}");}
}
#[test]
fn confirmation_invalid_input_no_effect(){
    let listener=TcpListener::bind("127.0.0.1:0").unwrap();listener.set_nonblocking(true).unwrap();let origin=format!("http://{}",listener.local_addr().unwrap());
    for (name,args,_) in cases(){
        if args.get("confirm").is_some(){for confirm in [None,Some(false)]{let mut a=args.clone();a.as_object_mut().unwrap().remove("confirm");if let Some(b)=confirm{a["confirm"]=json!(b);}
            let rows=parsed(&run(&origin,&[init("2025-11-25"),ready(),call(2,name,a)]));assert_eq!(rows[1]["result"]["structuredContent"],json!({"ok":false,"errors":[format!("confirm: true required for write tool {name}")],"code":"CONFIRM_REQUIRED"}));}}
        for key in ["actor","body","workspace","principal"] {let mut a=args.clone();a[key]=json!("forged");let rows=parsed(&run(&origin,&[init("2025-11-25"),ready(),call(2,name,a)]));assert_eq!(rows[1]["error"]["code"],-32602);}
    }
    assert!(listener.accept().is_err());
}
#[test]
fn malformed_mismatched_denied_and_lost_receipts(){
    for (name,args,_) in cases(){
        for response in [json!({"ok":true}),json!({"ok":true,"data":{}})]{let (got,_)=invoke(name,args.clone(),Some(response),"200 OK");assert_eq!(got["ok"],false);}
        let (got,_)=invoke(name,args.clone(),Some(json!({"error":{"code":"access_denied"}})),"403 Forbidden");assert_eq!(got["ok"],false);
        if args.get("confirm").is_some(){let (got,request)=invoke(name,args,None,"200 OK");assert_eq!(got["ok"],false);assert!(request.starts_with("POST "));assert!(got["errors"][0].as_str().unwrap().contains("no retry"));}
    }
    let mut a=assertion("wrong-id","proposed");let (got,_)=invoke("company_knowledge_propose",proposal(),Some(json!({"ok":true,"assertion":a})),"200 OK");assert_eq!(got["ok"],false);
    a=assertion("knowledge-fixture","proposed");a["companySlug"]=json!("other");let(got,_)=invoke("company_knowledge_propose",proposal(),Some(json!({"ok":true,"assertion":a})),"200 OK");assert_eq!(got["ok"],false);
}
#[test]
fn graph_explicit_anchor_filters_other_visible_companies(){
    let mut f=fact();f["ownedCompanySlug"]=json!("other");
    let(got,_)=invoke("ownership_graph_query",json!({"company":"synthetic","asOf":"2026-10-22"}),Some(json!({"ok":true,"asOf":"2026-10-22","facts":[f],"partial":false,"consolidation":{"eligible":false,"reason":"party, interval or non-exact ownership requires explicit supported scope"}})),"200 OK");
    assert_eq!(got,json!({"ok":true,"errors":[],"data":{"asOf":"2026-10-22","facts":[],"partial":true,"consolidation":{"eligible":false,"reason":"hidden endpoints"}}}));
}
