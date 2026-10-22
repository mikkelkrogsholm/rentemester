//! Original confirmed loss payload through shared sales admission.
use super::*;
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let credential = match documents::credential(&state, &request, true) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    if request.method() != axum::http::Method::POST
        || request.uri().query().is_some()
        || request.headers().contains_key("content-encoding")
        || request.headers().contains_key("transfer-encoding")
        || request.headers().contains_key("idempotency-key")
        || !one(request.headers(), "content-type")
            .is_some_and(|s| s.eq_ignore_ascii_case("application/json"))
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let path = request.uri().path().to_owned();
    let slug = path.split('/').nth(3).unwrap_or("").to_owned();
    let Ok(bytes) = to_bytes(request.into_body(), 256 * 1024).await else {
        return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
    };
    let body = match parse(&bytes) {
        Ok(v) => v,
        Err(code) => return refused(StatusCode::BAD_REQUEST, code),
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        let action = rentemester_application::sales::Action::OriginalBadDebt(&body);
        match credential {
            documents::Credential::Human(cookie) => {
                let attempt = owner
                    .sales(&cookie, &SystemClock, action, "", "", None)
                    .map_err(|_| ())?;
                Ok::<_, ()>(documents::outcome(attempt))
            }
            documents::Credential::Service(key) => {
                let attempt = owner
                    .sales_service(
                        &key,
                        &SystemClock,
                        rentemester_application::sales::Request {
                            action,
                            today: "",
                            now: "",
                            year: None,
                        },
                    )
                    .map_err(|_| ())?;
                Ok(match attempt.outcome {
                    Ok(done) => Ok(done.result),
                    Err(f) => {
                        let reply = service_company::failure(f);
                        Err(Box::new(reply))
                    }
                })
            }
        }
    })
    .await;
    match result {
        Ok(Ok(Ok(rentemester_application::sales::Output::Json(value)))) => {
            response(StatusCode::OK, value)
        }
        Ok(Ok(Err(r))) => *r,
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
    }
}

fn parse(bytes: &[u8]) -> Result<Value, &'static str> {
    let invoice_lifecycle::Unique(value) =
        serde_json::from_slice(bytes).map_err(|_| "invalid_input")?;
    rentemester_storage::invoice_bad_debt_original::validate(&value).map_err(|e| {
        if e.code == "confirmation_required" {
            "confirmation_required"
        } else {
            "invalid_input"
        }
    })?;
    Ok(value)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn loss_payload_rejects_duplicates_and_mixed_native_body() {
        assert!(parse(br#"{"payload":{"invoiceDocumentId":1,"writeOffDate":"2026-01-21","expenseAccountNo":"E"},"confirm":true}"#).is_ok());
        for raw in [
            r#"{"payload":{"invoiceDocumentId":1,"invoiceDocumentId":2,"writeOffDate":"2026-01-21","expenseAccountNo":"E"},"confirm":true}"#,
            r#"{"payload":{"invoiceDocumentId":1,"writeOffDate":"2026-01-21","expenseAccountNo":"E"},"body":"{}","confirm":true}"#,
        ] {
            assert!(parse(raw.as_bytes()).is_err());
        }
    }
}
