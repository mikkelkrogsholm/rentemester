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
    // Frozen private-base stub: no accounting, source publication or invented key.
    // The owning backend worker replaces this with current Owner/fence execution.
    if let Err(r) = proof(&state, &request, true) {
        return *r;
    }
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
    if parse(&bytes).is_err() {
        return refused(StatusCode::BAD_REQUEST, "import_original_invalid");
    }
    refused(
        StatusCode::SERVICE_UNAVAILABLE,
        "import_original_unavailable",
    )
}
