mod common;
use common::TcpListener;
use rentemester_client::{
    ApiClient, BookkeepingBatchMutation as Op, Error, MAX_BOOKKEEPING_BATCH_BODY,
};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    sync::mpsc,
    thread,
    time::Duration,
};
fn server(
    status: &'static str,
    reply: &'static str,
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
        l.set_nonblocking(true).unwrap();
        thread::sleep(Duration::from_millis(60));
        1 + l.incoming().take_while(Result::is_ok).count()
    });
    (origin, rx, t)
}

fn scope() -> Value {
    json!({"accountingFrom":"2026-01-01","accountingTo":"2026-12-31","bankFrom":"2026-01-01","bankTo":"2026-12-31"})
}
fn request(op: Op) -> Vec<u8> {
    let v = if op == Op::Persist {
        let mut v = scope();
        v["runKey"] = json!("chosen-synthetic-run");
        v["expectedPlanHash"] = json!("a".repeat(64));
        v["confirm"] = json!(true);
        v
    } else {
        json!({"runId":7,"planHash":"a".repeat(64),"confirm":true})
    };
    format!(" {v}\n").into_bytes()
}
#[tokio::test]
async fn five_routes_exact_body_scope_and_lossless_canonical_receipts() {
    const JSON: &str = r#"{"ok":true,"state":{"future":{"amount":"90071992547409.99"}},"results":[{"receipt":{"entryId":3}}]}"#;
    for action in ["plan", "status", "persist", "approve", "apply"] {
        let (origin, rx, t) = server("200 OK", JSON, "application/json");
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        let op = match action {
            "persist" => Some(Op::Persist),
            "approve" => Some(Op::Approve),
            "apply" => Some(Op::Apply),
            _ => None,
        };
        let bytes = op.map(request);
        let v = match op {
            Some(op) => {
                api.mutate_bookkeeping_batch("synthetic", op, bytes.clone().unwrap(), true)
                    .await
            }
            None if action == "plan" => api.bookkeeping_batch_plan("synthetic", &scope()).await,
            None => api.bookkeeping_batch_status("synthetic", 7).await,
        }
        .unwrap();
        assert_eq!(v, serde_json::from_str::<Value>(JSON).unwrap());
        let got = rx.recv_timeout(Duration::from_secs(3)).unwrap();
        let h = String::from_utf8_lossy(&got);
        let first = h.lines().next().unwrap();
        assert!(first.starts_with(if op.is_some() { "POST " } else { "GET " }));
        if action == "plan" {
            assert!(first.contains("/api/companies/synthetic/bookkeeping-batch?"));
            for (k, v) in scope().as_object().unwrap() {
                assert!(first.contains(&format!("{k}={}", v.as_str().unwrap())));
            }
        } else {
            let suffix = if action == "status" { "runs/7" } else { action };
            assert!(first.contains(&format!(
                "/api/companies/synthetic/bookkeeping-batch/{suffix} "
            )));
        }
        let end = got.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
        assert_eq!(&got[end..], bytes.as_deref().unwrap_or(&[]));
        assert_eq!(t.join().unwrap(), 1);
    }
}
#[tokio::test]
async fn invalid_native_shapes_refuse_before_http() {
    let api = ApiClient::new("http://127.0.0.1:9", true, Some("session=synthetic".into())).unwrap();
    for bytes in [b"{}".to_vec(),vec![b' ';MAX_BOOKKEEPING_BATCH_BODY+1],br#"{"runId":7,"planHash":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA","confirm":true}"#.to_vec(),br#"{"runId":7,"planHash":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","confirm":false}"#.to_vec(),br#"{"runId":7,"planHash":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","confirm":true,"actor":"user:forged"}"#.to_vec()]{assert!(matches!(api.mutate_bookkeeping_batch("synthetic",Op::Apply,bytes,true).await,Err(Error::InvalidInput(_))));}
    assert!(matches!(
        api.mutate_bookkeeping_batch("synthetic", Op::Apply, request(Op::Apply), false)
            .await,
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        api.bookkeeping_batch_status("synthetic", 0).await,
        Err(Error::InvalidInput(_))
    ));
    let mut s = scope();
    s["bankTo"] = json!("2025-01-01");
    assert!(matches!(
        api.bookkeeping_batch_plan("synthetic", &s).await,
        Err(Error::InvalidInput(_))
    ));
}
#[tokio::test]
async fn known_refusal_keeps_details_and_uncertain_response_never_retries() {
    for (status, reply, unknown) in [
        (
            "422 Unprocessable Entity",
            r#"{"ok":false,"code":"batch_stale","checks":[{"ok":false}],"future":{"amount":"0.01"}}"#,
            false,
        ),
        (
            "503 Service Unavailable",
            r#"{"ok":false,"code":"outcome_unknown"}"#,
            true,
        ),
        (
            "408 Request Timeout",
            r#"{"ok":false,"code":"timeout"}"#,
            true,
        ),
    ] {
        let (origin, rx, t) = server(status, reply, "application/json");
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        let got = api
            .mutate_bookkeeping_batch("synthetic", Op::Apply, request(Op::Apply), true)
            .await;
        if unknown {
            assert!(matches!(got, Err(Error::OutcomeUnknown)));
        } else {
            assert_eq!(got.unwrap(), serde_json::from_str::<Value>(reply).unwrap());
        }
        rx.recv_timeout(Duration::from_secs(3)).unwrap();
        assert_eq!(t.join().unwrap(), 1);
    }
}

#[tokio::test]
async fn unknown_persist_can_read_exact_run_key_without_resending() {
    let (origin, rx, t) = server(
        "200 OK",
        r#"{"ok":true,"state":{"run":{"id":7}}}"#,
        "application/json",
    );
    let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
    assert_eq!(
        api.bookkeeping_batch_status_by_key("synthetic", "chosen key/one")
            .await
            .unwrap()["state"]["run"]["id"],
        7
    );
    let wire = rx.recv_timeout(Duration::from_secs(3)).unwrap();
    assert!(wire.starts_with(
        b"GET /api/companies/synthetic/bookkeeping-batch/runs?runKey=chosen+key%2Fone HTTP/1.1"
    ));
    assert_eq!(t.join().unwrap(), 1);
}

fn workbench_input() -> Value {
    json!({"from":"2026-01-01","to":"2026-01-31","status":"ready","bankAccountId":7.0,
        "partyId":"party:synthetic","documentQuality":"matched","account":"1234",
        "vatTreatment":"purchase & VAT","dimension":"project:one","cursor":2.0,"limit":3.0,
        "search":"synthetic / æ & ?"})
}
#[tokio::test]
async fn original_first_use116_workbench_all_fields_are_inert_lossless_get() {
    const REPLY: &str = r#"{"ok":true,"workbench":{"rows":[],"population":{"total":9,"ready":2,"blockers":7},"selection":{"total":0,"ready":0,"blockers":0},"page":{"cursor":2.0,"limit":3.0,"total":0,"nextCursor":null},"sourceHash":"canonical","staleSources":[{"changedSource":"evidence_changed"}],"plan":{"planHash":"reviewed"},"periodClose":{"status":"unavailable","blockers":null},"future":{"amount":"90071992547409.99"}}}"#;
    let (origin, rx, task) = server("200 OK", REPLY, "application/json");
    let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
    assert_eq!(api.bookkeeping_workbench("synthetic", &workbench_input()).await.unwrap(), serde_json::from_str::<Value>(REPLY).unwrap());
    let bytes = rx.recv_timeout(Duration::from_secs(3)).unwrap();
    let wire = String::from_utf8(bytes).unwrap();
    assert!(wire.starts_with("GET /api/companies/synthetic/bookkeeping-workbench?"));
    for value in ["from=2026-01-01", "to=2026-01-31", "status=ready", "bankAccountId=7", "partyId=party%3Asynthetic", "documentQuality=matched", "account=1234", "vatTreatment=purchase+%26+VAT", "dimension=project%3Aone", "cursor=2", "limit=3", "search=synthetic+%2F+%C3%A6+%26+%3F"] {
        assert!(wire.lines().next().unwrap().contains(value), "{value}: {wire}");
    }
    assert!(wire.to_lowercase().contains("cookie: session=synthetic"));
    assert!(wire.ends_with("\r\n\r\n"));
    assert_eq!(task.join().unwrap(),1);
}
#[tokio::test]
async fn original_first_use116_workbench_invalid_grammar_and_selector_never_send() {
    let api = ApiClient::new("http://127.0.0.1:9",true,None).unwrap();
    for (key, value) in [
        ("from",json!("2026-2-30")),("to",json!("2025-12")),("status",json!("invented")),
        ("bankAccountId",json!(0)),("bankAccountId",json!(9007199254740992u64)),("partyId",json!("ab")),
        ("documentQuality",json!("guessed")),("account",json!("x".repeat(33))),
        ("vatTreatment",json!("x".repeat(81))),("dimension",json!("ab")),
        ("cursor",json!(-1)),("cursor",json!(9007199254740992u64)),("limit",json!(101)),
        ("limit",json!(0)),("limit",json!("3")),("search",json!("x".repeat(201))),
        ("actor",json!("user:forged")),("workspace",json!("/invented")),("confirm",json!(true)),
        ("scope",json!({})),("companyId",json!(1)),("search",Value::Null),
    ] {
        let mut input=workbench_input(); input[key]=value;
        assert!(matches!(api.bookkeeping_workbench("synthetic",&input).await,Err(Error::InvalidInput(_))),"{key}");
    }
    for company in ["", "../other", "other/ledger", "company?actor=forged"] {
        assert!(matches!(api.bookkeeping_workbench(company,&workbench_input()).await,Err(Error::InvalidInput(_))));
    }
    for key in ["from","to"] {
        let mut input=workbench_input(); input.as_object_mut().unwrap().remove(key);
        assert!(matches!(api.bookkeeping_workbench("synthetic",&input).await,Err(Error::InvalidInput(_))));
    }
    let mut input=workbench_input();input["search"]=json!("æ".repeat(200));
    assert!(ApiClient::validate_bookkeeping_workbench(&input).is_ok());
}
#[tokio::test]
async fn original_first_use116_workbench_current_refusal_and_response_loss_no_replay() {
    for (status, reply) in [
        ("403 Forbidden",r#"{"ok":false,"code":"permission_denied","errors":["current CompanyRead required"],"appliedRules":["current_authority"]}"#),
        ("404 Not Found",r#"{"ok":false,"code":"company_not_found","errors":["wrong selector"]}"#),
        ("401 Unauthorized",r#"{"ok":false,"code":"session_expired","errors":["expired"]}"#),
        ("200 OK",""),
        ("200 OK",r#"{"ok":true,"workbench":null}"#),
        ("200 OK",r#"{"ok":true,"other":[]}"#),
    ] {
        let (origin,rx,task)=server(status,reply,"application/json");
        let api=ApiClient::new(&origin,true,None).unwrap();
        let got=api.bookkeeping_workbench("synthetic",&json!({"from":"2026-01-01","to":"2026-01-31"})).await;
        if reply.is_empty() || serde_json::from_str::<Value>(reply).is_ok_and(|v| v["ok"]==true) {assert!(matches!(got,Err(Error::Protocol)));}
        else {assert_eq!(got.unwrap(),serde_json::from_str::<Value>(reply).unwrap());}
        rx.recv_timeout(Duration::from_secs(3)).unwrap();assert_eq!(task.join().unwrap(),1);
    }
}

#[tokio::test]
async fn original_first_use116_workbench_concurrent_calls_keep_company_and_page_isolated() {
    let (origin_a,rx_a,task_a)=server("200 OK",r#"{"ok":true,"workbench":{"page":{"cursor":0},"sourceHash":"a","population":{"blockers":4}}}"#,"application/json");
    let (origin_b,rx_b,task_b)=server("200 OK",r#"{"ok":true,"workbench":{"page":{"cursor":5},"sourceHash":"b","population":{"blockers":8}}}"#,"application/json");
    let api_a=ApiClient::new(&origin_a,true,None).unwrap();
    let api_b=ApiClient::new(&origin_b,true,None).unwrap();
    let input_a=json!({"from":"2026-01-01","to":"2026-01-31","cursor":0,"search":"alpha"});
    let input_b=json!({"from":"2026-02-01","to":"2026-02-28","cursor":5,"search":"beta"});
    let (a,b)=tokio::join!(api_a.bookkeeping_workbench("synthetic-a",&input_a),api_b.bookkeeping_workbench("synthetic-b",&input_b));
    assert_eq!(a.unwrap()["workbench"]["population"]["blockers"],4);
    assert_eq!(b.unwrap()["workbench"]["population"]["blockers"],8);
    for (rx,company,cursor,search,task) in [(rx_a,"synthetic-a",0,"alpha",task_a),(rx_b,"synthetic-b",5,"beta",task_b)] {
        let wire=String::from_utf8(rx.recv_timeout(Duration::from_secs(3)).unwrap()).unwrap();
        assert!(wire.starts_with(&format!("GET /api/companies/{company}/bookkeeping-workbench?")));
        assert!(wire.contains(&format!("cursor={cursor}")));assert!(wire.contains(&format!("search={search}")));
        assert_eq!(task.join().unwrap(),1);
    }
}

#[tokio::test]
async fn original_first_use116_workbench_domain_dates_remain_current_server_business_refusals() {
    let reply=r#"{"ok":false,"code":"invalid_input","errors":["ordered valid ISO range required"]}"#;
    for (from,to) in [("2026-02-30","2026-03-01"),("2026-01-31","2026-01-01")] {
        let(origin,rx,task)=server("400 Bad Request",reply,"application/json");
        let api=ApiClient::new(&origin,true,None).unwrap();
        assert_eq!(api.bookkeeping_workbench("synthetic",&json!({"from":from,"to":to})).await.unwrap(),serde_json::from_str::<Value>(reply).unwrap());
        let wire=String::from_utf8(rx.recv_timeout(Duration::from_secs(3)).unwrap()).unwrap();assert!(wire.contains(&format!("from={from}")));assert!(wire.contains(&format!("to={to}")));assert_eq!(task.join().unwrap(),1);
    }
}

#[cfg(unix)]
#[tokio::test]
async fn original_first_use116_workbench_absolute_company_requires_real_local_root() {
    use rentemester_client::LocalRuntime;
    use std::{fs,os::unix::{fs::{DirBuilderExt,PermissionsExt},net::UnixListener}};
    let root=fs::canonicalize(std::env::var_os("HOME").unwrap()).unwrap().join(format!(".rm116-workbench-{}",std::process::id()));
    fs::DirBuilder::new().mode(0o700).create(&root).unwrap();fs::DirBuilder::new().mode(0o700).create(root.join("workspace")).unwrap();
    let listener=UnixListener::bind(root.join("r.sock")).unwrap();fs::set_permissions(root.join("r.sock"),fs::Permissions::from_mode(0o600)).unwrap();
    fs::write(root.join("r.json"),json!({"format":"rentemester-local-runtime/1","origin":"http://127.0.0.1:9","socket":"r.sock","workspaceRoot":root.join("workspace"),"workspaceId":root.join("elsewhere")}).to_string()).unwrap();fs::set_permissions(root.join("r.json"),fs::Permissions::from_mode(0o600)).unwrap();
    let local=LocalRuntime::load(&root).unwrap();let api=ApiClient::new_local(&local,None).unwrap();let input=json!({"from":"2026-01-01","to":"2026-01-31"});
    for company in [root.join("elsewhere/synthetic"),root.join("workspace/../synthetic"),root.join("workspace/nested/synthetic")] { assert!(matches!(api.bookkeeping_workbench(company.to_str().unwrap(),&input).await,Err(Error::InvalidInput(_)))); }
    listener.set_nonblocking(true).unwrap();assert!(listener.accept().is_err());listener.set_nonblocking(false).unwrap();
    let task=thread::spawn(move||{let(mut stream,_)=listener.accept().unwrap();let mut bytes=Vec::new();loop{let mut b=[0;4096];let n=stream.read(&mut b).unwrap();assert!(n>0);bytes.extend_from_slice(&b[..n]);if bytes.ends_with(b"\r\n\r\n"){break;}}let wire=String::from_utf8(bytes).unwrap();assert!(wire.starts_with("GET /api/companies/synthetic/bookkeeping-workbench?"));let reply=r#"{"ok":true,"workbench":{"rows":[],"population":{"blockers":3}}}"#;write!(stream,"HTTP/1.1 200 OK\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{reply}",reply.len()).unwrap();});
    assert_eq!(api.bookkeeping_workbench(root.join("workspace/synthetic").to_str().unwrap(),&input).await.unwrap()["workbench"]["population"]["blockers"],3);task.join().unwrap();fs::remove_dir_all(root).unwrap();
}
