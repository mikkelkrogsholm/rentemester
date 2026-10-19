use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use rentemester_application::documents::Ingest;
use rentemester_engine::document::Metadata;
use rentemester_runtime::document::Prepared;
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Upload {
    confirm: bool,
    file_name: String,
    file_base64: String,
    metadata: Metadata,
    #[serde(default)]
    force: bool,
    vendor_id: Option<i64>,
}
pub(super) fn outcome<T>(
    attempt: rentemester_host::Attempt<
        Result<
            rentemester_application::control_fence::Committed<T>,
            rentemester_application::control_fence::Failure,
        >,
    >,
) -> Result<T, Box<Response>> {
    match attempt.outcome {
        Ok(value) => Ok(value.value),
        Err(f) if f.error.code == "invalid_credentials" => {
            Err(Box::new(refused(StatusCode::UNAUTHORIZED, "unauthorized")))
        }
        Err(f) if f.stage == FailureStage::NotCommitted => Err(Box::new(response(
            StatusCode::UNPROCESSABLE_ENTITY,
            json!({"ok":false,"code":f.error.code,"errors":[f.error.message]}),
        ))),
        _ => Err(Box::new(refused(
            StatusCode::SERVICE_UNAVAILABLE,
            "outcome_unknown",
        ))),
    }
}
pub(super) async fn ingest(State(state): State<Product>, request: Request) -> Response {
    let company_slug=request.uri().path().split('/').nth(3).unwrap_or("").to_owned();
    let cookie = match proof(&state, &request, true) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    if request.uri().query().is_some()
        || request.headers().contains_key("content-encoding")
        || !one(request.headers(), "content-type")
            .is_some_and(|s| s.eq_ignore_ascii_case("application/json"))
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let bytes = match to_bytes(request.into_body(), 35 * 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => return refused(StatusCode::PAYLOAD_TOO_LARGE, "upload_too_large"),
    };
    let input: Upload = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
    };
    if !input.confirm {
        return refused(StatusCode::BAD_REQUEST, "confirmation_required");
    }
    if input.force || input.vendor_id.is_some() {
        return response(
            StatusCode::UNPROCESSABLE_ENTITY,
            json!({"ok":false,"code":"native_document_review_required","errors":["Forced ingestion and vendor links are not available in this Rust upload yet"]}),
        );
    }
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let bytes = STANDARD
            .decode(input.file_base64)
            .map_err(|_| refused(StatusCode::BAD_REQUEST, "invalid_base64"))?;
        let file = Prepared::new(input.file_name, bytes).map_err(|message| {
            response(
                StatusCode::UNPROCESSABLE_ENTITY,
                json!({"ok":false,"code":"invalid_document_file","errors":[message]}),
            )
        })?;
        let today = chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now())
            .format("%Y-%m-%d")
            .to_string();
        let mut owner = state.native.lock_company(&company_slug)
            .map_err(|_| refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable"))?;
        let attempt = owner
            .ingest_document(
                &cookie,
                &SystemClock,
                Ingest {
                    file: &file,
                    metadata: &input.metadata,
                    today: &today,
                },
            )
            .map_err(|_| refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable"))?;
        outcome(attempt)
    })
    .await;
    match result {
        Ok(Ok(value)) => response(StatusCode::OK, value),
        Ok(Err(r)) => *r,
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unconfirmed"),
    }
}
pub(super) async fn list(State(state): State<Product>, request: Request) -> Response {
    let company_slug=request.uri().path().split('/').nth(3).unwrap_or("").to_owned();
    let cookie = match proof(&state, &request, false) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    if !selected(&state, &request) || request.uri().query().is_some() {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&company_slug).map_err(|_| ())?;
        owner.documents(&cookie, &SystemClock).map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(attempt)) => match outcome(attempt) {
            Ok(value) => response(StatusCode::OK, json!({"ok":true,"documents":value})),
            Err(r) => *r,
        },
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "read_unconfirmed"),
    }
}
pub(super) async fn file(State(state): State<Product>, request: Request) -> Response {
    let company_slug=request.uri().path().split('/').nth(3).unwrap_or("").to_owned();
    let cookie = match proof(&state, &request, false) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    if !selected(&state, &request) || request.uri().query().is_some() {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let Some(id) = request
        .uri()
        .path()
        .rsplit('/')
        .nth(1)
        .and_then(|s| s.parse::<i64>().ok())
        .filter(|id| *id > 0 && *id <= 9_007_199_254_740_991)
    else {
        return refused(StatusCode::NOT_FOUND, "document_not_found");
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&company_slug).map_err(|_| ())?;
        owner
            .document_original(&cookie, &SystemClock, id)
            .map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(attempt)) => match outcome(attempt) {
            Ok(original) => {
                let Ok(mime) = axum::http::HeaderValue::from_str(&original.mime) else {
                    return refused(StatusCode::SERVICE_UNAVAILABLE, "evidence_unavailable");
                };
                let filename = percent_encoding::utf8_percent_encode(
                    &original.filename,
                    percent_encoding::NON_ALPHANUMERIC,
                )
                .to_string();
                let Ok(disposition) = axum::http::HeaderValue::from_str(&format!(
                    "inline; filename*=UTF-8''{filename}"
                )) else {
                    return refused(StatusCode::SERVICE_UNAVAILABLE, "evidence_unavailable");
                };
                let mut reply = (StatusCode::OK, original.bytes).into_response();
                reply.headers_mut().insert("content-type", mime);
                reply
                    .headers_mut()
                    .insert("content-disposition", disposition);
                reply.headers_mut().insert(
                    "cache-control",
                    axum::http::HeaderValue::from_static("private, no-store"),
                );
                reply.headers_mut().insert(
                    "content-security-policy",
                    axum::http::HeaderValue::from_static("sandbox; default-src 'none'"),
                );
                reply
            }
            Err(r) => *r,
        },
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "evidence_unavailable"),
    }
}

