//! POST /api/companies/{company}/original716-compliance-report; no server file output.
use crate::product_http::*;
pub(crate) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let slug = request
        .uri()
        .path()
        .split('/')
        .nth(3)
        .unwrap_or("")
        .to_owned();
    let operation = request
        .uri()
        .path()
        .rsplit('/')
        .next()
        .unwrap_or("")
        .strip_prefix("original716-")
        .unwrap_or("")
        .to_owned();
    let cookie = match proof(&state, &request, true) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    if request.uri().query().is_some()
        || request.headers().contains_key("content-encoding")
        || !one(request.headers(), "content-type")
            .is_some_and(|v| v.eq_ignore_ascii_case("application/json"))
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let Ok(bytes) = to_bytes(request.into_body(), 16 * 1024).await else {
        return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
    };
    let Ok(input) = serde_json::from_slice::<Value>(&bytes) else {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    };
    if !input.is_object()
        || input.as_object().unwrap().iter().any(|(k, v)| {
            !if operation == "compliance-report" {
                ["asOf", "generatedAt", "commitSha", "expectedActor"].contains(&k.as_str())
            } else {
                ["asOf", "cvr", "name", "expectedActor", "confirm"].contains(&k.as_str())
            } || !(v.is_string() || k == "confirm" && v == true)
                || v.as_str().is_some_and(|s| s.len() > 256)
        })
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        if operation == "compliance-report" {
            owner
                .original716_compliance(&cookie, &SystemClock, &input)
                .map_err(|_| ())
        } else if ["retention", "export", "erase"].contains(&operation.as_str()) {
            owner
                .original716_remaining(&cookie, &SystemClock, &operation, &input)
                .map_err(|_| ())
        } else {
            Err(())
        }
    })
    .await;
    match result {
        Ok(Ok(attempt)) => match crate::product_http::documents::outcome(attempt) {
            Ok(value) => response(StatusCode::OK, json!({"ok":true,"result":value})),
            Err(r) => *r,
        },
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unconfirmed"),
    }
}
