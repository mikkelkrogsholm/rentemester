//! Confirmed public-invoice operations. Every provider call happens between
//! fixed admitted Owner phases, never under the global Owner mutex.
use super::*;
use rentemester_application::public_einvoice::{self as app, Intent, Next, Outcome};
use rentemester_runtime::public_einvoice as provider;
fn environment(client: &provider::Client) -> &'static str {
    match client.environment() {
        provider::Environment::Test => "test",
        provider::Environment::Production => "production",
    }
}
fn failed(f: rentemester_application::control_fence::Failure) -> Response {
    let unknown = f.stage == FailureStage::OutcomeUnknown;
    let status = if unknown {
        StatusCode::SERVICE_UNAVAILABLE
    } else if matches!(f.error.code, "access_denied" | "invalid_credentials") {
        StatusCode::FORBIDDEN
    } else {
        StatusCode::UNPROCESSABLE_ENTITY
    };
    response(
        status,
        json!({"ok":false,"code":if unknown{"outcome_unknown"}else{f.error.code},"errors":[f.error.message]}),
    )
}
fn next(
    attempt: rentemester_host::Attempt<
        Result<
            rentemester_application::control_fence::Committed<Next>,
            rentemester_application::control_fence::Failure,
        >,
    >,
) -> Result<Next, Box<Response>> {
    if attempt.retired {
        return Err(Box::new(refused(
            StatusCode::SERVICE_UNAVAILABLE,
            "outcome_unknown",
        )));
    }
    match attempt.outcome {
        Ok(c) if c.control_reusable => Ok(c.value),
        Ok(_) => Err(Box::new(refused(
            StatusCode::SERVICE_UNAVAILABLE,
            "outcome_unknown",
        ))),
        Err(f) => Err(Box::new(failed(f))),
    }
}
// Storage binds canonical eight digits; the provider contract requires DK prefix.
fn provider_cvr(cvr: &str) -> String {
    format!("DK{cvr}")
}
fn observed(value: provider::Observation) -> app::Observation {
    app::Observation {
        document_id: value.document_id,
        status: value.status,
    }
}
fn valid_original(action: &str, body: &Value) -> bool {
    let Some(o) = body.as_object() else {
        return false;
    };
    let allowed: &[&str] = match action {
        "export-public" | "export-public-oioubl" => &["documentId", "invoiceNumber"],
        "submit-public-peppol" => &[
            "documentId",
            "invoiceNumber",
            "accessPoint",
            "acknowledgement",
            "confirm",
        ],
        "transmit-digisense" => &["documentId", "invoiceNumber", "companyKey", "confirm"],
        _ => return false,
    };
    if o.keys().any(|k| !allowed.contains(&k.as_str())) {
        return false;
    }
    if let Some(id) = o.get("documentId")
        && !id
            .as_i64()
            .is_some_and(|id| (1..=9_007_199_254_740_991).contains(&id))
    {
        return false;
    }
    if let Some(no) = o.get("invoiceNumber")
        && !no
            .as_str()
            .is_some_and(|s| !s.trim().is_empty() && s.len() <= 256)
    {
        return false;
    }
    if !o.contains_key("documentId") && !o.contains_key("invoiceNumber") {
        return false;
    }
    if action == "submit-public-peppol" {
        if body["confirm"] != true
            || serde_json::from_value::<rentemester_engine::peppol_handoff::AccessPoint>(
                body["accessPoint"].clone(),
            )
            .is_err()
        {
            return false;
        }
        if let Some(a) = o.get("acknowledgement")
            && serde_json::from_value::<rentemester_engine::peppol_handoff::Acknowledgement>(
                a.clone(),
            )
            .is_err()
        {
            return false;
        }
    }
    if action == "transmit-digisense"
        && (body["confirm"] != true
            || o.get("companyKey").is_some_and(|k| {
                !k.as_str()
                    .is_some_and(|s| !s.trim().is_empty() && s.len() <= 512)
            }))
    {
        return false;
    }
    true
}
fn original_result(value: Value) -> Value {
    let Some(mut row) = value.get("submission").cloned() else {
        return value;
    };
    row["ok"] = json!(true);
    row["appliedRules"] = json!(["DK-PEPPOL-SUBMIT-001"]);
    row["errors"] = json!([]);
    row
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let path = request.uri().path().to_owned();
    let slug = path.split('/').nth(3).unwrap_or("").to_owned();
    let read = request.method() == axum::http::Method::GET;
    let cookie = match proof(&state, &request, !read) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    if request.uri().query().is_some() || request.headers().contains_key("content-encoding") {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let original = path.contains("/invoices/original/");
    let mut artifact = None;
    let intent = if read {
        match to_bytes(request.into_body(), 1).await {
            Ok(b) if b.is_empty() => {}
            _ => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
        };
        None
    } else {
        if !one(request.headers(), "content-type")
            .is_some_and(|v| v.eq_ignore_ascii_case("application/json"))
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        let Ok(bytes) = to_bytes(request.into_body(), 16 * 1024).await else {
            return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
        };
        let Ok(body) = serde_json::from_slice::<Value>(&bytes) else {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        };
        if original {
            let action = path.rsplit('/').next().unwrap_or("");
            if !valid_original(action, &body) {
                return refused(StatusCode::BAD_REQUEST, "invalid_input");
            }
            if action == "transmit-digisense" {
                Some(Intent::OriginalSend(body))
            } else {
                artifact = Some((
                    match action {
                        "export-public" => app::OriginalAction::Preview,
                        "export-public-oioubl" => app::OriginalAction::OioUbl,
                        _ => app::OriginalAction::Prepare,
                    },
                    body,
                ));
                None
            }
        } else if path.ends_with("/public-register") {
            if body != json!({"confirm":true}) {
                return refused(StatusCode::BAD_REQUEST, "confirmation_required");
            }
            Some(Intent::Setup)
        } else {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct Input {
                invoice_document_id: i64,
                confirm: bool,
            }
            let Ok(input) = serde_json::from_value::<Input>(body) else {
                return refused(StatusCode::BAD_REQUEST, "invalid_input");
            };
            if !input.confirm || !(1..=9_007_199_254_740_991).contains(&input.invoice_document_id) {
                return refused(StatusCode::BAD_REQUEST, "confirmation_required");
            }
            Some(if path.ends_with("/status") {
                Intent::Status(input.invoice_document_id)
            } else {
                Intent::Send(input.invoice_document_id)
            })
        }
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let client = state.native.public_einvoice.get(&slug);
        if let Some((action, body)) = artifact {
            let Ok(mut owner) = state.native.lock_company(&slug) else {
                return refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable");
            };
            return match owner.original_public_einvoice(&cookie, &SystemClock, action, &body) {
                Ok(a) => match super::documents::outcome(a) {
                    Ok(v) => response(StatusCode::OK, v),
                    Err(r) => *r,
                },
                Err(_) => refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable"),
            };
        }
        let Some(intent) = intent else {
            let Ok(mut owner) = state.native.lock_company(&slug) else {
                return refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable");
            };
            return match if path.ends_with("/public-submissions") {
                owner.public_einvoice_submissions(&cookie, &SystemClock)
            } else {
                owner.public_einvoice_configuration(&cookie, &SystemClock, client.map(environment))
            } {
                Ok(a) => match super::documents::outcome(a) {
                    Ok(v) => response(StatusCode::OK, v),
                    Err(r) => *r,
                },
                Err(_) => refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable"),
            };
        };
        match drive(&state, &slug, &cookie, intent) {
            Ok(value) => response(
                StatusCode::OK,
                if original {
                    original_result(value)
                } else {
                    value
                },
            ),
            Err(failure) => *failure,
        }
    })
    .await;
    match result {
        Ok(r) => r,
        Err(_) => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unknown"),
    }
}

