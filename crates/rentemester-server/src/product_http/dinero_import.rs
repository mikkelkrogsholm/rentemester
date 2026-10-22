//! Bounded, explicit source-package import. Originals remain raw private JSON bytes.
use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use rentemester_application::dinero_import::{Action, Output};
use rentemester_engine::fiscal_year::Date;
use rentemester_runtime::document::{MAX_BYTES, Prepared};
use rentemester_storage::dinero_import::Apply;
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
    Original(i64),
    Imported(String),
}
fn file(name: String, encoded: String) -> Result<Prepared, ()> {
    if encoded.len() > 4 * MAX_BYTES.div_ceil(3) {
        return Err(());
    }
    let bytes = STANDARD.decode(&encoded).map_err(|_| ())?;
    if STANDARD.encode(&bytes) != encoded {
        return Err(());
    }
    let prepared = Prepared::dinero_source(name, bytes).map_err(|_| ())?;
    if !matches!(prepared.mime(), "application/json" | "application/zip") {
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
        if !body.confirm
            || body.plan_hash.len() != 64
            || !body
                .plan_hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
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
fn as_of(raw: Option<&str>) -> Result<String, ()> {
    as_of_with_clock(raw, &SystemClock)
}
fn as_of_with_clock(
    raw: Option<&str>,
    clock: &dyn rentemester_runtime::auth::Clock,
) -> Result<String, ()> {
    if raw.is_none() {
        let utc = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(
            clock.now_unix_ms().map_err(|_| ())?,
        )
        .ok_or(())?;
        let date = utc.format("%Y-%m-%d").to_string();
        Date::parse(&date).map_err(|_| ())?;
        return Ok(date);
    }
    let date = raw.and_then(|q| q.strip_prefix("asOf=")).ok_or(())?;
    Date::parse(date).map_err(|_| ())?;
    Ok(date.into())
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
    let slug = path.split('/').nth(3).unwrap_or("").to_owned();
    let root = format!("/api/companies/{slug}/imports/dinero");
    let imported_path = format!("/api/companies/{slug}/invoices/imported-receivables");
    if request.headers().contains_key("content-encoding")
        || request.headers().contains_key("idempotency-key")
        || (path != imported_path && request.uri().query().is_some())
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let command = if posting {
        if path != format!("{root}/plan") && path != format!("{root}/apply") {
            return refused(StatusCode::NOT_FOUND, "not_found");
        }
        if !one(request.headers(), "content-type")
            .is_some_and(|v| v.eq_ignore_ascii_case("application/json"))
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        let bytes = match to_bytes(request.into_body(), MAX_BODY).await {
            Ok(b) => b,
            Err(_) => return refused(StatusCode::PAYLOAD_TOO_LARGE, "dinero_import_too_large"),
        };
        match parse(&bytes, applying) {
            Ok(c) => c,
            Err(()) => return refused(StatusCode::BAD_REQUEST, "dinero_import_invalid"),
        }
    } else if request.method() == axum::http::Method::GET {
        if request.headers().contains_key("transfer-encoding")
            || one(request.headers(), "content-length").is_some_and(|v| v != "0")
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        if path == imported_path {
            match as_of(request.uri().query()) {
                Ok(v) => Command::Imported(v),
                Err(()) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
            }
        } else if path == root {
            Command::List
        } else if let Some(id) = path
            .strip_prefix(&format!("{root}/"))
            .and_then(|s| s.strip_suffix("/original"))
        {
            match positive(id) {
                Ok(id) => Command::Original(id),
                Err(()) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
            }
        } else {
            return refused(StatusCode::NOT_FOUND, "not_found");
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
            Command::Plan(f) => Action::Plan(f),
            Command::Apply(f, i) => Action::Apply(f, i),
            Command::List => Action::List,
            Command::Original(id) => Action::Original(*id),
            Command::Imported(d) => Action::ImportedReceivables(d),
        };
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        owner
            .dinero_import(&cookie, &SystemClock, action)
            .map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(attempt)) => match attempt.outcome {
            Ok(value) => match value.value {
                Output::Json(value) => {
                    response(StatusCode::OK, json!({"ok":true,"dineroImport":value}))
                }
                Output::Imported(value) => response(
                    StatusCode::OK,
                    json!({"ok":true,"importedReceivables":value}),
                ),
                Output::Original {
                    bytes,
                    content_type,
                } => {
                    let mut reply = (StatusCode::OK, bytes).into_response();
                    reply.headers_mut().insert(
                        "content-type",
                        axum::http::HeaderValue::from_static(content_type),
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
                            "dinero_source_unavailable"
                        },
                    );
                }
                let status = if matches!(code, "access_denied" | "invalid_credentials") {
                    StatusCode::FORBIDDEN
                } else if code == "dinero_import_not_found" {
                    StatusCode::NOT_FOUND
                } else if code == "dinero_import_invalid" || code == "invalid_date" {
                    StatusCode::BAD_REQUEST
                } else if code == "backup_overdue" {
                    StatusCode::LOCKED
                } else if matches!(
                    code,
                    "IDEMPOTENCY_CONFLICT"
                        | "IDEMPOTENCY_OUTCOME_EXPIRED"
                        | "RESTORED_KEY_RECONCILIATION_REQUIRED"
                        | "dinero_plan_changed"
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
                "dinero_source_unavailable"
            },
        ),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn omitted_schedule_date_is_trusted_utc_today() {
        struct Fixed;
        impl rentemester_runtime::auth::Clock for Fixed {
            fn now_unix_ms(&self) -> Result<i64, rentemester_runtime::auth::AuthError> {
                Ok(1_761_091_199_000)
            }
        }
        assert_eq!(as_of_with_clock(None, &Fixed).unwrap(), "2025-10-21");
        assert_eq!(
            as_of_with_clock(Some("asOf=2025-03-21"), &Fixed).unwrap(),
            "2025-03-21"
        );
        assert!(as_of_with_clock(Some(""), &Fixed).is_err());
    }
    #[test]
    fn exact_source_bodies_and_canonical_selectors() {
        let valid = json!({"fileName":"synthetic.json","contentBase64":"e30="});
        assert!(parse(&serde_json::to_vec(&valid).unwrap(), false).is_ok());
        let mut apply = valid.clone();
        apply["confirm"] = json!(true);
        apply["planHash"] = json!("a".repeat(64));
        apply["idempotencyKey"] = json!("synthetic");
        assert!(parse(&serde_json::to_vec(&apply).unwrap(), true).is_ok());
        apply["confirm"] = json!(false);
        assert!(parse(&serde_json::to_vec(&apply).unwrap(), true).is_err());
        for raw in [
            r#"{"fileName":"x.json","contentBase64":"e30=","actor":"user:guessed"}"#,
            r#"{"fileName":"x.json","fileName":"y.json","contentBase64":"e30="}"#,
            r#"{"fileName":"x.json","contentBase64":"e30"}"#,
        ] {
            assert!(parse(raw.as_bytes(), false).is_err());
        }
        assert!(parse(&vec![b' '; MAX_BODY + 1], false).is_err());
        for id in ["0", "01", "-1", "1/2", "9007199254740992", "1%32"] {
            assert!(positive(id).is_err());
        }
        assert_eq!(positive("7"), Ok(7));
        assert_eq!(as_of(Some("asOf=2025-03-21")).unwrap(), "2025-03-21");
        for query in [
            Some(""),
            Some("asOf=2025-02-30"),
            Some("asOf=2025-03-21&asOf=2025-03-22"),
            Some("asOf=2025-03-21&actor=guessed"),
            Some("asOf=2025%2D03%2D21"),
        ] {
            assert!(as_of(query).is_err());
        }
    }
}
