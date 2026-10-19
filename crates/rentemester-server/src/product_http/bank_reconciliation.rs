use super::*;
use rentemester_application::bank_reconciliation::Action;
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let mutation = request.method() == axum::http::Method::POST;
    let cookie = match proof(&state, &request, mutation) {
        Ok(v) => v,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    let mut bank = None;
    for (key, value) in url::form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes())
    {
        if mutation
            || key != "bankTransactionId"
            || bank.is_some()
            || !value.bytes().all(|b| b.is_ascii_digit())
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_query");
        }
        bank = value
            .parse::<i64>()
            .ok()
            .filter(|n| *n > 0 && *n <= 9_007_199_254_740_991);
        if bank.is_none() {
            return refused(StatusCode::BAD_REQUEST, "invalid_query");
        }
    }
    if !mutation && bank.is_none() {
        return refused(StatusCode::BAD_REQUEST, "invalid_query");
    }
    let input = if mutation {
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
        match serde_json::from_slice::<rentemester_storage::bank_reconciliation::Apply>(&bytes) {
            Ok(v) => Some(v),
            Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
        }
    } else {
        None
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.owner.lock().map_err(|_| ())?;
        let action = match &input {
            Some(i) => Action::Apply(i),
            None => Action::Candidates(bank.unwrap()),
        };
        owner
            .bank_reconciliation(&cookie, &SystemClock, action)
            .map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(attempt)) => match super::documents::outcome(attempt) {
            Ok(v) => response(
                StatusCode::OK,
                json!({"ok":true,if mutation{"reconciliation"}else{"reconciliationCandidates"}:v}),
            ),
            Err(r) => *r,
        },
        _ => refused(
            StatusCode::SERVICE_UNAVAILABLE,
            if mutation {
                "outcome_unconfirmed"
            } else {
                "read_unconfirmed"
            },
        ),
    }
}