pub(super) async fn expense(State(state): State<Product>, request: Request) -> Response {
    let company_slug=request.uri().path().split('/').nth(3).unwrap_or("").to_owned();
    use rentemester_application::expense::{Action, Input};
    let posting = request.method() == axum::http::Method::POST;
    let cookie = match proof(&state, &request, posting) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    if !selected(&state, &request) || request.uri().query().is_some() {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let id = if posting {
        None
    } else {
        request
            .uri()
            .path()
            .rsplit('/')
            .nth(1)
            .and_then(|s| s.parse::<i64>().ok())
            .filter(|id| *id > 0 && *id <= 9_007_199_254_740_991)
    };
    if !posting && id.is_none() {
        return refused(StatusCode::NOT_FOUND, "document_not_found");
    }
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let input: Option<Input> = if posting {
        if request.headers().contains_key("content-encoding")
            || !one(request.headers(), "content-type")
                .is_some_and(|s| s.eq_ignore_ascii_case("application/json"))
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        let Ok(bytes) = to_bytes(request.into_body(), 16 * 1024).await else {
            return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
        };
        let Ok(mut object) = serde_json::from_slice::<serde_json::Map<String, Value>>(&bytes)
        else {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        };
        if object.remove("confirm") != Some(json!(true)) {
            return refused(StatusCode::BAD_REQUEST, "confirmation_required");
        }
        let Ok(value) = serde_json::from_value(Value::Object(object)) else {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        };
        Some(value)
    } else {
        None
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&company_slug).map_err(|_| ())?;
        let action = match input.as_ref() {
            Some(input) => Action::Post(input),
            None => Action::Options(id.unwrap()),
        };
        owner.expense(&cookie, &SystemClock, action).map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(attempt)) => match outcome(attempt) {
            Ok(value) => response(
                StatusCode::OK,
                json!({"ok":true,(if posting{"booking"}else{"options"}):value}),
            ),
            Err(r) => *r,
        },
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unconfirmed"),
    }
}
