use super::*;
use rentemester_application::reminder_claims::Action;
enum Command {
    Read(i64),
    Register(rentemester_storage::reminder_claims::Register),
    Post(rentemester_storage::reminder_claims::Post),
    Settle(rentemester_storage::reminder_claims::Settle),
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let slug = request
        .uri()
        .path()
        .split('/')
        .nth(3)
        .unwrap_or("")
        .to_owned();
    let path = request.uri().path().to_owned();
    let mutation = request.method() != axum::http::Method::GET;
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
        if path.ends_with("/register") {
            match serde_json::from_slice(&bytes) {
                Ok(i) => Command::Register(i),
                Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
            }
        } else if path.ends_with("/post") {
            match serde_json::from_slice(&bytes) {
                Ok(i) => Command::Post(i),
                Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
            }
        } else {
            match serde_json::from_slice(&bytes) {
                Ok(i) => Command::Settle(i),
                Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
            }
        }
    } else {
        let Some(id) = path
            .rsplit('/')
            .nth(1)
            .and_then(|s| s.parse::<i64>().ok())
            .filter(|id| (1..=9007199254740991).contains(id))
        else {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        };
        Command::Read(id)
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
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        let action = match &command {
            Command::Read(id) => Action::Read(*id),
            Command::Register(i) => Action::Register(i),
            Command::Post(i) => Action::Post(i),
            Command::Settle(i) => Action::Settle(i),
        };
        owner
            .reminder_claims(&cookie, &SystemClock, action, &today)
            .map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(attempt)) => match super::documents::outcome(attempt) {
            Ok(value) => response(StatusCode::OK, json!({"ok":true,"reminderClaims":value})),
            Err(r) => *r,
        },
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unconfirmed"),
    }
}
