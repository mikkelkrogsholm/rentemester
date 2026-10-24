use super::*;
use rentemester_application::supplier_commitments::Action;
use rentemester_storage::supplier_commitments as storage;
enum Command {
    List(String),
    Matches,
    Plan(storage::Plan),
    Apply(storage::Apply),
    Change(storage::Change),
    Match(storage::Match),
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    // GET-backed HEAD must refuse before credentials, body parsing or mutation dispatch.
    if request.method() == axum::http::Method::HEAD {
        let mut response = refused(StatusCode::METHOD_NOT_ALLOWED, "method_not_allowed");
        response.headers_mut().insert("allow", axum::http::HeaderValue::from_static("GET"));
        return response;
    }
    let company_slug=request.uri().path().split('/').nth(3).unwrap_or("").to_owned();
    let post = request.method() != axum::http::Method::GET;
    let cookie = match proof(&state, &request, post) {
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
    let path = request.uri().path().to_owned();
    let command = if post {
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
        macro_rules! parse {
            ($t:ty,$variant:ident) => {
                match serde_json::from_slice::<$t>(&bytes) {
                    Ok(i) => Command::$variant(i),
                    Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
                }
            };
        }
        if path.ends_with("/plan") {
            parse!(storage::Plan, Plan)
        } else if path.ends_with("/change") {
            parse!(storage::Change, Change)
        } else if path.ends_with("/match") {
            parse!(storage::Match, Match)
        } else {
            parse!(storage::Apply, Apply)
        }
    } else if path.ends_with("/matches") {
        if request.uri().query().is_some() {
            return refused(StatusCode::BAD_REQUEST, "invalid_query");
        }
        Command::Matches
    } else {
        let mut start = None;
        for (key, value) in
            url::form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes())
        {
            if key != "startDate" || start.is_some() || value.len() != 10 {
                return refused(StatusCode::BAD_REQUEST, "invalid_query");
            }
            start = Some(value.into_owned());
        }
        Command::List(start.unwrap_or(today))
    };
    let key = match &command {
        Command::List(_) => "supplierCommitments",
        Command::Matches => "matches",
        Command::Plan(_) => "plan",
        Command::Match(_) => "supplierCommitmentMatch",
        _ => "supplierCommitment",
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&company_slug).map_err(|_| ())?;
        let action = match &command {
            Command::List(start) => Action::List(start),
            Command::Matches => Action::Matches,
            Command::Plan(i) => Action::Plan(i),
            Command::Apply(i) => Action::Apply(i),
            Command::Change(i) => Action::Change(i),
            Command::Match(i) => Action::Match(i),
        };
        owner
            .supplier_commitments(
                &cookie,
                &SystemClock,
                action,
                &chrono::DateTime::from_timestamp_millis(now)
                    .unwrap()
                    .to_rfc3339(),
            )
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
