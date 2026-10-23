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
    super::documents_original::service_outcome(attempt)
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
// The request's admission already owns one global PDF slot. Borrow only idle
// slots for its remaining lanes: competing requests must never create 4 x 4
// children. Each lane keeps its permit until all of its actual work has drained.
// No owner/SQL guard crosses this boundary; one_parse owns both fenced phases.
fn parse_batch<T: Send>(
    ids: &[i64],
    first: tokio::sync::OwnedSemaphorePermit,
    slots: &std::sync::Arc<tokio::sync::Semaphore>,
    parse: impl Fn(i64) -> T + Sync,
) -> Result<Vec<T>, ()> {
    let mut permits = vec![first];
    while permits.len() < ids.len().min(4) {
        match slots.clone().try_acquire_owned() {
            Ok(permit) => permits.push(permit),
            Err(_) => break,
        }
    }
    let next = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|scope| {
        let mut lanes = Vec::new();
        for permit in permits {
            let (next, parse) = (&next, &parse);
            lanes.push(scope.spawn(move || {
                let _permit = permit;
                let mut completed = Vec::new();
                loop {
                    let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(&id) = ids.get(index) else { break };
                    completed.push((index, parse(id)));
                }
                completed
            }));
        }
        let mut completed = Vec::new();
        let mut panicked = false;
        // Always join every lane, even after a panic/error. Successful effects
        // are not replayed and no child is left running after the response.
        for lane in lanes {
            match lane.join() {
                Ok(values) => completed.extend(values),
                Err(_) => panicked = true,
            }
        }
        if panicked {
            return Err(());
        }
        completed.sort_by_key(|(index, _)| *index);
        Ok(completed.into_iter().map(|(_, value)| value).collect())
    })
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
        let pdf_permit=pdf_permit;
        match input {
            Input::Parse(id)=>{
                let _pdf_permit=pdf_permit;
                one_parse(&state,&cookie,&slug,id)
            },
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
                let ids=plan["documentIds"].as_array().ok_or_else(||Box::new(refused(StatusCode::SERVICE_UNAVAILABLE,"pdf_plan_unavailable")))?
                    .iter().map(|id|id.as_i64().ok_or_else(||Box::new(refused(StatusCode::SERVICE_UNAVAILABLE,"pdf_plan_unavailable"))))
                    .collect::<Result<Vec<_>,_>>()?;
                let results=parse_batch(&ids,pdf_permit.expect("write admission"),&state.pdf_workers,|id|one_parse(&state,&cookie,&slug,id))
                    .map_err(|()|Box::new(refused(StatusCode::SERVICE_UNAVAILABLE,"outcome_unknown")))?;
                let mut failed=Vec::new();
                // Publication results stay in plan order, regardless of child
                // completion order. Errors are returned only after all lanes drain.
                for (id,result) in ids.iter().zip(results) {
                    match result {Ok(value) if value["ok"]==true=>{},Ok(_)=>failed.push(*id),Err(r)=>return Err(r)}
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
    use std::sync::{
        Arc, Barrier, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    };
    use std::time::Duration;

    #[test]
    fn document_pdf_batch_four_overlapping_lanes_and_ordered_failures() {
        let slots = Arc::new(tokio::sync::Semaphore::new(4));
        let first = slots.clone().try_acquire_owned().unwrap();
        let gate = Arc::new(Barrier::new(5));
        let owner = Mutex::new(Vec::new());
        let (started, arrivals) = mpsc::channel();
        std::thread::scope(|scope| {
            let handle = scope.spawn(|| {
                parse_batch(&[1, 2, 3, 4], first, &slots, |id| {
                    // Model the two owner phases around a blocked child. The test
                    // thread can acquire this mutex while all four children run.
                    owner.lock().unwrap().push((id, "prepare"));
                    started.send(id).unwrap();
                    gate.wait();
                    owner.lock().unwrap().push((id, "finish"));
                    if id == 2 { Err("malformed") } else { Ok(id) }
                })
            });
            for _ in 0..4 {
                arrivals
                    .recv_timeout(Duration::from_secs(10))
                    .expect("four live lanes");
            }
            assert_eq!(slots.available_permits(), 0);
            assert_eq!(
                owner
                    .try_lock()
                    .expect("owner released during children")
                    .len(),
                4
            );
            gate.wait();
            assert_eq!(
                handle.join().unwrap().unwrap(),
                vec![Ok(1), Err("malformed"), Ok(3), Ok(4)]
            );
        });
        assert_eq!(owner.lock().unwrap().len(), 8);
        assert_eq!(slots.available_permits(), 4);
    }

    #[test]
    fn document_pdf_batch_competing_requests_share_four_global_slots() {
        let slots = Arc::new(tokio::sync::Semaphore::new(4));
        // Both requests are admitted before either fans out.
        let first = slots.clone().try_acquire_owned().unwrap();
        let second = slots.clone().try_acquire_owned().unwrap();
        let gate = Barrier::new(5);
        let active = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let calls = AtomicUsize::new(0);
        let (started, arrivals) = mpsc::channel();
        std::thread::scope(|scope| {
            let parse = |id| {
                let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(current, Ordering::SeqCst);
                started.send(id).unwrap();
                // Block the first four calls, independent of lane assignment.
                if calls.fetch_add(1, Ordering::SeqCst) < 4 {
                    gate.wait();
                }
                active.fetch_sub(1, Ordering::SeqCst);
                id
            };
            // Four ids in each request; the first global wave blocks.
            let shared_slots = &slots;
            let a = scope.spawn(move || parse_batch(&[0, 4, 5, 6], first, shared_slots, parse));
            let b = scope.spawn(move || parse_batch(&[1, 2, 3, 7], second, shared_slots, parse));
            for _ in 0..4 {
                arrivals
                    .recv_timeout(Duration::from_secs(10))
                    .expect("global overlap");
            }
            assert_eq!(active.load(Ordering::SeqCst), 4);
            assert_eq!(slots.available_permits(), 0);
            gate.wait();
            assert_eq!(a.join().unwrap().unwrap(), vec![0, 4, 5, 6]);
            assert_eq!(b.join().unwrap().unwrap(), vec![1, 2, 3, 7]);
        });
        assert_eq!(peak.load(Ordering::SeqCst), 4);
        assert_eq!(active.load(Ordering::SeqCst), 0);
        assert_eq!(slots.available_permits(), 4);
    }

    #[test]
    fn document_pdf_batch_panic_drains_successes_without_replay_and_releases_permits() {
        let slots = Arc::new(tokio::sync::Semaphore::new(4));
        let first = slots.clone().try_acquire_owned().unwrap();
        let gate = Barrier::new(4);
        let effects = Mutex::new(Vec::new());
        let result = parse_batch(&[1, 2, 3, 4, 5, 6], first, &slots, |id| {
            if id <= 4 {
                gate.wait();
            }
            if id == 2 {
                panic!("synthetic lane panic");
            }
            effects.lock().unwrap().push(id);
        });
        assert!(result.is_err());
        let mut effects = effects.into_inner().unwrap();
        effects.sort();
        assert_eq!(effects, vec![1, 3, 4, 5, 6]);
        assert_eq!(slots.available_permits(), 4);
    }

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
