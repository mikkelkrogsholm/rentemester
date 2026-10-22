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
// Original source adapters expose domain errors, never native service metering envelopes.
pub(super) fn service_outcome<T>(
    attempt: rentemester_host::Attempt<
        Result<
            rentemester_application::service_company::ServiceResult<T>,
            rentemester_application::service_company::ServiceFailure,
        >,
    >,
) -> Result<T, Box<Response>> {
    use rentemester_application::service_company::{BusinessStage, UsageStage};
    match attempt.outcome {
        Ok(value) => Ok(value.result),
        Err(f) => {
            let reply = if f.usage == UsageStage::OutcomeUnknown
                || f.business == BusinessStage::OutcomeUnknown
                || !f.control_reusable
                || !f.company_reusable
            {
                refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unconfirmed")
            } else if f.error.code == "invalid_credentials" {
                refused(StatusCode::UNAUTHORIZED, "unauthorized")
            } else if f.error.code == "access_denied" {
                refused(StatusCode::FORBIDDEN, "access_denied")
            } else {
                let mut value = f.rejection.unwrap_or_else(
                    || json!({"ok":false,"code":f.error.code,"errors":[f.error.message]}),
                );
                if let Some(fields) = value.as_object_mut() {
                    fields.retain(|key, _| {
                        ["ok", "code", "errors", "appliedRules"].contains(&key.as_str())
                    });
                }
                if value["code"]
                    .as_str()
                    .is_none_or(|code| code.trim().is_empty())
                {
                    value["code"] = json!(f.error.code);
                }
                response(StatusCode::UNPROCESSABLE_ENTITY, value)
            };
            Err(Box::new(reply))
        }
    }
}
pub(super) fn ingest_receipt(mut receipt: Value, hash: &str) -> Result<Value, Box<Response>> {
    if receipt["ok"] == false {
        if let Some(fields) = receipt.as_object_mut() {
            fields
                .retain(|key, _| ["ok", "code", "errors", "appliedRules"].contains(&key.as_str()));
        }
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
            documents::Credential::Service(key) => service_outcome(
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn documents_original_service_failures_preserve_domain_details_and_redact_authority() {
        use rentemester_application::service_company::{BusinessStage, ServiceFailure, UsageStage};
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        for (code, status, expected) in [
            (
                "DOCUMENT_ORIGINAL_REFUSED",
                StatusCode::UNPROCESSABLE_ENTITY,
                json!({"ok":false,"code":"DOCUMENT_ORIGINAL_REFUSED","errors":["actual immutable evidence conflict"]}),
            ),
            (
                "access_denied",
                StatusCode::FORBIDDEN,
                json!({"ok":false,"code":"access_denied","errors":["access_denied"]}),
            ),
            (
                "invalid_credentials",
                StatusCode::UNAUTHORIZED,
                json!({"ok":false,"code":"unauthorized","errors":["unauthorized"]}),
            ),
        ] {
            let attempt = rentemester_host::Attempt {
                retired: false,
                outcome: Err(ServiceFailure {
                    usage: UsageStage::Committed,
                    business: BusinessStage::NotCommitted,
                    error: rentemester_engine::Error::new(
                        code,
                        "actual immutable evidence conflict",
                    ),
                    control_reusable: true,
                    company_reusable: true,
                    rejection: None,
                }),
            };
            let response = service_outcome::<Value>(attempt).unwrap_err();
            assert_eq!(response.status(), status);
            let bytes = runtime
                .block_on(to_bytes(response.into_body(), 4096))
                .unwrap();
            assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap(), expected);
        }
        let value = ingest_receipt(json!({"ok":false,"code":"DOCUMENT_INGEST_BLOCKED","errors":["same immutable original"],"usage":"committed","business":"committed"}), "unused").unwrap();
        assert_eq!(
            value,
            json!({"ok":false,"code":"DOCUMENT_INGEST_BLOCKED","errors":["same immutable original"]})
        );
    }
}
