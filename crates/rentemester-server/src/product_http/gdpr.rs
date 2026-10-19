use super::*;
use rentemester_application::gdpr::Action;
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let mutation = request.method() != axum::http::Method::GET;
    let erase = request.uri().path().ends_with("/erase");
    let cookie = match proof(&state, &request, mutation) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    if request.uri().query().is_some() {
        return refused(StatusCode::BAD_REQUEST, "invalid_query");
    }
    let input = if mutation {
        if request.headers().contains_key("content-encoding")
            || !one(request.headers(), "content-type")
                .is_some_and(|v| v.eq_ignore_ascii_case("application/json"))
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        let Ok(bytes) = to_bytes(request.into_body(), 16 * 1024).await else {
            return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
        };
        let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        };
        if erase && value.get("asOf").is_some() {
            return refused(StatusCode::BAD_REQUEST, "erasure_does_not_accept_as_of");
        }
        let Ok(input) = serde_json::from_value::<rentemester_storage::gdpr::Input>(value) else {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        };
        Some(input)
    } else {
        None
    };
    let Ok(now) = rentemester_runtime::auth::Clock::now_unix_ms(&SystemClock) else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "clock_unavailable");
    };
    let Ok(today) = rentemester_runtime::posting_time::latest_date(now, 0) else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "clock_unavailable");
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.owner.lock().map_err(|_| ())?;
        let action = match &input {
            Some(i) if erase => Action::Erase(i),
            Some(i) => Action::Export(i),
            None => Action::Retention,
        };
        owner
            .gdpr(&cookie, &SystemClock, &today, action)
            .map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(attempt)) => match super::documents::outcome(attempt) {
            Ok(value) => response(
                StatusCode::OK,
                json!({"ok":true,(if !mutation{"retention"}else if erase{"gdprErasure"}else{"gdpr"}):value}),
            ),
            Err(r) => *r,
        },
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unconfirmed"),
    }
}
