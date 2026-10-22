//! Additive original claim transport through the existing admitted sales owner.
use super::*;
use rentemester_storage::invoice_claims_original::{self as storage, Kind};
fn original_foreign_refusal(
    error: &rentemester_engine::Error,
    not_committed: bool,
) -> Option<Response> {
    (not_committed && error.code == "unsupported_foreign_interest").then(|| {
        response(
            StatusCode::OK,
            json!({"ok":false,"code":error.code,"errors":[error.message],"appliedRules":[]}),
        )
    })
}
fn parse(path: &str, bytes: &[u8]) -> Result<(Kind, Value), &'static str> {
    let kind = match path.rsplit('/').next().unwrap_or("") {
        "remind" => Kind::Remind,
        "post-reminder" => Kind::PostReminder,
        "compensation" => Kind::Compensation,
        "claim-compensation" => Kind::ClaimCompensation,
        "post-compensation" => Kind::PostCompensation,
        "interest" => Kind::Interest,
        "claim-interest" => Kind::ClaimInterest,
        "post-interest" => Kind::PostInterest,
        "interest-correction" => Kind::InterestCorrection,
        "post-interest-correction" => Kind::PostInterestCorrection,
        "settle-claim-bank" => Kind::SettleClaimBank,
        _ => return Err("invalid_input"),
    };
    if bytes.len() > 256 * 1024 {
        return Err("invalid_input");
    }
    let invoice_lifecycle::Unique(value) =
        serde_json::from_slice(bytes).map_err(|_| "invalid_input")?;
    storage::validate(kind, &value).map_err(|e| {
        if e.code == "confirmation_required" {
            "confirmation_required"
        } else {
            "invalid_input"
        }
    })?;
    Ok((kind, value))
}
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
    let (kind, body) = match parse(&path, &bytes) {
        Ok(v) => v,
        Err(code) => return refused(StatusCode::BAD_REQUEST, code),
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        let action = rentemester_application::sales::Action::OriginalClaims(kind, &body);
        match credential {
            documents::Credential::Human(cookie) => {
                let attempt = owner
                    .sales(&cookie, &SystemClock, action, "", "", None)
                    .map_err(|_| ())?;
                if let Err(f) = &attempt.outcome
                    && let Some(reply) = original_foreign_refusal(
                        &f.error,
                        f.stage == rentemester_application::control_fence::FailureStage::NotCommitted,
                    )
                {
                    return Ok(Err(Box::new(reply)));
                }
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
                        let reply = original_foreign_refusal(
                            &f.error,
                            f.business == rentemester_application::service_company::BusinessStage::NotCommitted,
                        ).unwrap_or_else(|| service_company::failure(f));
                        Err(Box::new(reply))
                    },
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
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_foreign_interest_refusal_preserves_code_only_after_definite_rollback() {
        let error = rentemester_engine::Error::new(
            "unsupported_foreign_interest",
            "Foreign interest is unsupported",
        );
        assert!(original_foreign_refusal(&error, false).is_none());
        assert!(
            original_foreign_refusal(
                &rentemester_engine::Error::new("access_denied", "Denied"),
                true
            )
            .is_none()
        );
        let reply = original_foreign_refusal(&error, true).unwrap();
        assert_eq!(reply.status(), StatusCode::OK);
        let bytes = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(axum::body::to_bytes(reply.into_body(), 262144))
            .unwrap();
        let wire: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            wire,
            json!({"ok":false,"code":"unsupported_foreign_interest","errors":["Foreign interest is unsupported"],"appliedRules":[]})
        );
    }
    #[test]
    fn original_claim_routes_strict_duplicate_unknown_null_and_confirmation() {
        for (path, raw) in [
            (
                "remind",
                r#"{"documentId":1,"date":"2026-01-12","confirm":true}"#,
            ),
            (
                "post-reminder",
                r#"{"documentId":1,"receivableAccountNo":"R","reminderIncomeAccountNo":"I","confirm":true}"#,
            ),
            ("compensation", r#"{"documentId":1,"asOf":"2026-01-12"}"#),
            (
                "claim-compensation",
                r#"{"documentId":1,"asOf":"2026-01-12","confirm":true}"#,
            ),
            (
                "post-compensation",
                r#"{"documentId":1,"receivableAccountNo":"R","compensationIncomeAccountNo":"I","confirm":true}"#,
            ),
            ("interest", r#"{"documentId":1,"asOf":"2026-01-12"}"#),
            (
                "claim-interest",
                r#"{"documentId":1,"asOf":"2026-01-12","confirm":true}"#,
            ),
            (
                "post-interest",
                r#"{"documentId":1,"receivableAccountNo":"R","interestIncomeAccountNo":"I","confirm":true}"#,
            ),
            ("interest-correction", r#"{"invoiceNumber":"synthetic"}"#),
            (
                "post-interest-correction",
                r#"{"documentId":1,"reason":"Synthetic correction","confirm":true}"#,
            ),
            (
                "settle-claim-bank",
                r#"{"payload":{"invoiceDocumentId":1,"bankTransactionReference":"synthetic","bankAccountNo":"A"},"confirm":true}"#,
            ),
        ] {
            assert!(parse(path, raw.as_bytes()).is_ok(), "{path}");
        }
        for raw in [
            r#"{"documentId":1,"asOf":"2026-01-12","asOf":"2026-01-13"}"#,
            r#"{"documentId":1,"asOf":"2026-01-12","actor":"user:other"}"#,
            r#"{"documentId":1,"asOf":"2026-01-12","referenceRate":null}"#,
            r#"{"documentId":1,"asOf":"2026-01-12","confirm":true}"#,
        ] {
            assert!(parse("interest", raw.as_bytes()).is_err());
        }
        assert!(parse("settle-claim-bank",br#"{"payload":{"invoiceDocumentId":1,"bankTransactionId":1,"bankTransactionId":2},"confirm":true}"#).is_err());
        assert!(
            parse(
                "remind",
                br#"{"documentId":1,"date":"2026-01-12","confirm":false}"#
            )
            .is_err()
        );
        assert!(parse("interest", &vec![b' '; 256 * 1024 + 1]).is_err());
    }
}
