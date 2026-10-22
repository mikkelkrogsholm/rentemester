//! Explicit parsing releases the owner mutex while the bounded native child runs.
use super::*;
use rentemester_application::document_pdf::{Completion, Read};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Confirm {
    confirm: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Pending {
    confirm: bool,
    cursor: Option<i64>,
    limit: Option<usize>,
}
fn checked<T>(
    attempt: rentemester_host::Attempt<
        Result<
            rentemester_application::control_fence::Committed<T>,
            rentemester_application::control_fence::Failure,
        >,
    >,
) -> Result<T, Box<Response>> {
    if let Err(f) = &attempt.outcome
        && f.stage == FailureStage::NotCommitted
        && matches!(f.error.code, "PDF_EVIDENCE_TAMPERED" | "tampered_result")
    {
        return Err(Box::new(refused(
            StatusCode::CONFLICT,
            "PDF_EVIDENCE_TAMPERED",
        )));
    }
    documents::outcome(attempt)
}
// Preparatory results are capabilities, not final committed Value receipts.
fn prepare_outcome(
    attempt: rentemester_host::Attempt<
        Result<
            rentemester_application::control_fence::Committed<
                rentemester_application::document_pdf::Admission,
            >,
            rentemester_application::control_fence::Failure,
        >,
    >,
) -> Result<rentemester_application::document_pdf::Admission, Box<Response>> {
    if let Err(f) = &attempt.outcome
        && matches!(f.error.code, "PDF_EVIDENCE_TAMPERED" | "tampered_result")
    {
        return Err(Box::new(refused(
            StatusCode::CONFLICT,
            "PDF_EVIDENCE_TAMPERED",
        )));
    }
    if attempt.retired || attempt.outcome.as_ref().is_ok_and(|v| !v.control_reusable) {
        return Err(Box::new(refused(
            StatusCode::SERVICE_UNAVAILABLE,
            "owner_unavailable",
        )));
    }
    checked(attempt)
}
fn service_checked<T>(
    attempt: rentemester_host::Attempt<
        Result<
            rentemester_application::service_company::ServiceResult<T>,
            rentemester_application::service_company::ServiceFailure,
        >,
    >,
) -> Result<T, Box<Response>> {
    if let Err(f) = &attempt.outcome
        && matches!(f.error.code, "PDF_EVIDENCE_TAMPERED" | "tampered_result")
    {
        return Err(Box::new(refused(
            StatusCode::CONFLICT,
            "PDF_EVIDENCE_TAMPERED",
        )));
    }
    documents::service_outcome(attempt)
}
fn prepare_service_outcome(
    attempt: rentemester_host::Attempt<
        Result<
            rentemester_application::service_company::ServiceResult<
                rentemester_application::document_pdf::Admission,
            >,
            rentemester_application::service_company::ServiceFailure,
        >,
    >,
) -> Result<rentemester_application::document_pdf::Admission, Box<Response>> {
    if let Err(f) = &attempt.outcome
        && matches!(f.error.code, "PDF_EVIDENCE_TAMPERED" | "tampered_result")
    {
        return Err(Box::new(refused(
            StatusCode::CONFLICT,
            "PDF_EVIDENCE_TAMPERED",
        )));
    }
    if attempt.retired || attempt.outcome.as_ref().is_ok_and(|v| !v.control_reusable) {
        return Err(Box::new(refused(
            StatusCode::SERVICE_UNAVAILABLE,
            "owner_unavailable",
        )));
    }
    service_checked(attempt)
}
fn one_parse(
    state: &Product,
    cookie: &documents::Credential,
    slug: &str,
    id: i64,
) -> Result<Value, Box<Response>> {
    let parser = state.native.document_pdf_worker.as_ref().ok_or_else(|| {
        Box::new(refused(
            StatusCode::SERVICE_UNAVAILABLE,
            "pdf_worker_unavailable",
        ))
    })?;
    let admission = {
        let mut owner = state
            .native
            .lock_company(slug)
            .map_err(|_| refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable"))?;
        match cookie {
            documents::Credential::Human(cookie) => prepare_outcome(
                owner
                    .prepare_document_pdf(cookie, &SystemClock, id, parser.identity().clone())
                    .map_err(|_| refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable"))?,
            )?,
            documents::Credential::Service(key) => prepare_service_outcome(
                owner
                    .prepare_document_pdf_service(key, &SystemClock, id, parser.identity().clone())
                    .map_err(|_| refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable"))?,
            )?,
        }
    };
    let completion = admission.worker_input().map(|bytes| Completion {
        identity: parser.identity().clone(),
        outcome: parser.parse(bytes).map_err(|error| error.code()),
    });
    let mut owner = state
        .native
        .lock_company(slug)
        .map_err(|_| refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable"))?;
    match cookie {
        documents::Credential::Human(cookie) => checked(
            owner
                .finish_document_pdf(cookie, &SystemClock, admission, completion)
                .map_err(|_| refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable"))?,
        ),
        documents::Credential::Service(key) => service_checked(
            owner
                .finish_document_pdf_service(key, &SystemClock, admission, completion)
                .map_err(|_| refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable"))?,
        ),
    }
}
enum Input {
    Parse(i64),
    Pending { cursor: i64, limit: usize },
    Read(Read),
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let path = request.uri().path().to_owned();
    let parts: Vec<_> = path.split('/').collect();
    let slug = parts.get(3).copied().unwrap_or("").to_owned();
    let pending = path.ends_with("/parse-pending");
    let write = pending || path.ends_with("/parse");
    let pdf_permit = if write {
        match state.pdf_workers.clone().try_acquire_owned() {
            Ok(permit) => Some(permit),
            Err(_) => return refused(StatusCode::SERVICE_UNAVAILABLE, "busy"),
        }
    } else {
        None
    };
    let cookie = match documents::credential(&state, &request, write) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    if request.headers().contains_key("content-encoding")
        || request.headers().contains_key("idempotency-key")
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let id = if pending {
        0
    } else {
        match parts
            .get(5)
            .and_then(|s| s.parse::<i64>().ok())
            .filter(|n| (1..=9_007_199_254_740_991).contains(n))
        {
            Some(n) => n,
            None => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
        }
    };
    let query = request.uri().query().unwrap_or("").to_owned();
    let input = if write {
        if !query.is_empty()
            || !one(request.headers(), "content-type")
                .is_some_and(|v| v.eq_ignore_ascii_case("application/json"))
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        let Ok(bytes) = to_bytes(request.into_body(), 4096).await else {
            return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
        };
        if pending {
            let Ok(p) = serde_json::from_slice::<Pending>(&bytes) else {
                return refused(StatusCode::BAD_REQUEST, "invalid_input");
            };
            let (cursor, limit) = (p.cursor.unwrap_or(0), p.limit.unwrap_or(100));
            if !p.confirm
                || !(0..=9_007_199_254_740_991).contains(&cursor)
                || !(1..=100).contains(&limit)
            {
                return refused(StatusCode::BAD_REQUEST, "invalid_input");
            }
            Input::Pending { cursor, limit }
        } else {
            if !serde_json::from_slice::<Confirm>(&bytes).is_ok_and(|c| c.confirm) {
                return refused(StatusCode::BAD_REQUEST, "confirmation_required");
            }
            Input::Parse(id)
        }
    } else {
        let text = path.ends_with("/parsed-text");
        let (mut offset, mut limit) = (0usize, 10usize);
        if !query.is_empty() {
            if !text {
                return refused(StatusCode::BAD_REQUEST, "invalid_input");
            }
            let mut seen = std::collections::BTreeSet::new();
            for pair in query.split('&') {
                let Some((key, value)) = pair.split_once('=') else {
                    return refused(StatusCode::BAD_REQUEST, "invalid_input");
                };
                if !seen.insert(key)
                    || !matches!(key, "offset" | "limit")
                    || value.is_empty()
                    || !value.bytes().all(|b| b.is_ascii_digit())
                {
                    return refused(StatusCode::BAD_REQUEST, "invalid_input");
                }
                let Ok(n) = value.parse::<usize>() else {
                    return refused(StatusCode::BAD_REQUEST, "invalid_input");
                };
                if key == "offset" {
                    offset = n;
                } else {
                    limit = n;
                }
            }
        }
        if offset as u128 > 9_007_199_254_740_991 || !(1..=10).contains(&limit) {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        let Ok(bytes) = to_bytes(request.into_body(), 1).await else {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        };
        if !bytes.is_empty() {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        Input::Read(if text {
            Read::Text {
                document_id: id,
                offset,
                limit,
            }
        } else {
            Read::Status(id)
        })
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result=tokio::task::spawn_blocking(move || {
        let _permit=permit;
        let _pdf_permit=pdf_permit;
        match input {
            Input::Parse(id)=>one_parse(&state,&cookie,&slug,id),
            Input::Read(read)=>{
                let mut owner=state.native.lock_company(&slug).map_err(|_|refused(StatusCode::SERVICE_UNAVAILABLE,"owner_unavailable"))?;
                read_authorized(&mut owner,&cookie,read)
            },
            Input::Pending{cursor,limit}=>{
                let parser=state.native.document_pdf_worker.as_ref().ok_or_else(||Box::new(refused(StatusCode::SERVICE_UNAVAILABLE,"pdf_worker_unavailable")))?;
                let plan={
                    let mut owner=state.native.lock_company(&slug).map_err(|_|refused(StatusCode::SERVICE_UNAVAILABLE,"owner_unavailable"))?;
                    read_authorized(&mut owner,&cookie,Read::Pending{identity:parser.identity().clone(),cursor,limit})?
                };
                let ids=plan["documentIds"].as_array().ok_or_else(||Box::new(refused(StatusCode::SERVICE_UNAVAILABLE,"pdf_plan_unavailable")))?;
                let mut failed=Vec::new();
                for id in ids {
                    let id=id.as_i64().ok_or_else(||Box::new(refused(StatusCode::SERVICE_UNAVAILABLE,"pdf_plan_unavailable")))?;
                    match one_parse(&state,&cookie,&slug,id) {Ok(value) if value["ok"]==true=>{},Ok(_)=>failed.push(id),Err(r)=>return Err(r)}
                }
                Ok(json!({"ok":true,"batch":{"requested":ids.len(),"parsed":ids.len()-failed.len(),"failed":failed.len(),"cursor":plan["cursor"],"nextCursor":plan["nextCursor"],"resume":if failed.is_empty(){Value::Null}else{json!({"documentIds":failed})}}}))
            }
        }
    }).await;
    match result {
        Ok(Ok(value)) => response(StatusCode::OK, value),
        Ok(Err(r)) => *r,
        _ => refused(
            StatusCode::SERVICE_UNAVAILABLE,
            if write {
                "outcome_unknown"
            } else {
                "pdf_read_unavailable"
            },
        ),
    }
}

fn read_authorized(
    owner: &mut rentemester_host::Owner,
    credential: &documents::Credential,
    action: Read,
) -> Result<Value, Box<Response>> {
    match credential {
        documents::Credential::Human(cookie) => checked(
            owner
                .document_pdf(cookie, &SystemClock, action)
                .map_err(|_| refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable"))?,
        ),
        documents::Credential::Service(key) => service_checked(
            owner
                .document_pdf_service(key, &SystemClock, action)
                .map_err(|_| refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable"))?,
        ),
    }
}

#[cfg(test)]
mod documents_original_tests {
    use super::*;
    #[test]
    fn documents_original_retired_preparation_never_releases_admission() {
        use rentemester_application::control_fence::{Failure, FailureStage};
        let attempt = rentemester_host::Attempt {
            retired: true,
            outcome: Err(Failure {
                stage: FailureStage::NotCommitted,
                error: rentemester_engine::Error::new("unavailable", "retired preparation"),
                control_reusable: false,
                company_reusable: false,
            }),
        };
        match prepare_outcome(attempt) {
            Err(response) => assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE),
            Ok(_) => panic!("retired preparation released a parser capability"),
        }
        let attempt = rentemester_host::Attempt {
            retired: true,
            outcome: Err(Failure {
                stage: FailureStage::NotCommitted,
                error: rentemester_engine::Error::new("PDF_EVIDENCE_TAMPERED", "tampered source"),
                control_reusable: false,
                company_reusable: false,
            }),
        };
        match prepare_outcome(attempt) {
            Err(response) => assert_eq!(response.status(), StatusCode::CONFLICT),
            Ok(_) => panic!("tampered preparation released a parser capability"),
        }
    }
}
