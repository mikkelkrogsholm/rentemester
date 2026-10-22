//! Additive original-document adapter; no local server paths or caller actors.
use super::*;
use rentemester_application::documents_original::Action;
use rentemester_engine::document::Metadata;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Enrich {
    metadata: Metadata,
    confirm: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Context {
    source_reference: String,
    business_use_reason: String,
    confirm: bool,
}
pub(super) fn ingest_receipt(receipt: Value, hash: &str) -> Result<Value, Box<Response>> {
    if receipt["ok"] == false {
        return Ok(receipt);
    }
    let id = receipt["document"]["id"]
        .as_i64()
        .filter(|n| *n > 0)
        .ok_or_else(|| Box::new(refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unknown")))?;
    let number = receipt["document"]["documentNo"]
        .as_str()
        .ok_or_else(|| Box::new(refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unknown")))?;
    Ok(
        json!({"ok":true,"documentId":id,"documentNo":number,"sha256":hash,"storedPath":null,"message":format!("Bilag {number} er indlæst.")}),
    )
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let path = request.uri().path().to_owned();
    let parts = path.split('/').collect::<Vec<_>>();
    let slug = parts.get(3).copied().unwrap_or("").to_owned();
    let write = request.method() == axum::http::Method::POST;
    let credential = match documents::credential(&state, &request, write) {
        Ok(c) => c,
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
    if write
        && !one(request.headers(), "content-type")
            .is_some_and(|s| s.eq_ignore_ascii_case("application/json"))
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let action = if write {
        let Some(id) = parts
            .get(6)
            .and_then(|s| s.parse::<i64>().ok())
            .filter(|n| (1..=9_007_199_254_740_991).contains(n))
        else {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        };
        let Ok(bytes) = to_bytes(request.into_body(), 1024 * 1024).await else {
            return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
        };
        if path.ends_with("/enrich") {
            let Ok(input) = serde_json::from_slice::<Enrich>(&bytes) else {
                return refused(StatusCode::BAD_REQUEST, "invalid_input");
            };
            if !input.confirm {
                return refused(StatusCode::BAD_REQUEST, "confirmation_required");
            }
            Action::Enrich(id, Box::new(input.metadata))
        } else {
            let Ok(input) = serde_json::from_slice::<Context>(&bytes) else {
                return refused(StatusCode::BAD_REQUEST, "invalid_input");
            };
            if !input.confirm {
                return refused(StatusCode::BAD_REQUEST, "confirmation_required");
            }
            Action::CompanyContext(id, input.source_reference, input.business_use_reason)
        }
    } else {
        let Ok(bytes) = to_bytes(request.into_body(), 1).await else {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        };
        if !bytes.is_empty() {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        Action::List
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&slug).map_err(|_| {
            Box::new(refused(
                StatusCode::SERVICE_UNAVAILABLE,
                "owner_unavailable",
            ))
        })?;
        match credential {
            documents::Credential::Human(cookie) => documents::outcome(
                owner
                    .documents_original(&cookie, &SystemClock, action)
                    .map_err(|_| refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable"))?,
            ),
            documents::Credential::Service(key) => documents::service_outcome(
                owner
                    .documents_original_service(&key, &SystemClock, action)
                    .map_err(|_| refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable"))?,
            ),
        }
    })
    .await;
    match result {
        Ok(Ok(value)) => response(StatusCode::OK, value),
        Ok(Err(r)) => *r,
        _ => refused(
            StatusCode::SERVICE_UNAVAILABLE,
            if write {
                "outcome_unknown"
            } else {
                "document_read_unavailable"
            },
        ),
    }
}
