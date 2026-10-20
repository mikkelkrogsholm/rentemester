//! Current-principal manual-post receipt read-back and explicit acknowledgement.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Acknowledge {
    confirm: bool,
    receipt_id: i64,
}
use super::*;
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let slug = request
        .uri()
        .path()
        .split('/')
        .nth(3)
        .unwrap_or("")
        .to_owned();
    let write = request.method() == axum::http::Method::POST;
    let cookie = match proof(&state, &request, write) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    if request.uri().query().is_some() || request.headers().contains_key("content-encoding") {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let input = if write {
        if !one(request.headers(), "content-type")
            .is_some_and(|v| v.eq_ignore_ascii_case("application/json"))
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        let Ok(bytes) = to_bytes(request.into_body(), 16 * 1024).await else {
            return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
        };
        match serde_json::from_slice::<Acknowledge>(&bytes) {
            Ok(v) if v.confirm && v.receipt_id > 0 && v.receipt_id <= 9_007_199_254_740_991 => {
                Some(v.receipt_id)
            }
            _ => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
        }
    } else {
        match to_bytes(request.into_body(), 1).await {
            Ok(v) if v.is_empty() => {}
            _ => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
        };
        None
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        owner
            .manual_post_recovery(&cookie, &SystemClock, input)
            .map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(attempt)) => match super::documents::outcome(attempt) {
            Ok(mut value) => {
                value["ok"] = json!(true);
                response(StatusCode::OK, value)
            }
            Err(r) => *r,
        },
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unknown"),
    }
}
