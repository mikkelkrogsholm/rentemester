//! Explicit readonly calculation and distinct confirmed claim writes.
use super::*;
use rentemester_application::invoice_compensation::Action;
enum Command {
    Calculate(rentemester_storage::invoice_compensation::Calculate),
    Register(rentemester_storage::invoice_compensation::Register),
    Post(rentemester_storage::invoice_compensation::Post),
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    // POST calculation is a read in the ledger, but still uses ordinary cookie
    // Origin protection and never accepts service credentials as human proof.
    let cookie = match proof(&state, &request, true) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    if request.method() != axum::http::Method::POST
        || request.uri().query().is_some()
        || request.headers().contains_key("content-encoding")
        || request.headers().contains_key("idempotency-key")
        || !one(request.headers(), "content-type")
            .is_some_and(|value| value.eq_ignore_ascii_case("application/json"))
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let path = request.uri().path().to_owned();
    let slug = path.split('/').nth(3).unwrap_or("").to_owned();
    let Ok(bytes) = to_bytes(request.into_body(), 16 * 1024).await else {
        return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
    };
    let command = match path.rsplit('/').next() {
        Some("calculate") => serde_json::from_slice(&bytes).map(Command::Calculate),
        Some("register") => serde_json::from_slice(&bytes).map(Command::Register),
        Some("post") => serde_json::from_slice(&bytes).map(Command::Post),
        _ => return refused(StatusCode::NOT_FOUND, "not_found"),
    };
    let Ok(command) = command else {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        let action = match &command {
            Command::Calculate(input) => Action::Calculate(input),
            Command::Register(input) => Action::Register(input),
            Command::Post(input) => Action::Post(input),
        };
        owner
            .invoice_compensation(&cookie, &SystemClock, action)
            .map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(attempt)) => match super::documents::outcome(attempt) {
            Ok(value) => response(StatusCode::OK, json!({"ok":true,"compensation":value})),
            Err(response) => *response,
        },
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unconfirmed"),
    }
}
