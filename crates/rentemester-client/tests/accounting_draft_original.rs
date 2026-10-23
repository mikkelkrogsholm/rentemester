mod common;
use common::TcpListener;
use rentemester_client::{
    AccountingDraftOriginalOperation as Op, ApiClient, Error, original_accounting_draft_schema,
    parse_original_accounting_draft_json, validate_original_accounting_draft,
};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    thread,
};
fn payload() -> Value {
    json!({"transactionDate":"2026-01-02","text":"Synthetic receipt","currency":"DKK","documentId":1,"lines":[{"accountNo":"3729","debitAmount":12.5},{"accountNo":"8729","creditAmount":12.5}]})
}
fn input() -> Value {
    json!({"draftId":" synthetic ","payload":payload(),"confirm":true})
}
fn state(status: &str) -> Value {
    json!({"id":"synthetic","version":1,"status":status,"payloadHash":"a".repeat(64),"eventHash":"b".repeat(64),"payload":payload(),"actorId":"service_account:historical-author"})
}
fn server(status: u16, body: String) -> (String, thread::JoinHandle<String>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", l.local_addr().unwrap());
    let t = thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut bytes = vec![];
        loop {
            let mut b = [0; 4096];
            let n = s.read(&mut b).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&b[..n]);
            if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let header = String::from_utf8_lossy(&bytes[..i]);
                let len = header
                    .lines()
                    .find_map(|l| {
                        l.to_lowercase()
                            .strip_prefix("content-length:")
                            .map(|v| v.trim().parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                if bytes.len() >= i + 4 + len {
                    break;
                }
            }
        }
        write!(
            s,
            "HTTP/1.1 {status} Mock\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        String::from_utf8(bytes).unwrap()
    });
    (origin, t)
}
#[test]
fn strict_schema_file_parser_and_bounds() {
    for op in [
        Op::Create,
        Op::Revise,
        Op::Submit,
        Op::Reject,
        Op::ApproveAndPost,
        Op::Show,
        Op::List,
    ] {
        assert_eq!(
            original_accounting_draft_schema(op)["additionalProperties"],
            false
        );
    }
    assert!(validate_original_accounting_draft(Op::Create, &input()).is_ok());
    for raw in [
        b"[]".as_slice(),
        br#"{"x":1,"x":2}"#,
        br#"{"lines":[{"accountNo":"1","accountNo":"2"}]}"#,
        br#"{"createdBy":"user:forged"}"#,
    ] {
        assert!(parse_original_accounting_draft_json(raw).is_err());
    }
    assert!(parse_original_accounting_draft_json(&serde_json::to_vec(&payload()).unwrap()).is_ok());
    assert!(parse_original_accounting_draft_json(&vec![b' '; 262145]).is_err());
    for (k, v) in [
        ("draftId", json!("Bad")),
        ("confirm", json!(false)),
        ("actor", json!("user:forged")),
        ("expectedEventHash", json!("a".repeat(64))),
    ] {
        let mut i = input();
        i[k] = v;
        assert!(validate_original_accounting_draft(Op::Create, &i).is_err());
    }
    let mut i = input();
    i["payload"]["createdByProgram"] = json!("caller");
    assert!(validate_original_accounting_draft(Op::Create, &i).is_err());
    let mut i = input();
    i["payload"]["text"] = json!("x".repeat(262144));
    assert!(validate_original_accounting_draft(Op::Create, &i).is_err());
    for h in [
        "A".repeat(64),
        "a".repeat(63),
        format!(" {}", "a".repeat(64)),
    ] {
        assert!(
            validate_original_accounting_draft(
                Op::Submit,
                &json!({"draftId":"synthetic","expectedEventHash":h,"confirm":true})
            )
            .is_err()
        );
    }
}
#[test]
fn http_status_first_no_retry_strict_receipts_and_historical_reads() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let good = json!({"ok":true,"accountingDraft":state("created")});
        for (status, body, expected) in [
            (200, good.clone(), None),
            (403, good.clone(), Some(Error::Refused { status: 403 })),
            (503, good.clone(), Some(Error::OutcomeUnknown)),
            (
                200,
                json!({"ok":false,"errors":["refused"]}),
                Some(Error::OutcomeUnknown),
            ),
            (422, json!({"ok":false,"errors":["duplicate draft"]}), None),
            (
                422,
                json!({"ok":true}),
                Some(Error::Refused { status: 422 }),
            ),
        ] {
            let (origin, t) = server(status, body.to_string());
            let api = ApiClient::new(&origin, true, None).unwrap();
            let r = api
                .original_accounting_draft("synthetic", Op::Create, &input(), None, None)
                .await;
            if let Some(e) = expected {
                assert_eq!(r, Err(e));
            } else {
                assert_eq!(r.unwrap(), body);
            }
            let request = t.join().unwrap();
            assert!(
                request
                    .starts_with("POST /api/companies/synthetic/original/accounting-draft/create ")
            );
            assert!(request.contains("\"confirm\":true"));
        }
        for body in [good.clone(), json!({"ok":true,"accountingDraft":null})] {
            let (origin, t) = server(200, body.to_string());
            let api = ApiClient::new(&origin, true, None).unwrap();
            assert_eq!(
                api.original_accounting_draft(
                    "synthetic",
                    Op::Show,
                    &json!({"draftId":" synthetic "}),
                    None,
                    None
                )
                .await
                .unwrap(),
                body
            );
            let r = t.join().unwrap();
            assert!(r.starts_with(
                "GET /api/companies/synthetic/original/accounting-draft/show?draftId=synthetic "
            ));
            assert!(!r.contains("\"draftId\""));
        }
        let mut bad = good.clone();
        bad["accountingDraft"]["reason"] = Value::Null;
        let duplicate = good
            .to_string()
            .replacen("\"ok\":true", "\"ok\":true,\"ok\":true", 1);
        for body in [bad.to_string(), duplicate, "x".repeat(16 * 1024 * 1024 + 1)] {
            let (origin, t) = server(200, body);
            let api = ApiClient::new(&origin, true, None).unwrap();
            assert_eq!(
                api.original_accounting_draft("synthetic", Op::Create, &input(), None, None)
                    .await,
                Err(Error::OutcomeUnknown)
            );
            t.join().unwrap();
        }
    });
}
#[test]
fn approval_binds_journal_and_list_is_ordered() {
    tokio::runtime::Runtime::new().unwrap().block_on(async{
 let mut s=state("approved_posted");s["journalEntryId"]=json!(7);s["journal"]=json!({"ok":true,"entryId":7,"entryNo":"2026-0001","entryHash":"c".repeat(64),"appliedRules":["required-rule"],"errors":[]});
 let input=json!({"draftId":"synthetic","expectedEventHash":"a".repeat(64),"confirm":true});
 for bad in [false,true]{let mut s=s.clone();if bad{s["journal"]["entryId"]=json!(8);}let body=json!({"ok":true,"accountingDraft":s});let(origin,t)=server(200,body.to_string());let api=ApiClient::new(&origin,true,None).unwrap();let result=api.original_accounting_draft("synthetic",Op::ApproveAndPost,&input,Some("service_account:historical-author"),None).await;if bad{assert_eq!(result,Err(Error::OutcomeUnknown));}else{assert_eq!(result.unwrap(),body);}t.join().unwrap();}
 let mut second=state("submitted");second["id"]=json!("z-draft");
 for rows in [json!([state("created"),second.clone()]),json!([second,state("created")]),json!([state("created"),state("created")])]{let sorted=rows[0]["id"]=="synthetic" && rows[1]["id"]=="z-draft";let body=json!({"ok":true,"accountingDrafts":rows});let(origin,t)=server(200,body.to_string());let api=ApiClient::new(&origin,true,None).unwrap();let r=api.original_accounting_draft("synthetic",Op::List,&json!({}),None,None).await;assert_eq!(r.is_ok(),sorted);let r=t.join().unwrap();assert!(r.starts_with("GET /api/companies/synthetic/original/accounting-draft/list "));}
 });
}

#[test]
fn retained_source_actor_kinds_are_evidence_not_current_credentials() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        for actor in [
            "agent:retained",
            "system:retained",
            "user:old",
            "service_account:old",
        ] {
            let mut retained = state("submitted");
            retained["actorId"] = json!(actor);
            for op in [Op::Show, Op::List] {
                let body = if op == Op::List {
                    json!({"ok":true,"accountingDrafts":[retained]})
                } else {
                    json!({"ok":true,"accountingDraft":retained})
                };
                let (origin, t) = server(200, body.to_string());
                let api = ApiClient::new(&origin, true, None).unwrap();
                let input = if op == Op::List {
                    json!({})
                } else {
                    json!({"draftId":"synthetic"})
                };
                assert_eq!(
                    api.original_accounting_draft("synthetic", op, &input, None, None)
                        .await
                        .unwrap(),
                    body
                );
                t.join().unwrap();
            }
        }
        let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
        for actor in ["agent:retained", "system:retained"] {
            assert!(matches!(
                api.original_accounting_draft("synthetic", Op::Create, &input(), Some(actor), None)
                    .await,
                Err(Error::InvalidInput(_))
            ));
        }
    });
}
