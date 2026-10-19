//! Source package ingestion is explicit; plans and originals remain read-only operations.
use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use rentemester_application::archive_import::{Action, Output};
use rentemester_runtime::document::{MAX_BYTES, Prepared};
use rentemester_storage::archive_import::Apply;
const MAX_BODY: usize = 35 * 1024 * 1024;
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct PlanBody {
    file_name: String,
    content_base64: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ApplyBody {
    file_name: String,
    content_base64: String,
    plan_hash: String,
    idempotency_key: String,
    confirm: bool,
}
enum Command {
    Plan(Prepared),
    Apply(Prepared, Apply),
    List,
    Year(u16),
    Original(i64),
    FiscalYears(String),
}
fn file(name: String, encoded: String) -> Result<Prepared, ()> {
    if encoded.len() > 4 * MAX_BYTES.div_ceil(3) {
        return Err(());
    }
    let bytes = STANDARD.decode(encoded).map_err(|_| ())?;
    let prepared = Prepared::new(name, bytes).map_err(|_| ())?;
    if prepared.mime() != "application/json" {
        return Err(());
    }
    Ok(prepared)
}
fn parse(bytes: &[u8], apply: bool) -> Result<Command, ()> {
    if bytes.len() > MAX_BODY {
        return Err(());
    }
    if apply {
        let body: ApplyBody = serde_json::from_slice(bytes).map_err(|_| ())?;
        if !body.confirm {
            return Err(());
        }
        Ok(Command::Apply(
            file(body.file_name, body.content_base64)?,
            Apply {
                plan_hash: body.plan_hash,
                idempotency_key: body.idempotency_key,
                confirm: true,
            },
        ))
    } else {
        let body: PlanBody = serde_json::from_slice(bytes).map_err(|_| ())?;
        Ok(Command::Plan(file(body.file_name, body.content_base64)?))
    }
}
fn positive(raw: &str) -> Result<i64, ()> {
    if raw.is_empty() || raw.starts_with('0') || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return Err(());
    }
    raw.parse::<i64>()
        .ok()
        .filter(|id| *id > 0 && *id <= 9_007_199_254_740_991)
        .ok_or(())
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let path = request.uri().path().to_owned();
    let posting = request.method() == axum::http::Method::POST;
    let applying = posting && path.ends_with("/apply");
    let cookie = match proof(&state, &request, applying) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    if request.uri().query().is_some()
        || request.headers().contains_key("content-encoding")
        || request.headers().contains_key("idempotency-key")
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let slug = path.split('/').nth(3).unwrap_or("").to_owned();
    let command = if posting {
        if !path.ends_with("/plan") && !applying {
            return refused(StatusCode::NOT_FOUND, "not_found");
        }
        if !one(request.headers(), "content-type")
            .is_some_and(|v| v.eq_ignore_ascii_case("application/json"))
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        let bytes = match to_bytes(request.into_body(), MAX_BODY).await {
            Ok(b) => b,
            Err(_) => return refused(StatusCode::PAYLOAD_TOO_LARGE, "archive_import_too_large"),
        };
        match parse(&bytes, applying) {
            Ok(c) => c,
            Err(()) => return refused(StatusCode::BAD_REQUEST, "archive_import_invalid"),
        }
    } else if request.method() == axum::http::Method::GET {
        if request.headers().contains_key("transfer-encoding")
            || one(request.headers(), "content-length").is_some_and(|v| v != "0")
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        if path.ends_with("/fiscal-years") {
            Command::FiscalYears(
                chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now())
                    .format("%Y-%m-%d")
                    .to_string(),
            )
        } else if path.ends_with("/archives") {
            Command::List
        } else if path.ends_with("/original") {
            match path.split('/').nth(5).and_then(|v| positive(v).ok()) {
                Some(id) => Command::Original(id),
                None => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
            }
        } else {
            match path
                .split('/')
                .nth(5)
                .filter(|v| v.len() == 4)
                .and_then(|v| positive(v).ok())
                .and_then(|v| u16::try_from(v).ok())
            {
                Some(year) => Command::Year(year),
                None => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
            }
        }
    } else {
        return refused(StatusCode::METHOD_NOT_ALLOWED, "invalid_input");
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let action = match &command {
            Command::Plan(file) => Action::Plan(file),
            Command::Apply(file, input) => Action::Apply(file, input),
            Command::List => Action::List,
            Command::Year(year) => Action::Year(*year),
            Command::Original(id) => Action::Original(*id),
            Command::FiscalYears(today) => Action::FiscalYears { today },
        };
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        owner
            .archive_import(&cookie, &SystemClock, action)
            .map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(attempt)) => match attempt.outcome {
            Ok(value) => match value.value {
                Output::Json(json) => response(StatusCode::OK, json),
                Output::Original(bytes) => {
                    let mut reply = (StatusCode::OK, bytes).into_response();
                    reply.headers_mut().insert(
                        "content-type",
                        axum::http::HeaderValue::from_static("application/json"),
                    );
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
            },
            Err(f) => {
                let code = f.error.code;
                if f.stage != FailureStage::NotCommitted {
                    return refused(
                        StatusCode::SERVICE_UNAVAILABLE,
                        if applying {
                            "outcome_unconfirmed"
                        } else {
                            "archive_source_unavailable"
                        },
                    );
                }
                let status = if matches!(code, "access_denied" | "invalid_credentials") {
                    StatusCode::FORBIDDEN
                } else if code == "archive_not_found" {
                    StatusCode::NOT_FOUND
                } else if code == "archive_import_invalid" {
                    StatusCode::BAD_REQUEST
                } else if matches!(
                    code,
                    "IDEMPOTENCY_CONFLICT"
                        | "IDEMPOTENCY_OUTCOME_EXPIRED"
                        | "RESTORED_KEY_RECONCILIATION_REQUIRED"
                ) {
                    StatusCode::CONFLICT
                } else if code.contains("unavailable")
                    || code == "IDEMPOTENCY_STORAGE_FAILURE"
                    || !f.company_reusable
                    || !f.control_reusable
                {
                    StatusCode::SERVICE_UNAVAILABLE
                } else {
                    StatusCode::UNPROCESSABLE_ENTITY
                };
                response(
                    status,
                    json!({"ok":false,"code":code,"errors":[f.error.message],"outcome":"not_committed"}),
                )
            }
        },
        _ => refused(
            StatusCode::SERVICE_UNAVAILABLE,
            if applying {
                "outcome_unconfirmed"
            } else {
                "archive_source_unavailable"
            },
        ),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_confirmed_package_bodies_ids_and_bounds() {
        let source = STANDARD.encode(b"{}");
        assert!(
            parse(
                serde_json::to_string(&json!({"fileName":"synthetic.json","contentBase64":source}))
                    .unwrap()
                    .as_bytes(),
                false
            )
            .is_ok()
        );
        assert!(parse(serde_json::to_string(&json!({"fileName":"synthetic.json","contentBase64":source,"planHash":"a".repeat(64),"idempotencyKey":"synthetic","confirm":true})).unwrap().as_bytes(),true).is_ok());
        for raw in [
            r#"{"fileName":"x.json","contentBase64":"e30=","actor":"user:guessed"}"#,
            r#"{"fileName":"x.json","contentBase64":"e30=","fileName":"other.json"}"#,
            r#"{"fileName":"x.json","contentBase64":"e30=","confirm":false,"planHash":"x","idempotencyKey":"k"}"#,
        ] {
            assert!(parse(raw.as_bytes(), raw.contains("confirm")).is_err());
        }
        assert!(parse(&vec![b' '; MAX_BODY + 1], false).is_err());
        for id in ["0", "01", "-1", "9007199254740992", "1%32"] {
            assert!(positive(id).is_err());
        }
    }
}
