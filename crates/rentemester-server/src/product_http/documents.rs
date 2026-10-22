use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use rentemester_application::documents::ScanPolicy;
use rentemester_engine::document::Metadata;
use rentemester_runtime::document::Prepared;
use rentemester_runtime::document_scanner::{ScanFailure, Scanner};
pub(super) enum Credential {
    Human(VerifiedCookie),
    Service(rentemester_runtime::service_key::ServiceKey),
}
pub(super) fn credential(
    state: &Product,
    request: &Request,
    mutation: bool,
) -> Result<Credential, Box<Response>> {
    if super::service_company::requested(request) {
        if !mutation
            && (request.headers().contains_key("content-encoding")
                || request.headers().contains_key("transfer-encoding")
                || request.headers().contains_key("idempotency-key")
                || one(request.headers(), "content-length").is_some_and(|v| v != "0"))
        {
            return Err(Box::new(refused(StatusCode::BAD_REQUEST, "invalid_input")));
        }
        super::service_company::proof(state, request, mutation).map(Credential::Service)
    } else {
        proof(state, request, mutation).map(Credential::Human)
    }
}
pub(super) fn service_outcome<T>(
    attempt: rentemester_host::Attempt<
        Result<
            rentemester_application::service_company::ServiceResult<T>,
            rentemester_application::service_company::ServiceFailure,
        >,
    >,
) -> Result<T, Box<Response>> {
    attempt
        .outcome
        .map(|v| v.result)
        .map_err(|f| Box::new(super::service_company::failure(f)))
}
pub(super) fn service_envelope(
    attempt: rentemester_host::Attempt<
        Result<
            rentemester_application::service_company::ServiceResult<Value>,
            rentemester_application::service_company::ServiceFailure,
        >,
    >,
    envelope: Option<&str>,
) -> Result<Value, Box<Response>> {
    match attempt.outcome {
        Ok(value) => {
            let mut body = match envelope {
                Some(key) => json!({"ok":true,key:value.result}),
                None => value.result,
            };
            body["usage"] = json!(value.usage);
            body["business"] = json!(value.business);
            Ok(body)
        }
        Err(f) => Err(Box::new(super::service_company::failure(f))),
    }
}
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
        Err(f) if f.error.code == "access_denied" && f.stage == FailureStage::NotCommitted => {
            Err(Box::new(refused(StatusCode::FORBIDDEN, "access_denied")))
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
    let original_surface = request.uri().path().contains("/documents/original/");
    let company_slug = request
        .uri()
        .path()
        .split('/')
        .nth(3)
        .unwrap_or("")
        .to_owned();
    let cookie = match credential(&state, &request, true) {
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
    if input.vendor_id.is_some_and(|id| id <= 0) {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    // Keep the request bounded across both effects, but retain the known upload
    // receipt outside the later provider task (even if that task panics).
    let cookie = Arc::new(cookie);
    let upload_state = state.clone();
    let upload_cookie = cookie.clone();
    let upload_slug = company_slug.clone();
    let result = tokio::task::spawn_blocking(move || {
        let state = upload_state;
        let cookie = upload_cookie;
        let company_slug = upload_slug;
        let bytes = STANDARD
            .decode(input.file_base64)
            .map_err(|_| refused(StatusCode::BAD_REQUEST, "invalid_base64"))?;
        let file = Prepared::new(input.file_name, bytes).map_err(|message| {
            response(
                StatusCode::UNPROCESSABLE_ENTITY,
                json!({"ok":false,"code":"invalid_document_file","errors":[message]}),
            )
        })?;
        let admission = {
            let mut owner = state
                .native
                .lock_company(&company_slug)
                .map_err(|_| refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable"))?;
            let policy = if state.native.document_scanner.is_some() {
                ScanPolicy::Required
            } else {
                ScanPolicy::Off
            };
            match cookie.as_ref() {
                Credential::Human(cookie) => {
                    let attempt = owner
                        .prepare_documents_original(
                            cookie,
                            &SystemClock,
                            file,
                            input.metadata,
                            policy,
                            rentemester_application::documents::OriginalOptions {
                                vendor_id: input.vendor_id,
                                force: input.force,
                                original_api: original_surface,
                            },
                        )
                        .map_err(|_| {
                            refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable")
                        })?;
                    if attempt.retired {
                        return Err(Box::new(refused(
                            StatusCode::SERVICE_UNAVAILABLE,
                            "owner_unavailable",
                        )));
                    }
                    outcome(attempt)?
                }
                Credential::Service(key) => {
                    let attempt = owner
                        .prepare_documents_original_service(
                            key,
                            &SystemClock,
                            file,
                            input.metadata,
                            policy,
                            rentemester_application::documents::OriginalOptions {
                                vendor_id: input.vendor_id,
                                force: input.force,
                                original_api: original_surface,
                            },
                        )
                        .map_err(|_| {
                            refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable")
                        })?;
                    if attempt.retired {
                        return Err(Box::new(refused(
                            StatusCode::SERVICE_UNAVAILABLE,
                            "owner_unavailable",
                        )));
                    }
                    if original_surface {
                        super::documents_original::service_outcome(attempt)?
                    } else {
                        service_outcome(attempt)?
                    }
                }
            }
        }; // Release the global Owner mutex and all SQL transactions before provider I/O.
        let source_hash = admission.file().hash().to_owned();
        let scan = match &state.native.document_scanner {
            Some(scanner) => Some(scanner.scan(admission.file()).map_err(|failure| {
                Box::new(match failure {
                    ScanFailure::Rejected => refused(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        "document_scanner_rejected",
                    ),
                    ScanFailure::Unavailable => refused(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "document_scanner_unavailable",
                    ),
                })
            })?),
            None => None,
        };
        let mut owner = state
            .native
            .lock_company(&company_slug)
            .map_err(|_| refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable"))?;
        match cookie.as_ref() {
            Credential::Human(cookie) => {
                let attempt = owner
                    .finish_document_ingest_registered(cookie, &SystemClock, admission, scan)
                    .map_err(|_| refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable"))?;
                let retired = attempt.retired;
                let registered = outcome(attempt)?;
                let receipt = if original_surface {
                    super::documents_original::ingest_receipt(registered.receipt, &source_hash)?
                } else {
                    registered.receipt
                };
                Ok((receipt, Some(registered.upload), retired, permit))
            }
            Credential::Service(key) => {
                let attempt = owner
                    .finish_document_ingest_service(key, &SystemClock, admission, scan)
                    .map_err(|_| refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable"))?;
                let retired = attempt.retired;
                // Factual service ingestion does not grant human-only automatic extraction authority.
                let receipt = if original_surface {
                    super::documents_original::service_outcome(attempt)?
                } else {
                    service_envelope(attempt, None)?
                };
                let receipt = if original_surface {
                    super::documents_original::ingest_receipt(receipt, &source_hash)?
                } else {
                    receipt
                };
                Ok((receipt, None, retired, permit))
            }
        }
    })
    .await;
    let (receipt, upload, retired, permit) = match result {
        Ok(Ok(value)) => value,
        Ok(Err(reply)) => return *reply,
        _ => return refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unconfirmed"),
    };
    if let Some(upload) = upload.filter(|upload| {
        !retired && upload.eligible_for_extraction() && state.native.invoice_extractor.is_some()
    }) {
        // Registration is already known committed. Extraction failure, revocation,
        // unsupported context, or task failure must never replace that receipt.
        let _ = tokio::task::spawn_blocking(move || -> Result<(), ()> {
            let _permit = permit;
            let Credential::Human(cookie) = cookie.as_ref() else {
                return Err(());
            };
            use rentemester_application::invoice_extraction::Completion;
            let provider = state.native.invoice_extractor.as_ref().ok_or(())?;
            let admission = {
                let mut owner = state.native.lock_company(&company_slug).map_err(|_| ())?;
                let attempt = owner
                    .prepare_uploaded_invoice_extraction(
                        cookie,
                        &SystemClock,
                        upload,
                        Some(provider.identity().clone()),
                    )
                    .map_err(|_| ())?;
                if attempt.retired {
                    return Err(());
                }
                outcome(attempt).map_err(|_| ())?
            }; // No Owner mutex or SQL writer survives provider I/O.
            let completion = admission.provider_input().map(|input| Completion {
                identity: provider.identity().clone(),
                outcome: provider.extract(input),
            });
            let mut owner = state.native.lock_company(&company_slug).map_err(|_| ())?;
            let attempt = owner
                .finish_invoice_extraction(cookie, &SystemClock, admission, completion)
                .map_err(|_| ())?;
            outcome(attempt).map_err(|_| ())?;
            Ok(())
        })
        .await;
    }
    response(StatusCode::OK, receipt)
}
pub(super) async fn list(State(state): State<Product>, request: Request) -> Response {
    let company_slug = request
        .uri()
        .path()
        .split('/')
        .nth(3)
        .unwrap_or("")
        .to_owned();
    let cookie = match credential(&state, &request, false) {
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
        Ok::<_, ()>(match cookie {
            Credential::Human(cookie) => {
                outcome(owner.documents(&cookie, &SystemClock).map_err(|_| ())?)
                    .map(|value| json!({"ok":true,"documents":value}))
            }
            Credential::Service(key) => service_envelope(
                owner
                    .documents_service(&key, &SystemClock)
                    .map_err(|_| ())?,
                Some("documents"),
            ),
        })
    })
    .await;
    match result {
        Ok(Ok(value)) => match value {
            Ok(value) => response(StatusCode::OK, value),
            Err(r) => *r,
        },
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "read_unconfirmed"),
    }
}
pub(super) async fn file(State(state): State<Product>, request: Request) -> Response {
    let company_slug = request
        .uri()
        .path()
        .split('/')
        .nth(3)
        .unwrap_or("")
        .to_owned();
    let cookie = match credential(&state, &request, false) {
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
        Ok::<_, ()>(match cookie {
            Credential::Human(cookie) => outcome(
                owner
                    .document_original(&cookie, &SystemClock, id)
                    .map_err(|_| ())?,
            ),
            Credential::Service(key) => service_outcome(
                owner
                    .document_original_service(&key, &SystemClock, id)
                    .map_err(|_| ())?,
            ),
        })
    })
    .await;
    match result {
        Ok(Ok(value)) => match value {
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
    let company_slug = request
        .uri()
        .path()
        .split('/')
        .nth(3)
        .unwrap_or("")
        .to_owned();
    use rentemester_application::expense::{Action, Input};
    let posting = request.method() == axum::http::Method::POST;
    let cookie = match credential(&state, &request, posting) {
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
        Ok::<_, ()>(match cookie {
            Credential::Human(cookie) => outcome(
                owner
                    .expense(&cookie, &SystemClock, action)
                    .map_err(|_| ())?,
            ),
            Credential::Service(key) => service_outcome(
                owner
                    .expense_service(&key, &SystemClock, action)
                    .map_err(|_| ())?,
            ),
        })
    })
    .await;
    match result {
        Ok(Ok(value)) => match value {
            Ok(value) => response(
                StatusCode::OK,
                json!({"ok":true,(if posting{"booking"}else{"options"}):value}),
            ),
            Err(r) => *r,
        },
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unconfirmed"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn upload_cannot_supply_scanner_policy_or_clean_evidence() {
        let valid = json!({"confirm":true,"fileName":"synthetic.txt","fileBase64":"WA==","metadata":{"source":"Synthetic","documentType":"cash_register_receipt"}});
        assert!(serde_json::from_value::<Upload>(valid.clone()).is_ok());
        for field in ["scanner", "scannerPolicy", "scanEvidence", "admission"] {
            let mut request = valid.clone();
            request[field] = json!({"ok":true,"scannerId":"caller"});
            assert!(serde_json::from_value::<Upload>(request).is_err());
            let mut request = valid.clone();
            request["metadata"][field] = json!({"ok":true,"scannerId":"caller"});
            assert!(serde_json::from_value::<Upload>(request).is_err());
        }
    }
}
