//! Explicit service adapters; human-only product routes retain their cookie proof.
use super::*;
use rentemester_application::{
    archive_reports::Action,
    service_company::{BusinessStage, ServiceFailure, ServiceResult, UsageStage},
};
use rentemester_runtime::service_key::ServiceKey;

pub(super) fn requested(request: &Request) -> bool {
    request.headers().contains_key("x-rentemester-service-key")
}
pub(super) fn proof(state: &Product, request: &Request, mutation: bool) -> Result<ServiceKey, Box<Response>> {
    origin_proof(state, request, mutation)?;
    let headers = request.headers();
    if headers.contains_key("cookie") || headers.contains_key("authorization") {
        return Err(Box::new(refused(StatusCode::UNAUTHORIZED, "unauthorized")));
    }
    let Some(raw) = one(headers, "x-rentemester-service-key")
        .filter(|v| v.len() <= 4096 && !v.chars().any(char::is_control))
    else {
        return Err(Box::new(refused(StatusCode::UNAUTHORIZED, "unauthorized")));
    };
    ServiceKey::new(state.native.cookies.workspace(), raw.trim())
        .map_err(|_| Box::new(refused(StatusCode::UNAUTHORIZED, "unauthorized")))
}
fn read_headers(request: &Request) -> bool {
    request.method() == axum::http::Method::GET
        && !request.headers().contains_key("content-encoding")
        && !request.headers().contains_key("transfer-encoding")
        && !request.headers().contains_key("idempotency-key")
        && one(request.headers(), "content-length").is_none_or(|v| v == "0")
}
pub(super) fn failure(f: ServiceFailure) -> Response {
    let code = f.error.code;
    let status = if f.usage == UsageStage::OutcomeUnknown
        || f.business == BusinessStage::OutcomeUnknown
        || !f.control_reusable
        || !f.company_reusable
    {
        StatusCode::SERVICE_UNAVAILABLE
    } else if code == "invalid_credentials" {
        StatusCode::UNAUTHORIZED
    } else if code == "access_denied" {
        StatusCode::FORBIDDEN
    } else if f.rejection.is_some() || f.business == BusinessStage::NotCommitted {
        StatusCode::UNPROCESSABLE_ENTITY
    } else {
        StatusCode::BAD_REQUEST
    };
    let mut value = f
        .rejection
        .unwrap_or_else(|| json!({"ok":false,"code":code,"errors":[code]}));
    value["usage"] = json!(f.usage);
    value["business"] = json!(f.business);
    response(status, value)
}
fn mutation_result(result: ServiceResult) -> Response {
    let mut value = result.result;
    if let Some(receipt) = result.receipt {
        value["receipt"] = json!(receipt);
    }
    value["usage"] = json!(result.usage);
    value["business"] = json!(result.business);
    response(StatusCode::OK, value)
}
pub(super) async fn companies(state: Product, request: Request) -> Response {
    let key = match proof(&state, &request, false) {
        Ok(k) => k,
        Err(r) => return *r,
    };
    if request.uri().query().is_some() || !read_headers(&request) {
        return refused(StatusCode::BAD_REQUEST, "invalid_query");
    }
    if !matches!(to_bytes(request.into_body(), 0).await, Ok(b) if b.is_empty()) {
        return refused(StatusCode::BAD_REQUEST, "body_not_allowed");
    }
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.owner.lock().map_err(|_| ())?;
        owner.service_context(&SystemClock, &key).map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(attempt)) => match attempt.outcome {
            Ok(context) => match serde_json::from_str::<Value>(context.as_json()) {
                Ok(value) => response(
                    StatusCode::OK,
                    json!({"ok":true,"companies":value["companies"],"count":value["companies"].as_array().map(Vec::len)}),
                ),
                Err(_) => refused(StatusCode::SERVICE_UNAVAILABLE, "read_unconfirmed"),
            },
            Err(f) if f.error.code == "invalid_credentials" => {
                refused(StatusCode::UNAUTHORIZED, "unauthorized")
            }
            Err(_) => refused(StatusCode::SERVICE_UNAVAILABLE, "context_unconfirmed"),
        },
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable"),
    }
}
pub(super) async fn read(state: Product, request: Request) -> Response {
    let key = match proof(&state, &request, false) {
        Ok(k) => k,
        Err(r) => return *r,
    };
    if !read_headers(&request) {
        return refused(StatusCode::BAD_REQUEST, "invalid_query");
    }
    let Some((resource, envelope)) = archive_reports::resource(request.uri().path()) else {
        return refused(StatusCode::NOT_FOUND, "not_found");
    };
    let query = match archive_reports::parse(request.uri().query().unwrap_or(""), resource) {
        Ok(q) => q,
        Err(()) => return refused(StatusCode::BAD_REQUEST, "invalid_query"),
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    let slug = request
        .uri()
        .path()
        .split('/')
        .nth(3)
        .unwrap_or("")
        .to_owned();
    if !matches!(to_bytes(request.into_body(), 0).await, Ok(b) if b.is_empty()) {
        return refused(StatusCode::BAD_REQUEST, "body_not_allowed");
    }
    let today = match rentemester_runtime::auth::Clock::now_unix_ms(&SystemClock)
        .ok()
        .and_then(|now| rentemester_runtime::posting_time::latest_date(now, 0).ok())
    {
        Some(v) => v,
        None => return refused(StatusCode::SERVICE_UNAVAILABLE, "clock_unavailable"),
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        let action = match resource {
            Some(resource) => Action::Read(rentemester_application::archive_reports::Request {
                today: &today,
                year: query.year.as_deref(),
                source: query.source.as_deref(),
                account: query.account.as_deref(),
                as_of: query.as_of.as_deref(),
                resource,
            }),
            None => Action::FiscalYears { today: &today },
        };
        owner
            .archive_reports_service(&key, &SystemClock, action)
            .map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(attempt)) => match attempt.outcome {
            Ok(value) => response(
                StatusCode::OK,
                json!({"ok":true,envelope:value.result,"usage":value.usage,"business":value.business}),
            ),
            Err(f) => failure(f),
        },
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable"),
    }
}
pub(super) async fn write(state: Product, request: Request) -> Response {
    let key = match proof(&state, &request, true) {
        Ok(k) => k,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    if request.uri().query().is_some()
        || request.headers().contains_key("content-encoding")
        || !one(request.headers(), "content-type").is_some_and(|s| {
            s.eq_ignore_ascii_case("application/json")
                || s.eq_ignore_ascii_case("application/json; charset=utf-8")
        })
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
    let reversal = request.uri().path().ends_with("/manual-journal-reversal");
    let bytes = match to_bytes(request.into_body(), 65_536).await {
        Ok(b) => b,
        Err(_) => return refused(StatusCode::PAYLOAD_TOO_LARGE, "body_too_large"),
    };
    let command = if reversal {
        let wire: PostWire<journal_reversal::Input> = match serde_json::from_slice(&bytes) {
            Ok(w) => w,
            Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
        };
        if !wire.confirm || wire.input.idempotency_key.is_none() {
            return refused(StatusCode::BAD_REQUEST, "confirmation_and_key_required");
        }
        if rentemester_application::service_company::validate_reversal_input(&wire.input).is_err() {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        Mutation::Reverse(wire.input)
    } else {
        let wire: PostWire<journal_post::Input> = match serde_json::from_slice(&bytes) {
            Ok(w) => w,
            Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
        };
        if !wire.confirm || wire.input.idempotency_key.is_none() {
            return refused(StatusCode::BAD_REQUEST, "confirmation_and_key_required");
        }
        if rentemester_application::service_company::validate_post_input(&wire.input).is_err() {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        Mutation::Post(wire.input)
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        match command {
            Mutation::Post(input) => owner.post_service(
                &key,
                &SystemClock,
                journal_post::Policy::default().max_future_days,
                input,
            ),
            Mutation::Reverse(input) => owner.reverse_service(
                &key,
                &SystemClock,
                journal_reversal::Policy::default().max_future_days,
                input,
            ),
        }
        .map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(attempt)) => match attempt.outcome {
            Ok(value) => mutation_result(value),
            Err(f) => failure(f),
        },
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unconfirmed"),
    }
}
