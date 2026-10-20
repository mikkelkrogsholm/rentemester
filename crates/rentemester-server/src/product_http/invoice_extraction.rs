//! Explicit extraction: no Owner or SQL transaction survives provider I/O.
use super::*;
use axum::http::Method;
use rentemester_application::invoice_extraction::Completion;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Confirm {
    confirm: bool,
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
        return Err(Box::new(refused(
            StatusCode::SERVICE_UNAVAILABLE,
            "owner_unavailable",
        )));
    }
    if let Err(f) = &attempt.outcome {
        let status = match (f.stage, f.error.code) {
            (FailureStage::OutcomeUnknown, _) => StatusCode::SERVICE_UNAVAILABLE,
            (_, "invalid_credentials") => StatusCode::UNAUTHORIZED,
            (_, "access_denied") => StatusCode::FORBIDDEN,
            (_, "document_not_found") => StatusCode::NOT_FOUND,
            (
                _,
                "EXTRACTION_PROVIDER_UNAVAILABLE"
                | "EXTRACTION_SNAPSHOT_UNAVAILABLE"
                | "invoice_extraction_storage_unavailable",
            ) => StatusCode::SERVICE_UNAVAILABLE,
            (_, "invalid_input") => StatusCode::BAD_REQUEST,
            _ => StatusCode::UNPROCESSABLE_ENTITY,
        };
        return Err(Box::new(response(
            status,
            json!({"ok":false,"code":f.error.code,"errors":[f.error.message]}),
        )));
    }
    documents::outcome(attempt)
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let path = request.uri().path().to_owned();
    let parts = path.split('/').collect::<Vec<_>>();
    let slug = parts.get(3).copied().unwrap_or("").to_owned();
    let write = request.method() == Method::POST;
    let cookie = match proof(&state, &request, write) {
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
        .get(5)
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
    if write {
        if !serde_json::from_slice::<Confirm>(&bytes).is_ok_and(|value| value.confirm) {
            return refused(StatusCode::BAD_REQUEST, "confirmation_required");
        }
    } else if !bytes.is_empty() {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
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
            return checked(
                owner
                    .invoice_extraction(&cookie, &SystemClock, id)
                    .map_err(|_| {
                        Box::new(refused(
                            StatusCode::SERVICE_UNAVAILABLE,
                            "owner_unavailable",
                        ))
                    })?,
            );
        }
        let provider = state.native.invoice_extractor.as_ref();
        let admission = {
            let mut owner = state.native.lock_company(&slug).map_err(|_| {
                Box::new(refused(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "owner_unavailable",
                ))
            })?;
            checked(
                owner
                    .prepare_invoice_extraction(
                        &cookie,
                        &SystemClock,
                        id,
                        provider.map(|value| value.identity().clone()),
                    )
                    .map_err(|_| {
                        Box::new(refused(
                            StatusCode::SERVICE_UNAVAILABLE,
                            "owner_unavailable",
                        ))
                    })?,
            )?
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
        checked(
            owner
                .finish_invoice_extraction(&cookie, &SystemClock, admission, completion)
                .map_err(|_| {
                    Box::new(refused(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "owner_unavailable",
                    ))
                })?,
        )
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
#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Method;
    #[test]
    fn confirmation_is_exact_and_repeated_fields_are_not_admitted() {
        assert!(
            serde_json::from_str::<Confirm>(r#"{"confirm":true}"#)
                .unwrap()
                .confirm
        );
        for body in [
            r#"{"confirm":true,"actor":"user:other"}"#,
            r#"{"confirm":true,"confirm":true}"#,
            r#"{"provider":"caller"}"#,
        ] {
            assert!(serde_json::from_str::<Confirm>(body).is_err());
        }
        assert!(
            !serde_json::from_str::<Confirm>(r#"{"confirm":false}"#)
                .unwrap()
                .confirm
        );
    }
}
