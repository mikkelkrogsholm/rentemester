use rentemester_client::{ApiClient, Error, SupplierCommitmentOriginalOperation as Op, original_supplier_commitment_schema, validate_original_supplier_commitment};
use serde_json::{Value,json};
use std::{io::{Read,Write},net::TcpListener,thread};
fn commitment()->Value {json!({"commitmentId":"synthetic","vendorPartyId":"vendor:1","type":"contract","description":"Reviewed source","businessPurpose":"Operations","amount":125.25,"currency":"DKK","frequency":"monthly","nextDate":"2026-01-01","evidenceRefs":["document:1"]})}
#[test]
fn frozen_schema_rejects_authority_and_forged_evidence() {
    for op in [Op::Plan,Op::Apply,Op::List,Op::Change,Op::Match,Op::Matches,Op::Alerts] {
        let schema=original_supplier_commitment_schema(op);
        assert_eq!(schema["additionalProperties"],false);
        assert!(schema["properties"].get("actor").is_none());
        assert!(schema["properties"].get("company").is_none());
        let mut bad=json!({"principal":"forged"});
        assert!(validate_original_supplier_commitment(op,&bad).is_err());
        bad["confirm"]=json!(true);
        assert!(validate_original_supplier_commitment(op,&bad).is_err());
    }
    assert!(validate_original_supplier_commitment(Op::Plan,&json!({"commitment":commitment()})).is_ok());
    let mut bad=commitment(); bad["amount"]=json!("125.25");
    assert!(validate_original_supplier_commitment(Op::Plan,&json!({"commitment":bad})).is_err());
    assert!(validate_original_supplier_commitment(Op::Match,&json!({"commitmentId":"synthetic","occurrenceDate":"2026-01-01","evidence":{"kind":"payable","id":"1","amount":12},"confirm":true})).is_err());
    assert!(validate_original_supplier_commitment(Op::Alerts,&json!({})).is_err());
    assert!(validate_original_supplier_commitment(Op::Alerts,&json!({"asOf":"malformed"})).is_ok());
}
fn server(status:u16,reply:Value,target:&'static str)->(String,thread::JoinHandle<()>) {
    let listener=TcpListener::bind("127.0.0.1:0").unwrap();
    let origin=format!("http://{}",listener.local_addr().unwrap());
    let handle=thread::spawn(move||{
        let (mut stream,_)=listener.accept().unwrap();
        stream.set_read_timeout(Some(std::time::Duration::from_secs(3))).unwrap();
        let mut bytes=Vec::new();
        loop {
            let mut chunk=[0;4096]; let n=stream.read(&mut chunk).unwrap(); assert!(n>0);bytes.extend_from_slice(&chunk[..n]);
            if let Some(end)=bytes.windows(4).position(|w|w==b"\r\n\r\n") {
                let h=String::from_utf8_lossy(&bytes[..end]);
                assert!(h.starts_with(target),"{h}");
                let len=h.lines().find_map(|l|l.to_ascii_lowercase().strip_prefix("content-length:").map(|n|n.trim().parse::<usize>().unwrap())).unwrap_or(0);
                if bytes.len()>=end+4+len {break;}
            }
        }
        let body=reply.to_string();write!(stream,"HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        listener.set_nonblocking(true).unwrap();
        thread::sleep(std::time::Duration::from_millis(100));
        assert!(listener.accept().is_err(),"adapter must not retry");
    });(origin,handle)
}
#[tokio::test]
async fn status_and_operation_specific_receipts_fail_closed() {
    for (op,input,status,reply,expected) in [
        (Op::List,json!({}),200,json!({"ok":true,"rows":[],"errors":[]}),None),
        (Op::List,json!({}),200,json!({"ok":true}),Some(Error::Protocol)),
        (Op::Plan,json!({"commitment":commitment()}),200,json!({"ok":true,"proposal":{"payloadHash":"a".repeat(64)}}),Some(Error::Protocol)),
        (Op::Change,json!({"commitmentId":"synthetic","action":"paused","reason":"Reviewed","confirm":true}),200,json!({"ok":true}),Some(Error::OutcomeUnknown)),
        (Op::Change,json!({"commitmentId":"synthetic","action":"paused","reason":"Reviewed","confirm":true}),200,json!({"ok":true,"idempotent":false,"payloadHash":"a".repeat(64)}),None),
        (Op::List,json!({}),422,json!({"ok":false,"errors":["Source refusal retained"]}),None),
        (Op::List,json!({}),403,json!({"ok":false,"errors":["Not business"]}),Some(Error::Refused{status:403})),
        (Op::List,json!({}),422,json!({"ok":true,"rows":[],"errors":[]}),Some(Error::Refused{status:422})),
    ] {
        let target=match op {Op::List=>"GET /api/companies/synthetic/original/supplier-commitments ",Op::Plan=>"POST /api/companies/synthetic/original/supplier-commitments/plan ",_=>"POST /api/companies/synthetic/original/supplier-commitments/change "};
        let (origin,handle)=server(status,reply.clone(),target);
        let api=ApiClient::new(&origin,true,None).unwrap();
        let result=api.original_supplier_commitment("synthetic",op,&input,None).await;
        if let Some(e)=expected {assert_eq!(result,Err(e));} else {assert_eq!(result.unwrap(),reply);}
        handle.join().unwrap();
    }
}
