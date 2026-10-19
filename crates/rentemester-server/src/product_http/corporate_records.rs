//! Immutable corporate records. No caller actor/principal/company overrides.
use super::*;
use base64::Engine;
use rentemester_application::corporate_records::{self as app, Action};
enum Command {
    List(usize, usize),
    Inspect(String),
    Download(String),
    Ingest(app::Ingest, Vec<u8>),
    Link(String, app::LinkInput),
    Enrich(String, app::Enrich),
    Supersede(String, app::Supersede),
}
pub(super) async fn handle(
    State(state): State<Product>,
    axum::extract::Path(args): axum::extract::Path<std::collections::HashMap<String, String>>,
    request: Request,
) -> Response {
    let slug = args.get("slug").cloned().unwrap_or_default();
    let id = args.get("id").cloned();
    let mutation = request.method() != axum::http::Method::GET;
    let cookie = match proof(&state, &request, mutation) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "corporate_record_not_found");
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
        let bound = if id.is_none() {
            24 * 1024 * 1024
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
        let parsed = match (id, path.rsplit('/').next()) {
            (None, _) => serde_json::from_value::<app::Ingest>(body).and_then(|i| {
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(&i.bytes_base64)
                    .map_err(serde::de::Error::custom)?;
                Ok(Command::Ingest(i, bytes))
            }),
            (Some(id), Some("link")) => serde_json::from_value(body).map(|v| Command::Link(id, v)),
            (Some(id), Some("enrich")) => {
                serde_json::from_value(body).map(|v| Command::Enrich(id, v))
            }
            (Some(id), Some("supersede")) => {
                serde_json::from_value(body).map(|v| Command::Supersede(id, v))
            }
            _ => return refused(StatusCode::NOT_FOUND, "not_found"),
        };
        match parsed {
            Ok(v) => v,
            Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
        }
    } else if let Some(id) = id {
        if request.uri().query().is_some() {
            return refused(StatusCode::BAD_REQUEST, "invalid_query");
        }
        if path.ends_with("/download") {
            Command::Download(id)
        } else {
            Command::Inspect(id)
        }
    } else {
        let (mut cursor, mut limit) = (None, None);
        for (k, v) in url::form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes()) {
            let target = match k.as_ref() {
                "cursor" if cursor.is_none() => &mut cursor,
                "limit" if limit.is_none() => &mut limit,
                _ => return refused(StatusCode::BAD_REQUEST, "invalid_query"),
            };
            let Ok(n) = v.parse::<usize>() else {
                return refused(StatusCode::BAD_REQUEST, "invalid_query");
            };
            *target = Some(n);
        }
        Command::List(cursor.unwrap_or(0), limit.unwrap_or(25))
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let action = match &command {
            Command::List(c, l) => Action::List {
                cursor: *c,
                limit: *l,
            },
            Command::Inspect(id) => Action::Inspect(id),
            Command::Download(id) => Action::Download(id),
            Command::Ingest(i, b) => Action::Ingest(i, b),
            Command::Link(id, i) => Action::Link(id, i),
            Command::Enrich(id, i) => Action::Enrich(id, i),
            Command::Supersede(id, i) => Action::Supersede(id, i),
        };
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        owner
            .corporate_records(&state.native.cookies, &SystemClock, &cookie, action)
            .map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(a)) => match a.outcome {
            Ok(mut v) => {
                if v.get("bytes").is_some() {
                    let Ok(bytes) = serde_json::from_value::<Vec<u8>>(v["bytes"].take()) else {
                        return refused(StatusCode::SERVICE_UNAVAILABLE, "internal");
                    };
                    v.as_object_mut().unwrap().remove("bytes");
                    v["bytesBase64"] =
                        json!(base64::engine::general_purpose::STANDARD.encode(bytes));
                }
                v["ok"] = json!(true);
                response(StatusCode::OK, v)
            }
            Err(f) => {
                use rentemester_application::password_login::FailureStage;
                let status = if f.stage == FailureStage::OutcomeUnknown {
                    StatusCode::SERVICE_UNAVAILABLE
                } else if matches!(f.error.code, "access_denied" | "invalid_credentials") {
                    StatusCode::FORBIDDEN
                } else if f.error.code == "corporate_record_not_found" {
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