fn driver_refusal(status: StatusCode, code: &str) -> Box<Response> {
    Box::new(refused(status, code))
}

/// Shared admitted provider phases for direct commands and explicit workspace runs.
/// No caller-supplied endpoint, credentials, retry, or held Owner lock crosses transport.
pub(super) fn drive(
    state: &Product,
    slug: &str,
    cookie: &rentemester_runtime::auth::VerifiedCookie,
    intent: Intent,
) -> Result<Value, Box<Response>> {
    let Some(client) = state.native.public_einvoice.get(slug) else {
        return Err(driver_refusal(
            StatusCode::UNPROCESSABLE_ENTITY,
            "public_einvoice_config_required",
        ));
    };
    let initial = {
        let Ok(mut owner) = state.native.lock_company(slug) else {
            return Err(driver_refusal(
                StatusCode::SERVICE_UNAVAILABLE,
                "owner_unavailable",
            ));
        };
        match owner.start_public_einvoice(cookie, &SystemClock, intent, environment(client)) {
            Ok(a) => a,
            Err(_) => {
                return Err(driver_refusal(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "owner_unavailable",
                ));
            }
        }
    };
    let mut work = next(initial)?;
    // Setup has separate discovery and registration calls for company/outbound.
    // Each result crosses current authority before the next network action.
    // Send uses validation + delivery; status is one read. Never retry a phase.
    for _ in 0..5 {
        let admission = match work {
            Next::Complete(v) => {
                return Ok(v);
            }
            Next::Pending(a) => *a,
        };
        let outcome = match admission.request() {
            app::Request::FindCompany { cvr, name } => {
                let found = match client.registered_company(&provider_cvr(cvr), name) {
                    Ok(Some(existing)) => app::Discovery::Found(existing.company_key),
                    Ok(None) => app::Discovery::Missing,
                    Err(_) => app::Discovery::Failed,
                };
                Outcome::CompanyFound(found)
            }
            app::Request::Company { cvr, name } => Outcome::Company(
                client
                    .register_company(&provider_cvr(cvr), name)
                    .ok()
                    .map(|v| v.company_key),
            ),
            app::Request::FindOutbound {
                cvr,
                name,
                company_key,
            } => {
                let found = match client.registered_outbound(company_key, &provider_cvr(cvr), name)
                {
                    Ok(Some(existing))
                        if existing.registered_on_network && !existing.webhook_registered =>
                    {
                        app::Discovery::Found(())
                    }
                    Ok(Some(_)) => app::Discovery::Failed,
                    Ok(None) => app::Discovery::Missing,
                    Err(_) => app::Discovery::Failed,
                };
                Outcome::OutboundFound(found)
            }
            app::Request::Outbound {
                cvr,
                name: _,
                company_key,
            } => Outcome::Outbound(
                client
                    .register_outbound(
                        company_key,
                        &provider_cvr(cvr),
                        provider::Network::Nemhandel,
                    )
                    .is_ok_and(|v| v.registered_on_network && !v.webhook_registered),
            ),
            app::Request::Validate { xml } => {
                Outcome::Validated(client.validate(xml).is_ok_and(|v| v.valid))
            }
            app::Request::Deliver { xml, company_key } => {
                Outcome::Delivered(match client.deliver(company_key, xml) {
                    provider::DeliveryOutcome::Observed(o) => Some(observed(o)),
                    provider::DeliveryOutcome::Unknown => None,
                })
            }
            app::Request::Observe {
                company_key,
                document_id,
            } => Outcome::Observed(client.status(company_key, document_id).ok().map(observed)),
        };
        let attempt = {
            let Ok(mut owner) = state.native.lock_company(slug) else {
                return Err(driver_refusal(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "outcome_unknown",
                ));
            };
            match owner.finish_public_einvoice(cookie, &SystemClock, admission, outcome) {
                Ok(a) => a,
                Err(_) => {
                    return Err(driver_refusal(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "outcome_unknown",
                    ));
                }
            }
        };
        work = next(attempt)?;
    }
    Err(driver_refusal(
        StatusCode::SERVICE_UNAVAILABLE,
        "outcome_unknown",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_ledger_identity_is_encoded_for_both_provider_registration_steps() {
        assert_eq!(provider_cvr("12345678"), "DK12345678");
    }
}

#[cfg(test)]
mod original_tests {
    use super::*;
    #[test]
    fn strict_original_selectors_confirmation_and_metadata_boundary() {
        assert!(valid_original(
            "export-public",
            &json!({"documentId":1,"invoiceNumber":"n"})
        ));
        assert!(valid_original(
            "transmit-digisense",
            &json!({"invoiceNumber":"n","confirm":true,"companyKey":"configured-assertion"})
        ));
        assert!(valid_original(
            "submit-public-peppol",
            &json!({"documentId":1,"confirm":true,"accessPoint":{"accessPointId":"a","endpointUrl":"metadata:only","senderEndpointId":"b"}})
        ));
        for body in [
            json!({}),
            json!({"documentId":0}),
            json!({"documentId":9007199254740992i64}),
            json!({"invoiceNumber":" "}),
            json!({"documentId":1,"outPath":"local"}),
            json!({"documentId":1,"apiKey":"secret"}),
        ] {
            assert!(!valid_original("export-public", &body));
        }
        for body in [
            json!({"documentId":1}),
            json!({"documentId":1,"confirm":false}),
            json!({"documentId":1,"confirm":true,"companyKey":" "}),
            json!({"documentId":1,"confirm":true,"endpointUrl":"https://arbitrary.invalid"}),
        ] {
            assert!(!valid_original("transmit-digisense", &body));
        }
    }
    #[test]
    fn original_transmit_projection_preserves_actual_native_key_and_uncertain_evidence() {
        let r = original_result(
            json!({"ok":true,"submission":{"idempotencyKey":"native-key","submissionReference":"native-ref","envelope":"native-envelope","status":"uncertain","transmissionId":null,"duplicate":false}}),
        );
        assert_eq!(r["idempotencyKey"], "native-key");
        assert_eq!(r["envelope"], "native-envelope");
        assert_eq!(r["status"], "uncertain");
        assert_eq!(r["duplicate"], false);
        assert!(r["transmissionId"].is_null());
    }
}
