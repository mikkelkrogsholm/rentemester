//! Human-reviewed delegation to the configured live executor. No product JSON carries a key.
use super::*;
use rentemester_application::hosted_recurring::Action;
use rentemester_engine::scheduled_recurring::{Change, HostedConsent};
enum Command {
    Plan(i64),
    Consent(HostedConsent),
    Revoke(Change),
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    if !matches!(
        *request.method(),
        axum::http::Method::GET | axum::http::Method::POST
    ) {
        return refused(StatusCode::METHOD_NOT_ALLOWED, "method_not_allowed");
    }
    let path = request.uri().path().to_owned();
    let slug = path.split('/').nth(3).unwrap_or("").to_owned();
    let write = request.method() != axum::http::Method::GET;
    let cookie = match proof(&state, &request, write) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    if request.uri().query().is_some() {
        return refused(StatusCode::BAD_REQUEST, "invalid_query");
    }
    let key = state.native.scheduled_executor.clone();
    let command = if write {
        if request.headers().contains_key("content-encoding")
            || !one(request.headers(), "content-type")
                .is_some_and(|s| s.eq_ignore_ascii_case("application/json"))
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        let Ok(bytes) = to_bytes(request.into_body(), 16 * 1024).await else {
            return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
        };
        let parsed = match path.rsplit('/').next() {
            Some("hosted-consent") => serde_json::from_slice(&bytes).map(Command::Consent),
            Some("hosted-revoke") => serde_json::from_slice(&bytes).map(Command::Revoke),
            _ => return refused(StatusCode::NOT_FOUND, "not_found"),
        };
        match parsed {
            Ok(c) => c,
            Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
        }
    } else {
        if request.headers().contains_key("content-encoding") {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        let id = path
            .rsplit('/')
            .nth(1)
            .and_then(|s| s.parse::<i64>().ok())
            .filter(|id| *id > 0);
        let Some(id) = id else {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        };
        match to_bytes(request.into_body(), 1).await {
            Ok(b) if b.is_empty() => {}
            _ => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
        }
        Command::Plan(id)
    };
    if !matches!(command, Command::Revoke(_)) && key.is_none() {
        return refused(
            StatusCode::UNPROCESSABLE_ENTITY,
            "hosted_executor_unavailable",
        );
    }
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        let action = match &command {
            Command::Plan(id) => Action::Plan(*id),
            Command::Consent(i) => Action::Consent(i),
            Command::Revoke(i) => Action::Revoke(i),
        };
        owner
            .hosted_recurring(&cookie, &SystemClock, key.as_deref(), action)
            .map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(attempt)) => match super::documents::outcome(attempt) {
            Ok(mut v) => {
                v["ok"] = json!(true);
                response(StatusCode::OK, v)
            }
            Err(r) => *r,
        },
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unconfirmed"),
    }
}
