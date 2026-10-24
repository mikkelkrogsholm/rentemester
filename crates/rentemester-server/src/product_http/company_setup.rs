use super::*;
use rentemester_application::company_setup::{Account, Action, Role};
enum Command {
    Accounts,
    Profile,
    Create(Account),
    Confirm(Role),
    Update(serde_json::Map<String, Value>),
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    // GET-backed HEAD must never select account creation or a company update.
    if request.method() == axum::http::Method::HEAD {
        let mut response = refused(StatusCode::METHOD_NOT_ALLOWED, "method_not_allowed");
        response.headers_mut().insert("allow", axum::http::HeaderValue::from_static("GET"));
        return response;
    }
    let company_slug=request.uri().path().split('/').nth(3).unwrap_or("").to_owned();
    let mutation = request.method() != axum::http::Method::GET;
    let cookie = match proof(&state, &request, mutation) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    if !selected(&state, &request) || request.uri().query().is_some() {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let path = request.uri().path().to_owned();
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let command = if mutation {
        if request.headers().contains_key("content-encoding")
            || !one(request.headers(), "content-type")
                .is_some_and(|s| s.eq_ignore_ascii_case("application/json"))
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        let Ok(bytes) = to_bytes(request.into_body(), 16 * 1024).await else {
            return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
        };
        if path.ends_with("/accounts") {
            let Ok(input) = serde_json::from_slice(&bytes) else {
                return refused(StatusCode::BAD_REQUEST, "invalid_input");
            };
            Command::Create(input)
        } else if path.ends_with("/account-roles") {
            let Ok(input) = serde_json::from_slice(&bytes) else {
                return refused(StatusCode::BAD_REQUEST, "invalid_input");
            };
            Command::Confirm(input)
        } else {
            let Ok(input) = serde_json::from_slice(&bytes) else {
                return refused(StatusCode::BAD_REQUEST, "invalid_input");
            };
            Command::Update(input)
        }
    } else if path.ends_with("/accounts") {
        Command::Accounts
    } else {
        Command::Profile
    };
    let key = match command {
        Command::Accounts => "accounts",
        Command::Profile | Command::Update(_) => "company",
        Command::Create(_) => "account",
        Command::Confirm(_) => "resolution",
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let action = match &command {
            Command::Accounts => Action::Accounts,
            Command::Profile => Action::Profile,
            Command::Create(input) => Action::Create(input),
            Command::Confirm(input) => Action::Confirm(input),
            Command::Update(input) => Action::Update(input),
        };
        let mut owner = state.native.lock_company(&company_slug).map_err(|_| ())?;
        owner
            .company_setup(&cookie, &SystemClock, action)
            .map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(attempt)) => match super::documents::outcome(attempt) {
            Ok(value) => response(StatusCode::OK, json!({"ok":true,key:value})),
            Err(r) => *r,
        },
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unconfirmed"),
    }
}
