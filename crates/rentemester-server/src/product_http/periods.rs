use super::*;
use rentemester_application::periods::Action;
enum Command {
    List,
    Readiness(String, String),
    Review(rentemester_storage::periods::Bounds),
    Close(rentemester_storage::periods::Close),
    Reopen(rentemester_storage::periods::Reopen),
    Report(rentemester_storage::periods::Report),
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let company_slug=request.uri().path().split('/').nth(3).unwrap_or("").to_owned();
    let mutation = request.method() != axum::http::Method::GET;
    let cookie = match proof(&state, &request, mutation) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    let path = request.uri().path().to_owned();
    let command = if mutation {
        if request.uri().query().is_some()
            || request.headers().contains_key("content-encoding")
            || !one(request.headers(), "content-type")
                .is_some_and(|s| s.eq_ignore_ascii_case("application/json"))
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        let Ok(bytes) = to_bytes(request.into_body(), 256 * 1024).await else {
            return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
        };
        match path.rsplit('/').next() {
            Some("close-review") => match serde_json::from_slice(&bytes) {
                Ok(i) => Command::Review(i),
                Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
            },
            Some("close") => match serde_json::from_slice(&bytes) {
                Ok(i) => Command::Close(i),
                Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
            },
            Some("report") => match serde_json::from_slice(&bytes) {
                Ok(i) => Command::Report(i),
                Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
            },
            Some("reopen") => match serde_json::from_slice(&bytes) {
                Ok(i) => Command::Reopen(i),
                Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
            },
            _ => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
        }
    } else if path.ends_with("/close-readiness") {
        let mut from = None;
        let mut to = None;
        for (key, value) in
            url::form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes())
        {
            let slot = match key.as_ref() {
                "from" => &mut from,
                "to" => &mut to,
                _ => return refused(StatusCode::BAD_REQUEST, "invalid_query"),
            };
            if slot.is_some() || value.len() > 10 {
                return refused(StatusCode::BAD_REQUEST, "invalid_query");
            }
            *slot = Some(value.into_owned());
        }
        let (Some(from), Some(to)) = (from, to) else {
            return refused(StatusCode::BAD_REQUEST, "invalid_query");
        };
        Command::Readiness(from, to)
    } else {
        if request.uri().query().is_some() {
            return refused(StatusCode::BAD_REQUEST, "invalid_query");
        }
        Command::List
    };
    let key = match command {
        Command::List => "periods",
        Command::Readiness(..) => "packet",
        Command::Review(_) => "review",
        _ => "period",
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let today = chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now())
        .format("%Y-%m-%d")
        .to_string();
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&company_slug).map_err(|_| ())?;
        let action = match &command {
            Command::List => Action::List,
            Command::Readiness(from, to) => Action::Readiness(from, to),
            Command::Review(i) => Action::Review(i),
            Command::Close(i) => Action::Close(i),
            Command::Reopen(i) => Action::Reopen(i),
            Command::Report(i) => Action::Report(i),
        };
        owner
            .periods(&cookie, &SystemClock, action, &today)
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
