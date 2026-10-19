use super::*;
use rentemester_application::dimensions::Action;
use rentemester_storage::dimensions::{AssignmentInput, MasterInput, Supersede};
enum Input {
    Master(MasterInput),
    Assignment(AssignmentInput),
    Supersede(Supersede),
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let company_slug=request.uri().path().split('/').nth(3).unwrap_or("").to_owned();
    let is_post = request.method() == axum::http::Method::POST;
    let operation = request
        .uri()
        .path()
        .rsplit('/')
        .next()
        .unwrap_or("")
        .to_owned();
    let cookie = match proof(&state, &request, is_post) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    let mut dimension = None;
    for (key, value) in url::form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes())
    {
        if is_post
            || operation != "members"
            || key != "dimensionId"
            || dimension.is_some()
            || value.is_empty()
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_query");
        }
        dimension = Some(value.into_owned());
    }
    let line = operation
        .parse::<i64>()
        .ok()
        .filter(|v| *v > 0 && *v <= 9_007_199_254_740_991);
    let input = if is_post {
        if request.uri().query().is_some()
            || request.headers().contains_key("content-encoding")
            || !one(request.headers(), "content-type")
                .is_some_and(|v| v.eq_ignore_ascii_case("application/json"))
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        let Ok(bytes) = to_bytes(request.into_body(), 128 * 1024).await else {
            return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
        };
        let parsed = match operation.as_str() {
            "define" | "member" | "definition-lifecycle" | "member-lifecycle" => {
                serde_json::from_slice(&bytes).map(Input::Master)
            }
            "plan" | "apply" | "replace" => serde_json::from_slice(&bytes).map(Input::Assignment),
            "supersede" => serde_json::from_slice(&bytes).map(Input::Supersede),
            _ => return refused(StatusCode::NOT_FOUND, "not_found"),
        };
        match parsed {
            Ok(v) => Some(v),
            Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
        }
    } else {
        if !["dimensions", "members"].contains(&operation.as_str()) && line.is_none() {
            return refused(StatusCode::BAD_REQUEST, "invalid_journal_line");
        }
        None
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&company_slug).map_err(|_| ())?;
        let action = match &input {
            Some(Input::Master(i)) => Action::Master(&operation, i),
            Some(Input::Assignment(i)) => match operation.as_str() {
                "plan" => Action::Plan(i),
                "apply" => Action::Apply(i),
                _ => Action::Replace(i),
            },
            Some(Input::Supersede(i)) => Action::Supersede(i),
            None => match operation.as_str() {
                "dimensions" => Action::Definitions,
                "members" => Action::Members(dimension.as_deref()),
                _ => Action::Assignments(line.unwrap()),
            },
        };
        owner
            .dimensions(&cookie, &SystemClock, action)
            .map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(attempt)) => match super::documents::outcome(attempt) {
            Ok(value) => response(StatusCode::OK, value),
            Err(r) => *r,
        },
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unconfirmed"),
    }
}
