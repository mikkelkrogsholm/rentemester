//! Exact current-authority ownership proposal/review/apply adapter.
use super::*;
use rentemester_application::ownership::{self as app, Action};
enum Command {
    Read(String),
    History(Option<String>, Option<String>),
    Propose(app::Propose),
    Review(app::Review),
    Apply(app::Apply),
}
pub(super) async fn handle(
    State(state): State<Product>,
    axum::extract::Path(args): axum::extract::Path<std::collections::HashMap<String, String>>,
    request: Request,
) -> Response {
    let slug = args.get("slug").cloned().unwrap_or_default();
    let mutation = request.method() != axum::http::Method::GET;
    let cookie = match proof(&state, &request, mutation) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "ownership_not_found");
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
        let bound = if path.ends_with("/propose") {
            2 * 1024 * 1024
        } else {
            32 * 1024
        };
        let Ok(bytes) = to_bytes(request.into_body(), bound).await else {
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
            Some("apply") => serde_json::from_value(body).map(Command::Apply),
            _ => return refused(StatusCode::NOT_FOUND, "not_found"),
        };
        match parsed {
            Ok(v) => v,
            Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
        }
    } else {
        let history = path.ends_with("/history");
        let (mut id, mut at) = (None, None);
        for (k, v) in url::form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes()) {
            match k.as_ref() {
                "snapshotId" if history && id.is_none() => id = Some(v.into_owned()),
                "asOf" if at.is_none() => at = Some(v.into_owned()),
                _ => return refused(StatusCode::BAD_REQUEST, "invalid_query"),
            }
        }
        if history {
            Command::History(id, at)
        } else {
            let Some(at) = at else {
                return refused(StatusCode::BAD_REQUEST, "invalid_query");
            };
            Command::Read(at)
        }
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let action = match &command {
            Command::Read(at) => Action::Read(at),
            Command::History(id, at) => Action::History {
                snapshot_id: id.as_deref(),
                as_of: at.as_deref(),
            },
            Command::Propose(i) => Action::Propose(i),
            Command::Review(i) => Action::Review(i),
            Command::Apply(i) => Action::Apply(i),
        };
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        owner
            .ownership(&state.native.cookies, &SystemClock, &cookie, action)
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
                } else if f.error.code == "ownership_not_found" {
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
