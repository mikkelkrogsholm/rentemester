//! Exact original diagnostic HTTP envelopes, including meaningful business-false data.
use super::*;
fn date_query(query: Option<&str>, health: bool) -> Result<Option<String>, ()> {
    if health {
        return if query.is_none() { Ok(None) } else { Err(()) };
    }
    let mut date = None;
    for (key, value) in url::form_urlencoded::parse(query.unwrap_or("").as_bytes()) {
        if key != "asOf" || date.is_some() {
            return Err(());
        }
        date = Some(value.into_owned());
    }
    Ok(date)
}
fn auth_outcome(
    attempt: rentemester_host::Attempt<
        Result<Value, rentemester_application::password_login::Failure>,
    >,
) -> Result<Value, Box<Response>> {
    if attempt.retired {
        return Err(Box::new(refused(
            StatusCode::SERVICE_UNAVAILABLE,
            "owner_unavailable",
        )));
    }
    attempt.outcome.map_err(|f| {
        Box::new(refused(
            match f.error.code {
                "invalid_credentials" => StatusCode::UNAUTHORIZED,
                "access_denied" => StatusCode::FORBIDDEN,
                "invalid_company" | "company_not_found" => StatusCode::NOT_FOUND,
                _ => StatusCode::SERVICE_UNAVAILABLE,
            },
            f.error.code,
        ))
    })
}
fn human_outcome(
    attempt: rentemester_host::Attempt<
        Result<
            rentemester_application::control_fence::Committed<Value>,
            rentemester_application::control_fence::Failure,
        >,
    >,
) -> Result<Value, Box<Response>> {
    if attempt.retired {
        return Err(Box::new(refused(
            StatusCode::SERVICE_UNAVAILABLE,
            "owner_unavailable",
        )));
    }
    match attempt.outcome {
        Ok(v) if v.control_reusable => Ok(v.value),
        Ok(_) => Err(Box::new(refused(
            StatusCode::SERVICE_UNAVAILABLE,
            "read_unconfirmed",
        ))),
        Err(f) => Err(Box::new(refused(
            if !f.control_reusable || !f.company_reusable {
                StatusCode::SERVICE_UNAVAILABLE
            } else {
                match f.error.code {
                    "invalid_credentials" => StatusCode::UNAUTHORIZED,
                    "access_denied" => StatusCode::FORBIDDEN,
                    _ => StatusCode::SERVICE_UNAVAILABLE,
                }
            },
            f.error.code,
        ))),
    }
}
fn service_outcome(
    attempt: rentemester_host::Attempt<
        Result<
            rentemester_application::service_company::ServiceResult<Value>,
            rentemester_application::service_company::ServiceFailure,
        >,
    >,
) -> Result<Value, Box<Response>> {
    if attempt.retired {
        return Err(Box::new(refused(
            StatusCode::SERVICE_UNAVAILABLE,
            "owner_unavailable",
        )));
    }
    match attempt.outcome {
        Ok(v) if v.control_reusable => Ok(v.result),
        Ok(_) => Err(Box::new(refused(
            StatusCode::SERVICE_UNAVAILABLE,
            "read_unconfirmed",
        ))),
        Err(f) => Err(Box::new(service_company::failure(f))),
    }
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let slug = request
        .uri()
        .path()
        .split('/')
        .nth(3)
        .unwrap_or("")
        .to_owned();
    let health = request.uri().path().ends_with("/system-healthcheck");
    let credential = if service_company::requested(&request) {
        service_company::proof(&state, &request, false).map(documents::Credential::Service)
    } else {
        proof(&state, &request, false).map(documents::Credential::Human)
    };
    let credential = match credential {
        Ok(v) => v,
        Err(r) => return *r,
    };
    let date = match date_query(request.uri().query(), health) {
        Ok(v) => v,
        Err(()) => return refused(StatusCode::BAD_REQUEST, "invalid_query"),
    };
    if request.headers().contains_key("content-encoding")
        || request.headers().contains_key("transfer-encoding")
        || request.headers().contains_key("idempotency-key")
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    if !to_bytes(request.into_body(), 1)
        .await
        .is_ok_and(|b| b.is_empty())
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        if health {
            // Deliberately retain the admitted healthy anchor; NEVER open target ledger.
            let mut owner = state.native.owner.lock().map_err(|_| {
                Box::new(refused(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "owner_unavailable",
                ))
            })?;
            let attempt = match credential {
                documents::Credential::Human(cookie) => owner.system_healthcheck_original118(
                    &state.native.cookies,
                    &cookie,
                    &SystemClock,
                    &slug,
                ),
                documents::Credential::Service(key) => {
                    owner.system_healthcheck_original118_service(&key, &SystemClock, &slug)
                }
            }
            .map_err(|_| {
                Box::new(refused(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "owner_unavailable",
                ))
            })?;
            auth_outcome(attempt)
        } else {
            let mut owner = state.native.lock_company(&slug).map_err(|_| {
                Box::new(refused(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "owner_unavailable",
                ))
            })?;
            match credential {
                documents::Credential::Human(cookie) => human_outcome(
                    owner
                        .retention_status_original118(&cookie, &SystemClock, date.as_deref())
                        .map_err(|_| {
                            Box::new(refused(
                                StatusCode::SERVICE_UNAVAILABLE,
                                "owner_unavailable",
                            ))
                        })?,
                ),
                documents::Credential::Service(key) => service_outcome(
                    owner
                        .retention_status_original118_service(&key, &SystemClock, date.as_deref())
                        .map_err(|_| {
                            Box::new(refused(
                                StatusCode::SERVICE_UNAVAILABLE,
                                "owner_unavailable",
                            ))
                        })?,
                ),
            }
        }
    })
    .await;
    match result {
        Ok(Ok(core)) => response(
            StatusCode::OK,
            json!({"ok":true, (if health {"healthcheck"} else {"retentionStatus"}):core}),
        ),
        Ok(Err(r)) => *r,
        Err(_) => refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable"),
    }
}
#[cfg(test)]
#[path = "diagnostics_original118_tests.rs"]
mod tests;
