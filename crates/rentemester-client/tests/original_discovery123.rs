mod common;
use rentemester_client::{ApiClient, Error};
use serde_json::{Value, json};
use std::{io::{Read, Write}, thread, time::Duration};

async fn invoke(kind: &str, selector: &str, status: u16, body: Value) -> (Result<Value, Error>, String) {
    let listener = common::TcpListener::bind("127.0.0.1:0").unwrap();
    let api = ApiClient::new(&format!("http://{}", listener.local_addr().unwrap()), true, Some("session=synthetic".into())).unwrap();
    let task = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut wire = Vec::new();
        while !wire.ends_with(b"\r\n\r\n") {
            let mut byte = [0]; stream.read_exact(&mut byte).unwrap(); wire.push(byte[0]);
        }
        let body = body.to_string();
        write!(stream,"HTTP/1.1 {status} Reply\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{body}",body.len()).unwrap();
        drop(stream);
        listener.set_nonblocking(true).unwrap();
        thread::sleep(Duration::from_millis(20));
        assert!(listener.accept().is_err(), "no automatic retry or fallback");
        String::from_utf8(wire).unwrap()
    });
    let result = match kind {
        "about" => api.original_agent_about123().await,
        "search" => api.original_agent_capabilities123(selector,u64::MAX,50).await,
        _ => api.original_agent_workflow123(selector).await,
    };
    (result,task.join().unwrap())
}

#[tokio::test]
async fn original123_sdk_exact_routes_queries_and_raw_core() {
    let core=search_core();
    let (result,wire) = invoke("search","😀 &?=",200,core.clone()).await;
    assert_eq!(result.unwrap(),core);
    let target = wire.lines().next().unwrap().split_whitespace().nth(1).unwrap();
    let url = reqwest::Url::parse(&format!("http://localhost{target}")).unwrap();
    assert_eq!(url.path(),"/api/original/agent-capabilities");
    assert_eq!(url.query_pairs().collect::<Vec<_>>(),vec![("query".into(),"😀 &?=".into()),("cursor".into(),u64::MAX.to_string().into()),("limit".into(),"50".into())]);
    let (result,wire) = invoke("about","",200,about_core()).await;
    assert_eq!(result.unwrap()["serverName"],"rentemester-mcp");
    assert!(wire.starts_with("GET /api/original/agent-about HTTP/1.1"));
    let (result,wire) = invoke("workflow","unknown/../?x#%😀",404,json!({"code":"UNKNOWN_WORKFLOW"})).await;
    assert!(matches!(result,Err(Error::Refused{status:404})));
    assert!(wire.starts_with("GET /api/original/agent-workflows/unknown%2F%2E%2E%2F%3Fx%23%25%F0%9F%98%80 HTTP/1.1"));
}

#[tokio::test]
async fn original123_sdk_utf16_limits_refuse_before_network() {
    let api = ApiClient::new("http://127.0.0.1:9",true,None).unwrap();
    for query in ["a".repeat(201),"😀".repeat(101)] {
        assert!(matches!(api.original_agent_capabilities123(&query,0,10).await,Err(Error::InvalidInput(_))));
    }
    for limit in [0,51,usize::MAX] {
        assert!(matches!(api.original_agent_capabilities123("",0,limit).await,Err(Error::InvalidInput(_))));
    }
    for id in [String::new(),"a".repeat(101),"😀".repeat(51)] {
        assert!(matches!(api.original_agent_workflow123(&id).await,Err(Error::InvalidInput(_))));
    }
}

#[tokio::test]
async fn original123_sdk_typed_refusals_do_not_retry() {
    for status in [401,403,429,503] {
        assert!(matches!(invoke("search","",status,json!({})).await.0,Err(Error::Refused{status:s}) if s==status));
    }
}

fn catalogue()->Value{json!({"schemaVersion":"rentemester-agent-discovery-v1","hash":"a".repeat(64),"capabilityCount":26,"workflowCount":31,"fullCoverage":false})}
fn search_core()->Value{json!({"catalogue":catalogue(),"workspaceConnected":true,"items":[],"total":26,"count":0,"cursor":u64::MAX,"limit":50,"hasMore":false,"nextCursor":null})}
fn about_core()->Value{json!({"catalogue":catalogue(),"workspaceConnected":true,"serverName":"rentemester-mcp","serverVersion":"0.2.0","toolCount":null,"contractDocs":["docs/mcp-agent-contract.md"]})}
#[tokio::test]
async fn original123_malformed_success_is_protocol_before_metadata_decoration(){
 for kind in ["about","search","workflow"]{
  for bad in [json!({}),json!([]),json!({"catalogue":catalogue(),"workspaceConnected":true,"workflow":{"id":"synthetic","live":true,"steps":[null]}})]{
   assert!(matches!(invoke(kind,"synthetic",200,bad).await.0,Err(Error::Protocol)));
  }
 }
 let mut bad=search_core();bad["items"]=json!([null]);bad["count"]=json!(1);assert!(matches!(invoke("search","",200,bad).await.0,Err(Error::Protocol)));
}
