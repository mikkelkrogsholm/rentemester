use super::*;
use rentemester_runtime::document_scanner::Scanner;
use rentemester_application::mail_intake::original717::{Input, Message};
use rentemester_runtime::imap::{self, original717::ConnectionInput};
pub(crate) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let cookie = match proof(&state, &request, true) {
        Ok(v) => v,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    if request.uri().query().is_some()
        || request.headers().contains_key("content-encoding")
        || !one(request.headers(), "content-type")
            .is_some_and(|s| s.eq_ignore_ascii_case("application/json"))
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let slug = request
        .uri()
        .path()
        .split('/')
        .nth(3)
        .unwrap_or("")
        .to_owned();
    let poll = request.uri().path().ends_with("original-imap-intake");
    let Ok(bytes) = to_bytes(request.into_body(), 90 * 1024 * 1024).await else {
        return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
    };
    let Ok(mut body) = serde_json::from_slice::<Value>(&bytes) else {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    };
    let connection = if poll {
        let Some(fields) = body.as_object_mut() else {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        };
        let mut connection = json!({});
        for k in [
            "imapHost",
            "imapPort",
            "imapUsername",
            "imapMailbox",
            "sinceUid",
        ] {
            if let Some(v) = fields.remove(k) {
                connection[k] = v;
            }
        }
        fields.insert("messages".into(), json!([]));
        match serde_json::from_value::<ConnectionInput>(connection) {
            Ok(c) => Some(c),
            Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
        }
    } else {
        None
    };
    let Ok(mut input) = serde_json::from_value::<Input>(body) else {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    };
    if !input.confirm {
        return response(
            StatusCode::OK,
            json!({"ok":true,"result":{"ok":false,"errors":["confirm: true is required"]}}),
        );
    }
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    match tokio::task::spawn_blocking(move || {
        let _permit=permit;
        let value=if let Some(connection)=connection {
            let config=match imap::original717::resolve(&connection,|key|std::env::var(key).ok()) {
                Ok(c)=>c, Err(e)=>return result(json!({"ok":false,"errors":e})),
            };
            let admission={
                let Ok(mut owner)=state.native.lock_company(&slug) else {return refused(StatusCode::SERVICE_UNAVAILABLE,"owner_unavailable");};
                match owner.original717_prepare_imap_intake_poll(&state.native.cookies,&cookie,&SystemClock,config) {
                    Ok(a)=>match outcome(a) {Ok(a)=>a,Err(r)=>return r},
                    Err(_)=>return refused(StatusCode::SERVICE_UNAVAILABLE,"outcome_unknown"),
                }
            };
            let mut network=match NetworkClient::new(imap::OperatorConfig::default()) {Ok(c)=>c,Err(_)=>return result(json!({"ok":false,"errors":["imap_unavailable"]}))};
            let mut cursor=connection.since_uid.map(|uid|imap::Cursor{uid_validity:0,last_uid:uid});
            let mut total=json!({"ok":true,"messagesFetched":0,"messagesProcessed":0,"attachmentsIngested":0,"attachmentsSkipped":0,"exceptionsCreated":0,"documents":[]});
            let mut errors=Vec::new();
            loop {
                let batch=match network.fetch(admission.config(),cursor) {Ok(b)=>b,Err(e)=>{errors.push(e.code().to_owned());break;}};
                let next=match imap::original717::next_cursor(&batch,cursor) {Ok(c)=>c,Err(e)=>{errors.push(e.code().to_owned());break;}};
                if !batch.refusals.is_empty() {
                    let mut owner=match state.native.lock_company(&slug) {Ok(o)=>o,Err(_)=>return refused(StatusCode::SERVICE_UNAVAILABLE,"owner_unavailable")};
                    let receipt=match owner.original717_oversized_mail(&state.native.cookies,&cookie,&SystemClock,&admission,batch.uid_validity,&batch.refusals) {
                        Ok(a)=>match outcome(a){Ok(v)=>v,Err(r)=>return r},
                        Err(_)=>return refused(StatusCode::SERVICE_UNAVAILABLE,"outcome_unknown"),
                    };
                    total["messagesFetched"]=json!(total["messagesFetched"].as_u64().unwrap_or(0)+batch.refusals.len() as u64);
                    for key in ["messagesProcessed","exceptionsCreated"] {total[key]=json!(total[key].as_u64().unwrap_or(0)+receipt[key].as_u64().unwrap_or(0));}
                }
                input.messages=batch.messages.into_iter().map(|m|Message{source:format!("imap:{}",m.uid),base64:imap::original717::encode_eml(&m.raw)}).collect();
                let fetched=input.messages.len();
                match ingest(&state,&slug,&cookie,input.clone(),Some(&admission)) {
                    Ok(value)=> {
                        if value["ok"]!=true {return result(value);}
                        total["messagesFetched"]=json!(total["messagesFetched"].as_u64().unwrap_or(0)+fetched as u64);
                        for key in ["messagesProcessed","attachmentsIngested","attachmentsSkipped","exceptionsCreated"] {total[key]=json!(total[key].as_u64().unwrap_or(0)+value[key].as_u64().unwrap_or(0));}
                        if let Some(docs)=value["documents"].as_array() {total["documents"].as_array_mut().unwrap().extend(docs.iter().cloned());}
                    },
                    Err(r)=>return r,
                }
                if let Some(next)=next {cursor=Some(next);} else {break;}
            }
            if !errors.is_empty() {total["ok"]=json!(false);total["errors"]=json!(errors);}
            Ok(total)
        } else {ingest(&state,&slug,&cookie,input,None)};
        match value {Ok(v)=>result(v),Err(r)=>r}
    }).await {Ok(r)=>r,Err(_)=>refused(StatusCode::SERVICE_UNAVAILABLE,"outcome_unknown")}
}
pub(crate) fn result(value: Value) -> Response {
    response(StatusCode::OK, json!({"ok":true,"result":value}))
}
/// Only the nine original717 adapters use this projection. Authentication and
/// uncertain commits remain transport failures; known domain refusals remain
/// ordinary original tool results.
pub(crate) fn outcome<T>(
    a: rentemester_host::Attempt<
        Result<
            rentemester_application::control_fence::Committed<T>,
            rentemester_application::control_fence::Failure,
        >,
    >,
) -> Result<T, Response> {
    if a.retired {
        return Err(refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unknown"));
    }
    match a.outcome {
        Ok(c) if c.control_reusable => Ok(c.value),
        Err(f)
            if f.stage == FailureStage::NotCommitted
                && f.control_reusable
                && f.company_reusable =>
        {
            if f.error.code == "invalid_credentials" {
                Err(refused(StatusCode::UNAUTHORIZED, "unauthorized"))
            } else if f.error.code == "access_denied" {
                Err(refused(StatusCode::FORBIDDEN, "access_denied"))
            } else {
                Err(result(
                    json!({"ok":false,"code":f.error.code,"errors":[f.error.message]}),
                ))
            }
        }
        _ => Err(refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unknown")),
    }
}
fn ingest(
    state: &Product,
    slug: &str,
    cookie: &rentemester_runtime::auth::VerifiedCookie,
    input: Input,
    poll: Option<&rentemester_application::mail_intake::original717::PollAdmission>,
) -> Result<Value, Response> {
    let admission = {
        let mut owner = state
            .native
            .lock_company(slug)
            .map_err(|_| refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable"))?;
        let policy = if state.native.document_scanner.is_some() {
            rentemester_application::documents::ScanPolicy::Required
        } else {
            rentemester_application::documents::ScanPolicy::Off
        };
        outcome(
            owner
                .original717_prepare_mail_ingest(
                    &state.native.cookies,
                    cookie,
                    &SystemClock,
                    input,
                    policy,
                    poll,
                )
                .map_err(|_| refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unknown"))?,
        )?
    };
    let mut scans = Vec::new();
    if let Some(scanner) = &state.native.document_scanner {
        for file in admission.files() {
            match scanner.scan(file) {
                Ok(scan) => scans.push(scan),
                Err(rentemester_runtime::document_scanner::ScanFailure::Rejected) => {
                    return Err(result(
                        json!({"ok":false,"errors":["document_scanner_rejected"]}),
                    ));
                }
                Err(_) => {
                    return Err(result(
                        json!({"ok":false,"errors":["document_scanner_unavailable"]}),
                    ));
                }
            }
        }
    }
    let mut owner = state
        .native
        .lock_company(slug)
        .map_err(|_| refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable"))?;
    outcome(
        owner
            .original717_finish_mail_ingest(
                &state.native.cookies,
                cookie,
                &SystemClock,
                admission,
                scans,
            )
            .map_err(|_| refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unknown"))?,
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn original717_domain_refusal_is_result_but_auth_and_unknown_are_transport() {
        use rentemester_application::control_fence::{Failure, FailureStage};
        fn rejected(code: &'static str, stage: FailureStage) -> Response {
            let a = rentemester_host::Attempt {
                retired: false,
                outcome: Err(Failure {
                    stage,
                    error: rentemester_engine::Error::new(code, "synthetic rejection"),
                    control_reusable: true,
                    company_reusable: true,
                }),
            };
            match outcome::<Value>(a) {
                Err(r) => r,
                Ok(_) => panic!("rejection accepted"),
            }
        }
        let domain = rejected("invoice_not_found", FailureStage::NotCommitted);
        assert_eq!(domain.status(), StatusCode::OK);
        let body = to_bytes(domain.into_body(), 4096).await.unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["ok"], true);
        assert_eq!(value["result"]["ok"], false);
        assert_eq!(value["result"]["errors"], json!(["synthetic rejection"]));
        assert_eq!(
            rejected("invalid_credentials", FailureStage::NotCommitted).status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            rejected("access_denied", FailureStage::NotCommitted).status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            rejected("invoice_not_found", FailureStage::OutcomeUnknown).status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }
}
