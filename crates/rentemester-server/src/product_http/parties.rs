//! Canonical parties projected through current admitted company visibility, never raw control data.
use super::*;
use rentemester_application::parties::{self as app, Action};
enum Command {
    List(Option<String>, usize, usize),
    Inspect(String),
    Create(app::Create),
    Role(String, app::Role),
    Assertion(String, app::Assertion),
    Alias(String, app::Alias),
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    // GET-backed HEAD must refuse before credentials, body parsing or mutation dispatch.
    if request.method() == axum::http::Method::HEAD {
        let mut response = refused(StatusCode::METHOD_NOT_ALLOWED, "method_not_allowed");
        response.headers_mut().insert("allow", axum::http::HeaderValue::from_static("GET"));
        return response;
    }
    let path = request.uri().path().to_owned();
    let parts: Vec<_> = path.split('/').collect();
    let slug = parts.get(3).unwrap_or(&"").to_string();
    let id = parts.get(5).map(|s| {
        url::form_urlencoded::parse(format!("id={}", s.replace('+', "%2B")).as_bytes())
            .next()
            .unwrap()
            .1
            .into_owned()
    });
    let mutation = request.method() != axum::http::Method::GET;
    let cookie = match proof(&state, &request, mutation) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
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
        let c = match (id, parts.get(6).copied()) {
            (None, None) => serde_json::from_value(body).map(Command::Create),
            (Some(id), Some("role")) => serde_json::from_value(body).map(|b| Command::Role(id, b)),
            (Some(id), Some("assertion")) => {
                serde_json::from_value(body).map(|b| Command::Assertion(id, b))
            }
            (Some(id), Some("alias")) => {
                serde_json::from_value(body).map(|b| Command::Alias(id, b))
            }
            _ => return refused(StatusCode::NOT_FOUND, "not_found"),
        };
        match c {
            Ok(c) => c,
            Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
        }
    } else if let Some(id) = id {
        if request.uri().query().is_some() {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        Command::Inspect(id)
    } else {
        let mut query = None;
        let mut cursor = None;
        let mut limit = None;
        for (k, v) in url::form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes()) {
            match k.as_ref() {
                "query" if query.is_none() && v.encode_utf16().count() <= 512 => {
                    query = Some(v.into_owned())
                }
                "cursor" if cursor.is_none() && v.bytes().all(|b| b.is_ascii_digit()) => {
                    cursor = v.parse::<usize>().ok()
                }
                "limit" if limit.is_none() && v.bytes().all(|b| b.is_ascii_digit()) => {
                    limit = v.parse::<usize>().ok()
                }
                _ => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
            };
            if k == "cursor" && cursor.is_none() || k == "limit" && limit.is_none() {
                return refused(StatusCode::BAD_REQUEST, "invalid_input");
            }
        }
        Command::List(
            query,
            cursor.unwrap_or(0),
            limit.unwrap_or(25).clamp(1, 100),
        )
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let action = match &command {
            Command::List(q, c, l) => Action::List {
                query: q.as_deref(),
                cursor: *c,
                limit: *l,
            },
            Command::Inspect(id) => Action::Inspect(id),
            Command::Create(i) => Action::Create(i),
            Command::Role(id, i) => Action::Role(id, i),
            Command::Assertion(id, i) => Action::Assertion(id, i),
            Command::Alias(id, i) => Action::Alias(id, i),
        };
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        owner
            .parties(&state.native.cookies, &SystemClock, &cookie, action)
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
                let unknown = f.stage == FailureStage::OutcomeUnknown;
                let status = if unknown {
                    StatusCode::SERVICE_UNAVAILABLE
                } else if matches!(f.error.code, "access_denied" | "invalid_credentials") {
                    StatusCode::FORBIDDEN
                } else if f.error.code == "party_not_found" {
                    StatusCode::NOT_FOUND
                } else {
                    StatusCode::UNPROCESSABLE_ENTITY
                };
                refused(status, if unknown { "internal" } else { f.error.code })
            }
        },
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "internal"),
    }
}
