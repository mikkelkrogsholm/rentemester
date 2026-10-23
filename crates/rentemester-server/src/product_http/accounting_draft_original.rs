//! Seven source accounting draft operations; decoding follows current company authority.
use super::*;
use rentemester_application::accounting_draft_original::{self as app, Operation};
fn domain_error(code: &str, unknown: bool) -> Response {
    if matches!(code, "invalid_credentials" | "access_denied") {
        return refused(
            if code == "invalid_credentials" {
                StatusCode::UNAUTHORIZED
            } else {
                StatusCode::FORBIDDEN
            },
            code,
        );
    }
    if unknown {
        return response(
            StatusCode::SERVICE_UNAVAILABLE,
            json!({"ok":false,"code":"outcome_unknown","outcome":"unknown","errors":["Write outcome unknown; read canonical accounting_draft before another action"]}),
        );
    }
    if code == "invalid_input" {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    refused(StatusCode::SERVICE_UNAVAILABLE, "operation_unconfirmed")
}
fn failure(error: &rentemester_engine::Error, unknown: bool) -> Response {
    if !unknown
        && error.code == "accounting_draft_original_business"
        && let Ok(value) = serde_json::from_str::<Value>(&error.message)
    {
        return response(StatusCode::UNPROCESSABLE_ENTITY, value);
    }
    domain_error(error.code, unknown)
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
        && matches!(f.error.code, "invalid_credentials" | "access_denied")
    {
        return Err(Box::new(domain_error(f.error.code, false)));
    }
    if attempt.retired {
        return Err(Box::new(domain_error("", true)));
    }
    match attempt.outcome {
        Ok(value) if value.control_reusable => Ok(value.value),
        Ok(_) => Err(Box::new(domain_error("", true))),
        Err(f) => Err(Box::new(failure(
            &f.error,
            f.stage == FailureStage::OutcomeUnknown || !f.control_reusable || !f.company_reusable,
        ))),
    }
}
fn service_checked<T>(
    attempt: rentemester_host::Attempt<
        Result<
            rentemester_application::service_company::ServiceResult<T>,
            rentemester_application::service_company::ServiceFailure,
        >,
    >,
) -> Result<T, Box<Response>> {
    use rentemester_application::service_company::{BusinessStage, UsageStage};
    if let Err(f) = &attempt.outcome
        && matches!(f.error.code, "invalid_credentials" | "access_denied")
    {
        return Err(Box::new(domain_error(f.error.code, false)));
    }
    if attempt.retired {
        return Err(Box::new(domain_error("", true)));
    }
    match attempt.outcome {
        Ok(value) if value.control_reusable => Ok(value.result),
        Ok(_) => Err(Box::new(domain_error("", true))),
        Err(f) => Err(Box::new(failure(
            &f.error,
            f.usage == UsageStage::OutcomeUnknown
                || f.business == BusinessStage::OutcomeUnknown
                || !f.control_reusable
                || !f.company_reusable,
        ))),
    }
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let slug = request
        .uri()
        .path()
        .split('/')
        .nth(3)
        .unwrap_or("")
        .to_owned();
    let operation = match request.uri().path().rsplit('/').next().unwrap_or("") {
        "create" => Operation::Create,
        "revise" => Operation::Revise,
        "submit" => Operation::Submit,
        "reject" => Operation::Reject,
        "approve-and-post" => Operation::ApproveAndPost,
        "show" => Operation::Show,
        "list" => Operation::List,
        _ => return refused(StatusCode::NOT_FOUND, "not_found"),
    };
    let post = app::write(operation);
    // GET may omit Origin; POST must prove same origin. Use the credential proofs
    // directly so structural errors still reach the held current-authority fence.
    let credential = if service_company::requested(&request) {
        service_company::proof(&state, &request, post).map(documents::Credential::Service)
    } else {
        proof(&state, &request, post).map(documents::Credential::Human)
    };
    let credential = match credential {
        Ok(c) => c,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    let query = request.uri().query().map(str::to_owned);
    let valid_transport = request.method()
        == if post {
            axum::http::Method::POST
        } else {
            axum::http::Method::GET
        }
        && !request.headers().contains_key("content-encoding")
        && !request.headers().contains_key("transfer-encoding")
        && !request.headers().contains_key("idempotency-key")
        && (!post
            || one(request.headers(), "content-type")
                .is_some_and(|v| v.eq_ignore_ascii_case("application/json")));
    // Even bounded-read failures are carried through the current authority fence.
    let bytes = to_bytes(request.into_body(), app::MAX_BODY).await;
    let valid_transport = valid_transport && bytes.as_ref().is_ok_and(|b| post || b.is_empty());
    let bytes = bytes.map(|b| b.to_vec()).unwrap_or_default();
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state
            .native
            .lock_company(&slug)
            .map_err(|_| Box::new(domain_error("", true)))?;
        let request = app::Request {
            body: &bytes,
            query: query.as_deref(),
            valid_transport,
        };
        match &credential {
            documents::Credential::Human(cookie) => checked(
                owner
                    .original_accounting_draft(cookie, &SystemClock, request, operation)
                    .map_err(|_| Box::new(domain_error("", true)))?,
            ),
            documents::Credential::Service(key) => service_checked(
                owner
                    .original_accounting_draft_service(key, &SystemClock, request, operation)
                    .map_err(|_| Box::new(domain_error("", true)))?,
            ),
        }
    })
    .await;
    match result {
        Ok(Ok(value)) => response(StatusCode::OK, value),
        Ok(Err(r)) => *r,
        _ => domain_error("", true),
    }
}
