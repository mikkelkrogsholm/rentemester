use super::*;
use rentemester_application::assets::Action;
enum Command {
    List,
    Next(i64),
    Register(rentemester_storage::assets::Register),
    Depreciate(i64, rentemester_storage::assets::Depreciate),
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let mutation = request.method() != axum::http::Method::GET;
    let cookie = match proof(&state, &request, mutation) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    let Ok(now) = rentemester_runtime::auth::Clock::now_unix_ms(&SystemClock) else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "clock_unavailable");
    };
    let Ok(today) = rentemester_runtime::posting_time::latest_date(now, 0) else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "clock_unavailable");
    };
    if request.uri().query().is_some() {
        return refused(StatusCode::BAD_REQUEST, "invalid_query");
    }
    let path = request.uri().path().to_owned();
    let id = || {
        path.rsplit('/')
            .nth(1)
            .and_then(|s| s.parse::<i64>().ok())
            .filter(|n| *n > 0)
    };
    let command = if mutation {
        if request.headers().contains_key("content-encoding")
            || !one(request.headers(), "content-type")
                .is_some_and(|s| s.eq_ignore_ascii_case("application/json"))
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        let Ok(bytes) = to_bytes(request.into_body(), 256 * 1024).await else {
            return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
        };
        if path.ends_with("/depreciate") {
            let Some(id) = id() else {
                return refused(StatusCode::BAD_REQUEST, "invalid_input");
            };
            match serde_json::from_slice(&bytes) {
                Ok(input) => Command::Depreciate(id, input),
                Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
            }
        } else {
            match serde_json::from_slice(&bytes) {
                Ok(input) => Command::Register(input),
                Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
            }
        }
    } else if path.ends_with("/next-depreciation") {
        let Some(id) = id() else {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        };
        Command::Next(id)
    } else {
        Command::List
    };
    let key = match command {
        Command::List => "assets",
        Command::Next(_) => "nextDepreciation",
        Command::Register(_) => "asset",
        Command::Depreciate(_, _) => "depreciation",
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.owner.lock().map_err(|_| ())?;
        let action = match &command {
            Command::List => Action::List,
            Command::Next(id) => Action::Next(*id),
            Command::Register(i) => Action::Register(i),
            Command::Depreciate(id, i) => Action::Depreciate(*id, i),
        };
        owner
            .assets(&cookie, &SystemClock, action, &today)
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
