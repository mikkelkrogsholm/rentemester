//! Bounded source-backed advisory knowledge. No caller actor/principal/company overrides.
use super::*;
use rentemester_application::company_knowledge::{self as app, Action};
enum Command {
    Read(Option<String>, bool),
    Propose(app::Proposal),
    Review(app::Review),
    Supersede(app::Supersede),
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let slug = request
        .uri()
        .path()
        .split('/')
        .nth(3)
        .unwrap_or("")
        .to_owned();
    let mutation = request.method() != axum::http::Method::GET;
    let cookie = match proof(&state, &request, mutation) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
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
        let Ok(mut body) = serde_json::from_slice::<serde_json::Map<String, Value>>(&bytes) else {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        };
        if body.remove("confirm") != Some(json!(true)) {
            return refused(StatusCode::BAD_REQUEST, "confirmation_required");
        }
        let body = Value::Object(body);
        let parsed = match path.rsplit('/').next() {
            Some("propose") => serde_json::from_value(body).map(Command::Propose),
            Some("review") => serde_json::from_value(body).map(Command::Review),
            Some("supersede") => serde_json::from_value(body).map(Command::Supersede),
            _ => return refused(StatusCode::NOT_FOUND, "not_found"),
        };
        match parsed {
            Ok(c) => c,
            Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
        }
    } else {
        let mut as_of = None;
        let mut proposed = None;
        if let Some(query) = request.uri().query() {
            for (k, v) in url::form_urlencoded::parse(query.as_bytes()) {
                match k.as_ref() {
                    "asOf" if as_of.is_none() => as_of = Some(v.into_owned()),
                    "includeProposed"
                        if proposed.is_none() && matches!(v.as_ref(), "true" | "false") =>
                    {
                        proposed = Some(v == "true")
                    }
                    _ => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
                }
            }
        }
        Command::Read(as_of, proposed.unwrap_or(false))
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let action = match &command {
            Command::Read(a, p) => Action::Read {
                as_of: a.as_deref(),
                include_proposed: *p,
            },
            Command::Propose(i) => Action::Propose(i),
            Command::Review(i) => Action::Review(i),
            Command::Supersede(i) => Action::Supersede(i),
        };
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        owner
            .company_knowledge(&state.native.cookies, &SystemClock, &cookie, action)
            .map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(a)) => match a.outcome {
            Ok(mut v) => {
                v["ok"] = json!(true);
                response(StatusCode::OK, v)
            }
            Err(f) => {
                use rentemester_application::password_login::FailureStage;
                let status = if f.stage == FailureStage::OutcomeUnknown {
                    StatusCode::SERVICE_UNAVAILABLE
                } else if matches!(f.error.code, "access_denied" | "invalid_credentials") {
                    StatusCode::FORBIDDEN
                } else if f.error.code == "knowledge_not_found" {
                    StatusCode::NOT_FOUND
                } else {
                    StatusCode::UNPROCESSABLE_ENTITY
                };
                refused(
                    status,
                    if f.stage == FailureStage::OutcomeUnknown {
                        "internal"
                    } else {
                        f.error.code
                    },
                )
            }
        },
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "internal"),
    }
}
