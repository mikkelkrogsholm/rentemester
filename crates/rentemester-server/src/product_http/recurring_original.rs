//! Source-shaped recurring contracts over the same admitted native company owner.
use super::documents::{Credential, credential};
use super::*;
use rentemester_application::recurring_invoices::Action;
use rentemester_storage::recurring_invoices::original;
enum Command {
    List(bool),
    Create(Box<original::Create>),
    Generate(original::Generate),
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let slug = request
        .uri()
        .path()
        .split('/')
        .nth(3)
        .unwrap_or("")
        .to_owned();
    let path = request.uri().path().to_owned();
    let mutation = request.method() != axum::http::Method::GET;
    let credential = match credential(&state, &request, mutation) {
        Ok(v) => v,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    let command = if !mutation {
        let include = match request.uri().query() {
            None | Some("includeInactive=false") => false,
            Some("includeInactive=true") => true,
            _ => return refused(StatusCode::BAD_REQUEST, "invalid_query"),
        };
        Command::List(include)
    } else {
        if request.uri().query().is_some()
            || request.headers().contains_key("content-encoding")
            || !one(request.headers(), "content-type")
                .is_some_and(|s| s.eq_ignore_ascii_case("application/json"))
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        let Ok(bytes) = to_bytes(request.into_body(), 256 * 1024).await else {
            return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
        };
        if path.ends_with("/create") {
            match serde_json::from_slice(&bytes) {
                Ok(v) => Command::Create(v),
                Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
            }
        } else {
            match serde_json::from_slice(&bytes) {
                Ok(v) => Command::Generate(v),
                Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
            }
        }
    };
    let Ok(ms) = rentemester_runtime::auth::Clock::now_unix_ms(&SystemClock) else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "clock_unavailable");
    };
    let Ok(today) = rentemester_runtime::posting_time::latest_date(ms, 0) else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "clock_unavailable");
    };
    let Ok(now) = rentemester_engine::saft::timestamp(ms) else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "clock_unavailable");
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        let action = match &command {
            Command::List(v) => Action::OriginalList(*v),
            Command::Create(v) => Action::OriginalCreate(v),
            Command::Generate(v) => Action::OriginalGenerate(v),
        };
        Ok::<_, ()>(match credential {
            Credential::Human(cookie) => super::documents::outcome(
                owner
                    .recurring_invoices(&cookie, &SystemClock, action, &today, &now)
                    .map_err(|_| ())?,
            ),
            Credential::Service(key) => super::documents::service_outcome(
                owner
                    .recurring_invoices_service(&key, &SystemClock, action, &today, &now)
                    .map_err(|_| ())?,
            ),
        })
    })
    .await;
    match result {
        Ok(Ok(Ok(value))) => response(StatusCode::OK, value),
        Ok(Ok(Err(r))) => *r,
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unconfirmed"),
    }
}
