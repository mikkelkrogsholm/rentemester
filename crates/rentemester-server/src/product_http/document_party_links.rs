//! Source-shaped document party review; the plan POST is read-only and never writes evidence.
use super::*;
use rentemester_application::document_party_links::Action;
use rentemester_storage::document_party_links::{Apply, PlanInput, Supersede};
enum Command {
    List(Option<String>, usize),
    History(i64),
    Plan(PlanInput),
    Apply(Apply),
    Supersede(Supersede),
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let path = request.uri().path().to_owned();
    let pieces: Vec<_> = path.split('/').collect();
    let slug = pieces.get(3).unwrap_or(&"").to_string();
    let operation = pieces.last().copied().unwrap_or("").to_owned();
    let post = request.method() == axum::http::Method::POST;
    let cookie = match proof(&state, &request, post && operation != "plan") {
        Ok(p) => p,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    let command = if post {
        if request.uri().query().is_some()
            || request.headers().contains_key("content-encoding")
            || !one(request.headers(), "content-type")
                .is_some_and(|s| s.eq_ignore_ascii_case("application/json"))
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        let Ok(bytes) = to_bytes(request.into_body(), 16 * 1024).await else {
            return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
        };
        let c = match operation.as_str() {
            "plan" => serde_json::from_slice(&bytes).map(Command::Plan),
            "apply" => serde_json::from_slice(&bytes).map(Command::Apply),
            "supersede" => serde_json::from_slice(&bytes).map(Command::Supersede),
            _ => return refused(StatusCode::NOT_FOUND, "not_found"),
        };
        match c {
            Ok(c) => c,
            Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
        }
    } else if pieces.get(5).copied() == Some("party-links") {
        let mut status = None;
        let mut limit = None;
        for (k, v) in url::form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes()) {
            match k.as_ref() {
                "status"
                    if status.is_none()
                        && [
                            "linked",
                            "unlinked",
                            "resolved",
                            "unresolved",
                            "internal_no_external_party",
                        ]
                        .contains(&v.as_ref()) =>
                {
                    status = Some(v.into_owned())
                }
                "limit" if limit.is_none() && v.bytes().all(|b| b.is_ascii_digit()) => {
                    limit = v.parse::<usize>().ok();
                    if limit.is_none_or(|n| !(1..=100).contains(&n)) {
                        return refused(StatusCode::BAD_REQUEST, "invalid_query");
                    }
                }
                _ => return refused(StatusCode::BAD_REQUEST, "invalid_query"),
            }
        }
        Command::List(status, limit.unwrap_or(100))
    } else {
        if request.uri().query().is_some() {
            return refused(StatusCode::BAD_REQUEST, "invalid_query");
        }
        let Some(id) = pieces
            .get(5)
            .and_then(|s| s.parse::<i64>().ok())
            .filter(|id| *id > 0 && *id <= 9_007_199_254_740_991)
        else {
            return refused(StatusCode::BAD_REQUEST, "invalid_document");
        };
        Command::History(id)
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        let action = match &command {
            Command::List(s, l) => Action::List(s.as_deref(), *l),
            Command::History(id) => Action::History(*id),
            Command::Plan(i) => Action::Plan(i),
            Command::Apply(i) => Action::Apply(i),
            Command::Supersede(i) => Action::Supersede(i),
        };
        owner
            .document_party_links(&cookie, &SystemClock, action)
            .map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(attempt)) => match super::documents::outcome(attempt) {
            Ok(v) => response(StatusCode::OK, v),
            Err(r) => *r,
        },
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unconfirmed"),
    }
}
