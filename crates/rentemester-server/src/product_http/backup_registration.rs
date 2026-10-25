//! Owner metadata and declarations; never dereference a registered location.
use super::*;
use rentemester_application::backup_registration::{Action, Request as Input};
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let mutation = request.method() == axum::http::Method::POST;
    let cookie = match proof(&state, &request, mutation) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    if request.uri().query().is_some() || request.headers().contains_key("content-encoding") {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let path = request.uri().path().to_owned();
    let slug = path.split('/').nth(3).unwrap_or("").to_owned();
    let operation = path.split('/').nth(6).map(str::to_owned);
    if operation
        .as_deref()
        .is_some_and(|id| !rentemester_runtime::backup_destination::valid_id(id))
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let input = if mutation {
        if !one(request.headers(), "content-type")
            .is_some_and(|value| value.eq_ignore_ascii_case("application/json"))
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        let Ok(bytes) = to_bytes(request.into_body(), 16 * 1024).await else {
            return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
        };
        let Ok(value) = serde_json::from_slice::<Input>(&bytes) else {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        };
        Some(value)
    } else {
        if to_bytes(request.into_body(), 0).await.is_err() {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        None
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let action = match (&input, &operation) {
            (Some(input), _) => Action::Apply(input),
            (_, Some(operation)) => Action::Inspect { operation },
            _ => Action::List,
        };
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        owner
            .backup_registration(
                &cookie,
                &SystemClock,
                &state.native.backup_destinations,
                action,
            )
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
            "backup_registration_unavailable",
        ),
    }
}
mod original717;
pub(super) async fn original717(state: State<Product>, request: Request) -> Response {
    original717::handle(state, request).await
}
