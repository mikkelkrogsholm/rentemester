//! Bounded explicit scheduling operations; GET/reopen never dispatch or initialize schema.
use super::*;
use rentemester_application::scheduled_recurring::Action;
use rentemester_engine::scheduled_recurring as engine;
enum Command {
    List,
    Plan(i64),
    Enable(engine::Enable),
    Pause(engine::Change),
    Resume(engine::Change),
    Cancel(engine::Change),
    CatchUp(engine::Selection),
    Skip(engine::Skip),
    Dispatch(engine::Dispatch),
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let path = request.uri().path().to_owned();
    let slug = path.split('/').nth(3).unwrap_or("").to_owned();
    let write = request.method() != axum::http::Method::GET;
    let cookie = match proof(&state, &request, write) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    if request.uri().query().is_some() {
        return refused(StatusCode::BAD_REQUEST, "invalid_query");
    }
    let command = if write {
        if request.headers().contains_key("content-encoding")
            || !one(request.headers(), "content-type")
                .is_some_and(|s| s.eq_ignore_ascii_case("application/json"))
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        let Ok(bytes) = to_bytes(request.into_body(), 16 * 1024).await else {
            return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
        };
        let action = path.rsplit('/').next();
        let parsed = match action {
            Some("enable") => serde_json::from_slice(&bytes).map(Command::Enable),
            Some("pause") => serde_json::from_slice(&bytes).map(Command::Pause),
            Some("resume") => serde_json::from_slice(&bytes).map(Command::Resume),
            Some("cancel") => serde_json::from_slice(&bytes).map(Command::Cancel),
            Some("catch-up") => serde_json::from_slice(&bytes).map(Command::CatchUp),
            Some("skip") => serde_json::from_slice(&bytes).map(Command::Skip),
            Some("dispatch") => serde_json::from_slice(&bytes).map(Command::Dispatch),
            _ => return refused(StatusCode::NOT_FOUND, "not_found"),
        };
        match parsed {
            Ok(v) => v,
            Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
        }
    } else if path.ends_with("/plan") {
        let Some(id) = path
            .rsplit('/')
            .nth(1)
            .and_then(|v| v.parse::<i64>().ok())
            .filter(|id| *id > 0)
        else {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        };
        Command::Plan(id)
    } else {
        Command::List
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let now = rentemester_runtime::auth::Clock::now_unix_ms(&SystemClock).map_err(|_| ())?;
        let today = rentemester_runtime::posting_time::latest_date(now, 0).map_err(|_| ())?;
        let at = chrono::DateTime::from_timestamp_millis(now)
            .ok_or(())?
            .to_rfc3339();
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        let action = match &command {
            Command::List => Action::List,
            Command::Plan(id) => Action::Plan(*id),
            Command::Enable(i) => Action::Enable(i),
            Command::Pause(i) => Action::Pause(i),
            Command::Resume(i) => Action::Resume(i),
            Command::Cancel(i) => Action::Cancel(i),
            Command::CatchUp(i) => Action::CatchUp(i),
            Command::Skip(i) => Action::Skip(i),
            Command::Dispatch(i) => Action::Dispatch(i),
        };
        owner
            .scheduled_recurring(&cookie, &SystemClock, action, &today, &at)
            .map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(attempt)) => match super::documents::outcome(attempt) {
            Ok(mut v) => {
                v["ok"] = json!(true);
                response(StatusCode::OK, v)
            }
            Err(r) => *r,
        },
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unconfirmed"),
    }
}
