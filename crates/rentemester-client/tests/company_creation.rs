mod common;
use common::TcpListener;
use rentemester_client::{ApiClient, Error, MAX_COMPANY_CREATION_BODY};
use std::{
    io::{Read, Write},
    thread,
    time::Duration,
};
#[tokio::test]
async fn new_company_is_workspace_scoped_exact_and_never_retried() {
    let body = "{ \"name\":\" Synthetic Company \",\"slug\":\"chosen-company\",\"fiscalYearStartMonth\":\"7\",\"fiscalYearLabelStrategy\":\"span\",\"vatPeriodType\":null,\"payment\":null }\n";
    for (status, expected) in [
        ("201 Created", None),
        ("200 OK", Some(Error::OutcomeUnknown)),
        ("409 Conflict", Some(Error::Refused { status: 409 })),
        ("503 Unavailable", Some(Error::OutcomeUnknown)),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let task = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut wire = vec![];
            loop {
                let mut buffer = [0; 4096];
                let n = stream.read(&mut buffer).unwrap();
                assert!(n > 0);
                wire.extend_from_slice(&buffer[..n]);
                if wire.ends_with(body.as_bytes()) {
                    break;
                }
            }
            let wire = String::from_utf8(wire).unwrap();
            assert!(wire.starts_with("POST /api/companies HTTP/1.1\r\n"));
            assert!(
                wire.to_ascii_lowercase()
                    .contains("cookie: session=synthetic")
            );
            assert!(wire.ends_with(body));
            let reply = r#"{"ok":true,"company":{"slug":"chosen-company","name":"Synthetic Company","futureCanonicalField":null}}"#;
            write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{reply}",
                reply.len()
            )
            .unwrap();
            drop(stream);
            listener.set_nonblocking(true).unwrap();
            thread::sleep(Duration::from_millis(50));
            assert!(listener.accept().is_err(), "no automatic creation replay");
        });
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        let result = api.create_company(body.as_bytes().to_vec(), true).await;
        if let Some(error) = expected {
            assert_eq!(result.unwrap_err(), error);
        } else {
            let value = result.unwrap();
            assert_eq!(value["company"]["slug"], "chosen-company");
            assert!(
                value["company"]
                    .get("futureCanonicalField")
                    .unwrap()
                    .is_null()
            );
        }
        task.join().unwrap();
    }
}
#[tokio::test]
async fn creation_rejects_inner_confirmation_keys_and_company_selectors() {
    let api = ApiClient::new("http://127.0.0.1:9", true, Some("session=synthetic".into())).unwrap();
    for bytes in [
        br#"{"name":"Synthetic","confirm":true}"#.to_vec(),
        br#"{"name":"Synthetic","company":"old"}"#.to_vec(),
        br#"{"name":"Synthetic","idempotencyKey":"invented"}"#.to_vec(),
        br#"{"name":"Synthetic","fiscalYearStartMonth":7}"#.to_vec(),
        br#"{"name":"Synthetic","payment":{"secret":"excluded"}}"#.to_vec(),
        vec![b' '; MAX_COMPANY_CREATION_BODY + 1],
    ] {
        assert!(matches!(
            api.create_company(bytes, true).await,
            Err(Error::InvalidInput(_))
        ));
    }
    assert!(matches!(
        api.create_company(br#"{"name":"Synthetic"}"#.to_vec(), false)
            .await,
        Err(Error::InvalidInput(_))
    ));
}

#[tokio::test]
async fn original116_company_sdk_flat_conversion_refusal_and_no_replay() {
    use serde_json::{Value,json};
    for (status, reply, code) in [
        ("201 Created",r#"{"ok":true,"company":{"slug":"synthetic","name":"Synthetic"}}"#,None),
        ("403 Forbidden",r#"{"ok":false,"errors":["current rights revoked"],"code":"MCP_UNAUTHORIZED","appliedRules":["current-authority"]}"#,None),
        ("409 Conflict","{}",Some(Error::Refused{status:409})),
        ("503 Unavailable","",Some(Error::OutcomeUnknown)),
        ("201 Created","{",Some(Error::OutcomeUnknown)),
    ] {
        let listener=TcpListener::bind("127.0.0.1:0").unwrap();
        let origin=format!("http://{}",listener.local_addr().unwrap());
        let task=thread::spawn(move || {
            let(mut stream,_)=listener.accept().unwrap();
            let mut wire=Vec::new();
            loop {
                let mut b=[0;4096];let n=stream.read(&mut b).unwrap();assert!(n>0);wire.extend_from_slice(&b[..n]);
                if let Some(i)=wire.windows(4).position(|b|b==b"\r\n\r\n") {
                    let head=String::from_utf8_lossy(&wire[..i]).to_ascii_lowercase();
                    let len=head.lines().find_map(|l|l.strip_prefix("content-length:")).unwrap().trim().parse::<usize>().unwrap();
                    if wire.len()>=i+4+len {break;}
                }
            }
            let wire=String::from_utf8(wire).unwrap();
            assert!(wire.starts_with("POST /api/companies HTTP/1.1"));
            assert!(wire.to_ascii_lowercase().contains("cookie: session=synthetic"));
            assert_eq!(serde_json::from_str::<Value>(wire.split_once("\r\n\r\n").unwrap().1).unwrap(),json!({"name":"Synthetic","slug":"synthetic","cvr":"DK12345678","fiscalYearStartMonth":"12","fiscalYearLabelStrategy":"start-year"}));
            write!(stream,"HTTP/1.1 {status}\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{reply}",reply.len()).unwrap();drop(stream);
            listener.set_nonblocking(true).unwrap();thread::sleep(Duration::from_millis(50));assert!(listener.accept().is_err(),"no replay");
        });
        let api=ApiClient::new(&origin,true,Some("session=synthetic".into())).unwrap();
        let r=api.create_company_original116(json!({"name":"Synthetic","slug":" synthetic ","cvr":"DK12345678","fiscalYearStartMonth":12,"fiscalYearLabelStrategy":"start-year","confirm":true})).await;
        if let Some(e)=code {assert_eq!(r.unwrap_err(),e);} else {assert_eq!(r.unwrap(),serde_json::from_str::<Value>(reply).unwrap());}
        task.join().unwrap();
    }
}

#[tokio::test]
async fn original116_company_sdk_confirmation_grammar_and_workspace_are_preflight() {
    use serde_json::json;
    let listener=TcpListener::bind("127.0.0.1:0").unwrap();
    let api=ApiClient::new(&format!("http://{}",listener.local_addr().unwrap()),true,None).unwrap();
    for args in [json!({"name":"Synthetic"}),json!({"name":"Synthetic","confirm":false,"workspace":"/missing"}),json!({"name":"Synthetic","confirm":true,"workspace":"/missing"}),json!({"name":"Synthetic","confirm":true,"workspace":"relative"}),json!({"name":"Synthetic","confirm":true,"actor":"user:forged"}),json!({"name":"Synthetic","confirm":true,"fiscalYearStartMonth":0}),json!({"name":"Synthetic","confirm":true,"fiscalYearStartMonth":13}),json!({"name":"Synthetic","confirm":true,"fiscalYearStartMonth":"7"}),json!({"name":"Synthetic","confirm":true,"cvr":null})] {
        assert!(matches!(api.create_company_original116(args).await,Err(Error::InvalidInput(_))));
    }
    listener.set_nonblocking(true).unwrap();assert!(listener.accept().is_err());
}

#[cfg(unix)]
#[tokio::test]
async fn original116_company_sdk_verified_local_workspace_matches_exact_root() {
    use rentemester_client::LocalRuntime;
    use serde_json::json;
    use std::{fs,os::unix::{fs::{DirBuilderExt,PermissionsExt},net::UnixListener}};
    // Match the established local-runtime fixture's private owned HOME profile.
    let root=fs::canonicalize(std::env::var_os("HOME").unwrap()).unwrap().join(format!(".rm116-company-{}",std::process::id()));
    fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
    fs::DirBuilder::new().mode(0o700).create(root.join("workspace")).unwrap();
    let listener=UnixListener::bind(root.join("r.sock")).unwrap();
    fs::set_permissions(root.join("r.sock"),fs::Permissions::from_mode(0o600)).unwrap();
    fs::write(root.join("r.json"),json!({"format":"rentemester-local-runtime/1","origin":"http://127.0.0.1:9","socket":"r.sock","workspaceRoot":root.join("workspace"),"workspaceId":root.join("other")}).to_string()).unwrap();
    fs::set_permissions(root.join("r.json"),fs::Permissions::from_mode(0o600)).unwrap();
    let local=LocalRuntime::load(&root).unwrap();
    let api=ApiClient::new_local(&local,Some("session=synthetic".into())).unwrap();
    assert!(matches!(api.create_company_original116(json!({"name":"Synthetic","confirm":true,"workspace":root.join("other")})).await,Err(Error::InvalidInput(_))));
    listener.set_nonblocking(true).unwrap();assert!(listener.accept().is_err());listener.set_nonblocking(false).unwrap();
    let task=thread::spawn(move || {
        let(mut s,_)=listener.accept().unwrap();let mut b=vec![];
        loop {let mut buf=[0;4096];let n=s.read(&mut buf).unwrap();assert!(n>0);b.extend_from_slice(&buf[..n]);if b.ends_with(br#"{"name":"Synthetic"}"#) {break;}}
        assert!(String::from_utf8(b).unwrap().starts_with("POST /api/companies HTTP/1.1"));
        let reply=r#"{"ok":true,"company":{"slug":"synthetic","name":"Synthetic"}}"#;
        write!(s,"HTTP/1.1 201 Created\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{reply}",reply.len()).unwrap();drop(s);
        listener.set_nonblocking(true).unwrap();thread::sleep(Duration::from_millis(50));assert!(listener.accept().is_err());
    });
    assert_eq!(api.create_company_original116(json!({"name":"Synthetic","confirm":true,"workspace":root.join("workspace")})).await.unwrap()["company"]["slug"],"synthetic");
    task.join().unwrap();fs::remove_dir_all(root).unwrap();
}
