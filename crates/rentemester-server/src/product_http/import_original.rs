//! Original import invocation wire. Canonical accounting remains in the existing motor.
use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use rentemester_runtime::document::Prepared;
const MAX_BODY: usize = 35 * 1024 * 1024;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Wire {
    source_system: String,
    file_name: String,
    content_base64: String,
    dry_run: bool,
    confirm: bool,
}
enum Input {
    Synthetic {
        source: rentemester_storage::synthetic_import::Input,
        dry_run: bool,
    },
    Dinero {
        file: Prepared,
        dry_run: bool,
    },
}
fn parse(bytes: &[u8]) -> Result<Input, ()> {
    if bytes.len() > MAX_BODY {
        return Err(());
    }
    let w: Wire = serde_json::from_slice(bytes).map_err(|_| ())?;
    if !w.confirm
        || w.file_name.is_empty()
        || w.file_name.len() > 255
        || w.file_name.contains(['/', '\\'])
        || w.file_name.chars().any(char::is_control)
        || w.content_base64.len() > 4 * (25 * 1024 * 1024usize).div_ceil(3)
    {
        return Err(());
    }
    let raw = STANDARD.decode(&w.content_base64).map_err(|_| ())?;
    if raw.is_empty() || STANDARD.encode(&raw) != w.content_base64 {
        return Err(());
    }
    match w.source_system.as_str() {
        "synthetic-csv" if raw.len() <= 48 * 1024 => Ok(Input::Synthetic {
            source: rentemester_storage::synthetic_import::Input {
                source_system: w.source_system,
                content: String::from_utf8(raw).map_err(|_| ())?,
            },
            dry_run: w.dry_run,
        }),
        "dinero" => Ok(Input::Dinero {
            file: Prepared::dinero_source(w.file_name, raw).map_err(|_| ())?,
            dry_run: w.dry_run,
        }),
        _ => Err(()),
    }
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let cookie = match proof(&state, &request, true) {
        Ok(cookie) => cookie,
        Err(r) => return *r,
    };
    let slug = request
        .uri()
        .path()
        .split('/')
        .nth(3)
        .unwrap_or("")
        .to_owned();
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    if request.method() != axum::http::Method::POST
        || request.uri().query().is_some()
        || request.headers().contains_key("content-encoding")
        || request.headers().contains_key("idempotency-key")
        || !one(request.headers(), "content-type")
            .is_some_and(|v| v.eq_ignore_ascii_case("application/json"))
    {
        return refused(StatusCode::BAD_REQUEST, "import_original_invalid");
    }
    let Ok(bytes) = to_bytes(request.into_body(), MAX_BODY).await else {
        return refused(StatusCode::PAYLOAD_TOO_LARGE, "import_original_invalid");
    };
    let input = match parse(&bytes) {
        Ok(input) => input,
        Err(()) => return refused(StatusCode::BAD_REQUEST, "import_original_invalid"),
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        match &input {
            Input::Synthetic { source, dry_run } => owner
                .opening_balance(
                    &cookie,
                    &SystemClock,
                    rentemester_application::opening_balance::Action::OriginalRun(source, *dry_run),
                )
                .map(|attempt| attempt.outcome.map(|v| v.value))
                .map_err(|_| ()),
            Input::Dinero { file, dry_run } => owner
                .dinero_import(
                    &cookie,
                    &SystemClock,
                    rentemester_application::dinero_import::Action::OriginalRun(file, *dry_run),
                )
                .map(|attempt| {
                    attempt.outcome.map(|v| match v.value {
                        rentemester_application::dinero_import::Output::Json(value) => value,
                        _ => unreachable!("original import returns JSON"),
                    })
                })
                .map_err(|_| ()),
        }
    })
    .await;
    match result {
        Ok(Ok(Ok(value))) => response(StatusCode::OK, value),
        Ok(Ok(Err(f))) => {
            let unknown =
                f.stage != rentemester_application::control_fence::FailureStage::NotCommitted;
            let status = if unknown
                || !f.control_reusable
                || !f.company_reusable
                || f.error.code.contains("unavailable")
            {
                StatusCode::SERVICE_UNAVAILABLE
            } else if matches!(f.error.code, "access_denied" | "invalid_credentials") {
                StatusCode::FORBIDDEN
            } else {
                StatusCode::UNPROCESSABLE_ENTITY
            };
            response(
                status,
                json!({"ok":false,"code":if unknown {"outcome_unknown"} else {f.error.code},"errors":[f.error.message]}),
            )
        }
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unknown"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn body(system: &str, bytes: &[u8]) -> serde_json::Value {
        json!({"sourceSystem":system,"fileName":"source.json","contentBase64":STANDARD.encode(bytes),"dryRun":true,"confirm":true})
    }
    #[test]
    fn original_import94_frozen_wire_rejects_actor_key_hash_and_invalid_content() {
        let b = body("synthetic-csv", b"source");
        assert!(matches!(
            parse(&serde_json::to_vec(&b).unwrap()),
            Ok(Input::Synthetic { dry_run: true, .. })
        ));
        for field in ["actor", "idempotencyKey", "planHash", "outputPath"] {
            let mut bad = b.clone();
            bad[field] = json!("override");
            assert!(parse(&serde_json::to_vec(&bad).unwrap()).is_err());
        }
        for (field, value) in [
            ("confirm", json!(false)),
            ("fileName", json!("../x")),
            ("sourceSystem", json!("other")),
            ("contentBase64", json!("c291cmNl\n")),
        ] {
            let mut bad = b.clone();
            bad[field] = value;
            assert!(parse(&serde_json::to_vec(&bad).unwrap()).is_err());
        }
        assert!(parse(&serde_json::to_vec(&body("synthetic-csv", &[255])).unwrap()).is_err());
        assert!(
            parse(&serde_json::to_vec(&body("synthetic-csv", &vec![b'x'; 48 * 1024 + 1])).unwrap())
                .is_err()
        );
        assert!(matches!(
            parse(&serde_json::to_vec(&body("dinero", b"{}")).unwrap()),
            Ok(Input::Dinero { dry_run: true, .. })
        ));
    }
}
