//! Company-scoped read over current retained bytes and historical placement evidence.
use super::*;
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let cookie = match proof(&state, &request, false) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    if request.uri().query().is_some() || request.headers().contains_key("content-encoding") {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let slug = request
        .uri()
        .path()
        .split('/')
        .nth(3)
        .unwrap_or("")
        .to_owned();
    if to_bytes(request.into_body(), 0).await.is_err() {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        owner
            .backup_governance(&cookie, &SystemClock, &state.native.backup_destinations)
            .map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(attempt)) => match super::documents::outcome(attempt) {
            Ok(value) => response(StatusCode::OK, value),
            Err(response) => *response,
        },
        _ => refused(
            StatusCode::SERVICE_UNAVAILABLE,
            "backup_governance_unavailable",
        ),
    }
}
pub(super) async fn original717(state: State<Product>, request: Request) -> Response {
    super::backup_registration::original717(state, request).await
}
