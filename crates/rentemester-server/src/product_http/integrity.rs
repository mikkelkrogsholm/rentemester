use super::*;
pub(super) async fn read(State(state): State<Product>, request: Request) -> Response {
    if request.method() != axum::http::Method::GET {
        return refused(StatusCode::METHOD_NOT_ALLOWED, "method_not_allowed");
    }
    let cookie = match proof(&state, &request, false) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    if request.uri().query().is_some()
        || request.headers().contains_key("content-encoding")
        || request.headers().contains_key("content-length")
        || request.headers().contains_key("transfer-encoding")
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.owner.lock().map_err(|_| ())?;
        owner.integrity(&cookie, &SystemClock).map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(attempt)) => match super::documents::outcome(attempt) {
            Ok(integrity) => response(StatusCode::OK, json!({"ok":true,"integrity":integrity})),
            Err(r) => *r,
        },
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "integrity_unavailable"),
    }
}
