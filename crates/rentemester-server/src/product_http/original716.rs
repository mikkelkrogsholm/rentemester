//! Original structure overview only. No operator file or arbitrary-path endpoint.
pub(crate) mod custody;
pub(crate) mod report;
use super::*;
/// Register GET /api/group/overview; human current-session authorization is Owner-owned.
pub(super) async fn group_overview(State(state): State<Product>, request: Request) -> Response {
    let cookie = match proof(&state, &request, false) { Ok(v) => v, Err(r) => return *r };
    if request.method() != axum::http::Method::GET
        || request.headers().contains_key("content-encoding")
        || request.headers().contains_key("transfer-encoding")
        || request.headers().contains_key("idempotency-key")
        || one(request.headers(), "content-length").is_some_and(|v| v != "0") {
        return refused(StatusCode::BAD_REQUEST, "invalid_query");
    }
    let pairs: Vec<_> = url::form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes()).collect();
    if pairs.len() != 1 || pairs[0].0 != "asOf"
        || rentemester_engine::fiscal_year::Date::parse(&pairs[0].1).is_err() {
        return refused(StatusCode::BAD_REQUEST, "invalid_query");
    }
    let date = pairs[0].1.to_string();
    if !matches!(to_bytes(request.into_body(), 0).await, Ok(b) if b.is_empty()) {
        return refused(StatusCode::BAD_REQUEST, "body_not_allowed");
    }
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        state.native.owner.lock().map_err(|_| ())?
            .group_overview_original716(&state.native.cookies, &SystemClock, &cookie, &date)
            .map_err(|_| ())
    }).await;
    match result {
        Ok(Ok(attempt)) if !attempt.retired => match attempt.outcome {
            Ok(value) => response(StatusCode::OK, value),
            Err(f) => refused(match f.error.code {
                "invalid_credentials" => StatusCode::UNAUTHORIZED,
                "access_denied" => StatusCode::FORBIDDEN,
                _ => StatusCode::SERVICE_UNAVAILABLE,
            }, f.error.code),
        },
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable"),
    }
}
