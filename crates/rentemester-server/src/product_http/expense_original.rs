//! Original expense: no Owner or SQL transaction survives provider I/O.
use super::*;
use axum::http::Method;
use rentemester_application::expense_original::{self as app, Input};
struct Write {
    confirm: bool,
    expected_actor: Option<String>,
    fields: Value,
}
impl<'de> Deserialize<'de> for Write {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Write;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("strict purchase object")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut map: M,
            ) -> Result<Write, M::Error> {
                use serde::de::Error;
                let mut fields = serde_json::Map::new();
                while let Some((key, value)) = map.next_entry::<String, Value>()? {
                    if fields.insert(key, value).is_some() {
                        return Err(M::Error::custom("duplicate field"));
                    }
                }
                let confirm = fields
                    .remove("confirm")
                    .and_then(|v| v.as_bool())
                    .ok_or_else(|| M::Error::custom("confirmation required"))?;
                let expected_actor = match fields.remove("expectedActor") {
                    None => None,
                    Some(Value::String(v))
                        if !v.trim().is_empty()
                            && v.trim() == v
                            && v.len() <= 160
                            && !v.chars().any(char::is_control) =>
                    {
                        Some(v)
                    }
                    _ => return Err(M::Error::custom("invalid actor assertion")),
                };
                Ok(Write {
                    confirm,
                    expected_actor,
                    fields: Value::Object(fields),
                })
            }
        }
        d.deserialize_map(Visitor)
    }
}
fn valid_input(v: &Value) -> bool {
    let Some(m) = v.as_object() else {
        return false;
    };
    if m.keys().any(|k| {
        !matches!(
            k.as_str(),
            "documentId"
                | "bankTransactionId"
                | "expenseAccount"
                | "vatTreatment"
                | "paymentAccount"
                | "date"
                | "text"
                | "idempotencyKey"
        )
    }) {
        return false;
    }
    for k in ["documentId", "bankTransactionId"] {
        if !m
            .get(k)
            .and_then(Value::as_i64)
            .is_some_and(|n| (1..=9_007_199_254_740_991).contains(&n))
        {
            return false;
        }
    }
    for k in [
        "expenseAccount",
        "vatTreatment",
        "paymentAccount",
        "date",
        "text",
        "idempotencyKey",
    ] {
        if let Some(v) = m.get(k) {
            if !v.as_str().is_some_and(|s| {
                !s.trim().is_empty() && s.len() <= 4096 && !s.chars().any(char::is_control)
            }) {
                return false;
            }
        }
    }
    m.get("expenseAccount").is_some()
        && m.get("vatTreatment").is_none_or(|v| {
            matches!(
                v.as_str(),
                Some(
                    "standard"
                        | "reverse_charge"
                        | "eu_goods_acquisition"
                        | "representation"
                        | "exempt"
                        | "non_deductible"
                )
            )
        })
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
            json!({"ok":false,"errors":["PURCHASE_FAILED"]}),
        );
    }
    let (status, safe) = if unknown {
        (StatusCode::SERVICE_UNAVAILABLE, "outcome_unknown")
    } else {
        match code {
            "invalid_input" => (StatusCode::BAD_REQUEST, "invalid_input"),
            "vies_invalid_config" => (StatusCode::SERVICE_UNAVAILABLE, "vies_invalid_config"),
            "document_evidence_unavailable" => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "document_evidence_unavailable",
            ),
            "purchase_admission_changed" => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "purchase_admission_changed",
            ),
            "idempotency_conflict" => (StatusCode::CONFLICT, "idempotency_conflict"),
            "idempotency_expired" => (StatusCode::CONFLICT, "idempotency_expired"),
            _ => (StatusCode::UNPROCESSABLE_ENTITY, "purchase_refused"),
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
    if let Err(f) = &attempt.outcome {
        if matches!(f.error.code, "invalid_credentials" | "access_denied") {
            return Err(Box::new(domain_error(f.error.code, false)));
        }
    }
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
    if let Err(f) = &attempt.outcome {
        if matches!(f.error.code, "invalid_credentials" | "access_denied") {
            return Err(Box::new(domain_error(f.error.code, false)));
        }
    }
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
    let booking = parts.get(6) == Some(&"book");
    let Some(id) = (if booking {
        Some(1)
    } else {
        parts
            .get(7)
            .and_then(|value| value.parse::<i64>().ok())
            .filter(|id| (1..=9_007_199_254_740_991).contains(id))
    }) else {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    };
    if write
        && !one(request.headers(), "content-type")
            .is_some_and(|value| value.eq_ignore_ascii_case("application/json"))
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let bytes = match to_bytes(request.into_body(), if write { 16_384 } else { 1 }).await {
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
    let (expected_actor, input) = if write {
        let Ok(value) = serde_json::from_slice::<Write>(&bytes) else {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        };
        if !value.confirm {
            return refused(StatusCode::BAD_REQUEST, "confirmation_required");
        }
        if booking {
            let Ok(input) = serde_json::from_value::<Input>(value.fields.clone()) else {
                return refused(StatusCode::BAD_REQUEST, "invalid_input");
            };
            if !valid_input(&value.fields) {
                return refused(StatusCode::BAD_REQUEST, "invalid_input");
            }
            (value.expected_actor, Some(input))
        } else {
            if value.fields.as_object().is_none_or(|m| !m.is_empty()) {
                return refused(StatusCode::BAD_REQUEST, "invalid_input");
            }
            (value.expected_actor, None)
        }
    } else {
        if !bytes.is_empty() {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        (None, None)
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
                        .original_expense_read(proof, &SystemClock, id)
                        .map_err(owner_error)?,
                ),
                documents::Credential::Service(key) => service_checked(
                    owner
                        .original_expense_read_service(key, &SystemClock, id)
                        .map_err(owner_error)?,
                ),
            };
        }
        if let Some(input) = input {
            let mut owner = state
                .native
                .lock_company(&slug)
                .map_err(|_| owner_error(rentemester_host::OwnerError::Retired))?;
            return match &cookie {
                documents::Credential::Human(proof) => checked(
                    owner
                        .original_expense_book(
                            proof,
                            &SystemClock,
                            &input,
                            expected_actor.as_deref(),
                        )
                        .map_err(owner_error)?,
                ),
                documents::Credential::Service(key) => service_checked(
                    owner
                        .original_expense_book_service(
                            key,
                            &SystemClock,
                            &input,
                            expected_actor.as_deref(),
                        )
                        .map_err(owner_error)?,
                ),
            };
        }
        let provider = &state.native.vies;
        let admission = {
            let mut owner = state.native.lock_company(&slug).map_err(|_| {
                Box::new(refused(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "owner_unavailable",
                ))
            })?;
            match &cookie {
                documents::Credential::Human(proof) => checked(
                    owner
                        .original_expense_prepare(
                            proof,
                            &SystemClock,
                            id,
                            expected_actor.as_deref(),
                        )
                        .map_err(owner_error)?,
                ),
                documents::Credential::Service(key) => service_checked(
                    owner
                        .original_expense_prepare_service(
                            key,
                            &SystemClock,
                            id,
                            expected_actor.as_deref(),
                        )
                        .map_err(owner_error)?,
                ),
            }?
        }; // Both DB transactions and the external Owner mutex are released here.
        let completion = admission
            .provider_vat()
            .map(|vat| app::observation(provider.validate(vat)))
            .transpose()
            .map_err(|e| Box::new(domain_error(e.code, false)))?;
        let mut owner = state.native.lock_company(&slug).map_err(|_| {
            Box::new(refused(
                StatusCode::SERVICE_UNAVAILABLE,
                "owner_unavailable",
            ))
        })?;
        match &cookie {
            documents::Credential::Human(proof) => checked(
                owner
                    .original_expense_finish(proof, &SystemClock, admission, completion)
                    .map_err(owner_error)?,
            ),
            documents::Credential::Service(key) => service_checked(
                owner
                    .original_expense_finish_service(key, &SystemClock, admission, completion)
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
                "purchase_unavailable"
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
    fn strict_writes_reject_forged_or_ambiguous_fields() {
        for raw in [
            r#"{"confirm":true,"confirm":true}"#,
            r#"{"confirm":true,"expectedActor":null}"#,
            r#"{"confirm":true,"expectedActor":" "}"#,
            r#"{"confirm":true,"expectedActor":12}"#,
        ] {
            assert!(serde_json::from_str::<Write>(raw).is_err(), "{raw}");
        }
        let w: Write =
            serde_json::from_str(r#"{"confirm":true,"expectedActor":"user:synthetic"}"#).unwrap();
        assert!(w.confirm);
        assert_eq!(w.expected_actor.as_deref(), Some("user:synthetic"));
        assert_eq!(w.fields, json!({}));
    }
    #[test]
    fn booking_has_six_treatments_and_strict_safe_ids() {
        let base = json!({"documentId":1,"bankTransactionId":2,"expenseAccount":"synthetic"});
        assert!(valid_input(&base));
        for treatment in [
            "standard",
            "reverse_charge",
            "eu_goods_acquisition",
            "representation",
            "exempt",
            "non_deductible",
        ] {
            let mut v = base.clone();
            v["vatTreatment"] = json!(treatment);
            assert!(valid_input(&v));
        }
        for (field, value) in [
            ("documentId", json!(0)),
            ("documentId", json!(9_007_199_254_740_992_i64)),
            ("bankTransactionId", json!(1.5)),
            ("vatTreatment", Value::Null),
            ("actor", json!("user:forged")),
            ("paymentAccount", Value::Null),
            ("path", json!("/private")),
        ] {
            let mut v = base.clone();
            v[field] = value;
            assert!(!valid_input(&v), "{field}");
        }
    }
}
