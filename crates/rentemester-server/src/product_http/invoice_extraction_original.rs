//! Explicit extraction: no Owner or SQL transaction survives provider I/O.
use super::*;
use axum::http::Method;
use rentemester_application::invoice_extraction::Completion;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Confirm {
    confirm: bool,
    #[serde(
        rename = "expectedActor",
        default,
        deserialize_with = "actor_assertion"
    )]
    expected_actor: Option<String>,
}
fn actor_assertion<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    let value = String::deserialize(d)?;
    if value.trim().is_empty() || value.trim() != value || value.len() > 160 {
        return Err(serde::de::Error::custom("invalid actor assertion"));
    }
    Ok(Some(value))
}
fn domain_error(code: &str, unknown: bool) -> Response {
    // Authority refusals remain authoritative even if the Owner is retired.
    if matches!(code, "invalid_credentials" | "access_denied") {
        return response(
            if code == "invalid_credentials" {
                StatusCode::UNAUTHORIZED
            } else {
                StatusCode::FORBIDDEN
            },
            json!({"ok":false,"errors":["EXTRACTION_FAILED"]}),
        );
    }
    let (status, safe) = if unknown {
        (StatusCode::SERVICE_UNAVAILABLE, "outcome_unknown")
    } else {
        match code {
            "invalid_credentials" => (StatusCode::UNAUTHORIZED, "EXTRACTION_FAILED"),
            "access_denied" => (StatusCode::FORBIDDEN, "EXTRACTION_FAILED"),
            "EXTRACTION_PROVIDER_UNAVAILABLE" => (
                StatusCode::SERVICE_UNAVAILABLE,
                "EXTRACTION_PROVIDER_UNAVAILABLE",
            ),
            "EXTRACTION_SNAPSHOT_INVALID" => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "EXTRACTION_SNAPSHOT_INVALID",
            ),
            "EXTRACTION_SNAPSHOT_HASH_MISMATCH" => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "EXTRACTION_SNAPSHOT_HASH_MISMATCH",
            ),
            "EXTRACTION_SNAPSHOT_UNAVAILABLE" => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "EXTRACTION_SNAPSHOT_UNAVAILABLE",
            ),
            "EXTRACTION_ADMISSION_CHANGED" => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "EXTRACTION_ADMISSION_CHANGED",
            ),
            _ => (StatusCode::UNPROCESSABLE_ENTITY, "EXTRACTION_FAILED"),
        }
    };
    response(status, json!({"ok":false,"errors":[safe]}))
}
fn checked<T>(
    attempt: rentemester_host::Attempt<
        Result<
            rentemester_application::control_fence::Committed<T>,
            rentemester_application::control_fence::Failure,
        >,
    >,
) -> Result<T, Box<Response>> {
    if attempt.retired {
        return Err(Box::new(domain_error("", true)));
    }
    match attempt.outcome {
        Ok(value) if value.control_reusable => Ok(value.value),
        Ok(_) => Err(Box::new(domain_error("", true))),
        Err(f) => Err(Box::new(domain_error(
            f.error.code,
            f.stage == FailureStage::OutcomeUnknown || !f.control_reusable || !f.company_reusable,
        ))),
    }
}
fn service_checked<T>(
    attempt: rentemester_host::Attempt<
        Result<
            rentemester_application::service_company::ServiceResult<T>,
            rentemester_application::service_company::ServiceFailure,
        >,
    >,
) -> Result<T, Box<Response>> {
    use rentemester_application::service_company::{BusinessStage, UsageStage};
    if attempt.retired {
        return Err(Box::new(domain_error("", true)));
    }
    match attempt.outcome {
        Ok(value) if value.control_reusable => Ok(value.result),
        Ok(_) => Err(Box::new(domain_error("", true))),
        Err(f) => Err(Box::new(domain_error(
            f.error.code,
            f.usage == UsageStage::OutcomeUnknown
                || f.business == BusinessStage::OutcomeUnknown
                || !f.control_reusable
                || !f.company_reusable,
        ))),
    }
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let path = request.uri().path().to_owned();
    let parts = path.split('/').collect::<Vec<_>>();
    let slug = parts.get(3).copied().unwrap_or("").to_owned();
    let write = request.method() == Method::POST;
    let cookie = match documents::credential(&state, &request, write) {
        Ok(value) => value,
        Err(response) => return *response,
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
    let Some(id) = parts
        .get(6)
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|id| (1..=9_007_199_254_740_991).contains(id))
    else {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    };
    if write
        && !one(request.headers(), "content-type")
            .is_some_and(|value| value.eq_ignore_ascii_case("application/json"))
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let bytes = match to_bytes(request.into_body(), if write { 4096 } else { 1 }).await {
        Ok(value) => value,
        Err(_) => {
            return refused(
                if write {
                    StatusCode::PAYLOAD_TOO_LARGE
                } else {
                    StatusCode::BAD_REQUEST
                },
                "invalid_input",
            );
        }
    };
    let expected_actor = if write {
        let Ok(value) = serde_json::from_slice::<Confirm>(&bytes) else {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        };
        if !value.confirm {
            return refused(StatusCode::BAD_REQUEST, "confirmation_required");
        }
        value.expected_actor
    } else {
        if !bytes.is_empty() {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        None
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        if !write {
            let mut owner = state.native.lock_company(&slug).map_err(|_| {
                Box::new(refused(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "owner_unavailable",
                ))
            })?;
            return match &cookie {
                documents::Credential::Human(proof) => checked(
                    owner
                        .original_invoice_extraction(proof, &SystemClock, id)
                        .map_err(owner_error)?,
                ),
                documents::Credential::Service(key) => service_checked(
                    owner
                        .invoice_extraction_service(key, &SystemClock, id)
                        .map_err(owner_error)?,
                ),
            };
        }
        let provider = state.native.invoice_extractor.as_ref();
        let admission = {
            let mut owner = state.native.lock_company(&slug).map_err(|_| {
                Box::new(refused(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "owner_unavailable",
                ))
            })?;
            let identity = provider.map(|value| value.identity().clone());
            match &cookie {
                documents::Credential::Human(proof) => checked(
                    owner
                        .prepare_original_invoice_extraction(
                            proof,
                            &SystemClock,
                            id,
                            identity,
                            expected_actor.as_deref(),
                        )
                        .map_err(owner_error)?,
                ),
                documents::Credential::Service(key) => service_checked(
                    owner
                        .prepare_invoice_extraction_service(
                            key,
                            &SystemClock,
                            id,
                            identity,
                            expected_actor.as_deref(),
                        )
                        .map_err(owner_error)?,
                ),
            }?
        }; // Both DB transactions and the external Owner mutex are released here.
        let completion = admission.provider_input().map(|input| {
            let provider = provider.expect("configured provider required by admitted prepare");
            Completion {
                identity: provider.identity().clone(),
                outcome: provider.extract(input),
            }
        });
        let mut owner = state.native.lock_company(&slug).map_err(|_| {
            Box::new(refused(
                StatusCode::SERVICE_UNAVAILABLE,
                "owner_unavailable",
            ))
        })?;
        match &cookie {
            documents::Credential::Human(proof) => checked(
                owner
                    .finish_invoice_extraction(proof, &SystemClock, admission, completion)
                    .map_err(owner_error)?,
            ),
            documents::Credential::Service(key) => service_checked(
                owner
                    .finish_invoice_extraction_service(key, &SystemClock, admission, completion)
                    .map_err(owner_error)?,
            ),
        }
    })
    .await;
    match result {
        Ok(Ok(value)) => response(StatusCode::OK, value),
        Ok(Err(response)) => *response,
        _ => refused(
            StatusCode::SERVICE_UNAVAILABLE,
            if write {
                "outcome_unknown"
            } else {
                "invoice_extraction_unavailable"
            },
        ),
    }
}

fn owner_error(_: rentemester_host::OwnerError) -> Box<Response> {
    Box::new(domain_error("", true))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_confirmation_is_strict() {
        assert!(
            serde_json::from_str::<Confirm>(r#"{"confirm":true}"#)
                .unwrap()
                .confirm
        );
        for value in [
            r#"{"confirm":true,"actor":"user:u"}"#,
            r#"{"confirm":true,"expectedActor":null}"#,
            r#"{"confirm":true,"expectedActor":" "}"#,
            r#"{"confirm":true,"confirm":true}"#,
        ] {
            assert!(serde_json::from_str::<Confirm>(value).is_err(), "{value}");
        }
    }
}
