//! Strict original source wire over the current admitted accrual owner.
use super::*;
use rentemester_application::accrual_original::{Action, Recognize, Register};
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
fn checked<T>(
    attempt: rentemester_host::Attempt<
        Result<
            rentemester_application::control_fence::Committed<T>,
            rentemester_application::control_fence::Failure,
        >,
    >,
) -> Result<T, Box<Response>> {
    if let Err(f) = &attempt.outcome
        && matches!(f.error.code, "invalid_credentials" | "access_denied")
    {
        return Err(Box::new(domain_error(f.error.code, false)));
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
    if let Err(f) = &attempt.outcome
        && matches!(f.error.code, "invalid_credentials" | "access_denied")
    {
        return Err(Box::new(domain_error(f.error.code, false)));
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
fn domain_error(code: &str, unknown: bool) -> Response {
    let status = match code {
        "invalid_credentials" => StatusCode::UNAUTHORIZED,
        "access_denied" => StatusCode::FORBIDDEN,
        _ if unknown => StatusCode::SERVICE_UNAVAILABLE,
        "invalid_input" | "confirmation_required" => StatusCode::BAD_REQUEST,
        "accrual_refused"
        | "invoice_refused"
        | "sales_invoice_refused"
        | "journal_refused"
        | "document_evidence_unavailable"
        | "invalid_account"
        | "period_closed" => StatusCode::UNPROCESSABLE_ENTITY,
        _ => StatusCode::SERVICE_UNAVAILABLE,
    };
    let safe = match status {
        StatusCode::UNAUTHORIZED => "unauthorized",
        StatusCode::FORBIDDEN => "access_denied",
        StatusCode::BAD_REQUEST => "invalid_input",
        StatusCode::UNPROCESSABLE_ENTITY => {
            "Accrual operation refused by current accounting or evidence controls"
        }
        _ => "outcome_unknown",
    };
    response(
        status,
        json!({"ok":false,"appliedRules":["DK-BOOKKEEPING-ACCRUAL-001"],"errors":[safe]}),
    )
}
enum Command {
    Report,
    Register(Register),
    Recognize(i64, Recognize),
    Invalid,
}
impl Command {
    fn action(&self) -> Action<'_> {
        match self {
            Self::Report => Action::Report,
            Self::Register(v) => Action::Register(v),
            Self::Recognize(id, v) => Action::Recognize(*id, v),
            Self::Invalid => Action::Invalid,
        }
    }
}
fn valid_fields(fields: &Value, register: bool) -> bool {
    let Some(m) = fields.as_object() else {
        return false;
    };
    if m.iter().any(|(k, v)| {
        v.is_null()
            || !if register {
                matches!(
                    k.as_str(),
                    "accrualType"
                        | "description"
                        | "totalAmount"
                        | "recognitionPeriods"
                        | "firstRecognitionDate"
                        | "resultAccountNo"
                        | "registrationDate"
                        | "periodStepMonths"
                        | "balanceAccountNo"
                        | "settlementAccountNo"
                        | "documentId"
                        | "note"
                )
            } else {
                matches!(k.as_str(), "period" | "date" | "settlementAccountNo")
            }
    }) {
        return false;
    }
    for (k, v) in m {
        match k.as_str() {
            "documentId" => {
                if !v
                    .as_i64()
                    .is_some_and(|n| (1..=9_007_199_254_740_991).contains(&n))
                {
                    return false;
                }
            }
            "recognitionPeriods" | "periodStepMonths" | "period" => {
                if !v.as_u64().is_some_and(|n| n > 0 && n <= u32::MAX as u64) {
                    return false;
                }
            }
            "totalAmount" => {
                if !v.is_number() {
                    return false;
                }
            }
            "date" | "firstRecognitionDate" | "registrationDate" => {
                if v.as_str()
                    .is_none_or(|s| rentemester_engine::fiscal_year::Date::parse(s).is_err())
                {
                    return false;
                }
            }
            _ => {
                if !v.as_str().is_some_and(|s| {
                    s.len() <= 4096
                        && (k == "note" || !s.trim().is_empty())
                        && !s
                            .chars()
                            .any(|c| c.is_control() && !(k == "note" && matches!(c, '\n' | '\t')))
                }) {
                    return false;
                }
            }
        }
    }
    true
}
fn parse(bytes: &[u8], register: bool, id: Option<i64>) -> (Command, Option<String>) {
    let Ok(wire) = serde_json::from_slice::<Write>(bytes) else {
        return (Command::Invalid, None);
    };
    if !wire.confirm || !valid_fields(&wire.fields, register) {
        return (Command::Invalid, None);
    }
    let command = if register {
        serde_json::from_value(wire.fields)
            .map(Command::Register)
            .unwrap_or(Command::Invalid)
    } else if let Some(id) = id {
        serde_json::from_value(wire.fields)
            .map(|v| Command::Recognize(id, v))
            .unwrap_or(Command::Invalid)
    } else {
        Command::Invalid
    };
    (command, wire.expected_actor)
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let write = request.method() == axum::http::Method::POST;
    let credential = match documents::credential(&state, &request, write) {
        Ok(v) => v,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    let path = request.uri().path().to_owned();
    let slug = path.split('/').nth(3).unwrap_or("").to_owned();
    let register = path.ends_with("/register");
    let id = path
        .split('/')
        .nth(6)
        .and_then(|s| s.parse::<i64>().ok())
        .filter(|n| (1..=9_007_199_254_740_991).contains(n));
    let headers_valid = request.uri().query().is_none()
        && !request.headers().contains_key("content-encoding")
        && !request.headers().contains_key("idempotency-key")
        && (!write
            || one(request.headers(), "content-type")
                .is_some_and(|v| v.eq_ignore_ascii_case("application/json")));
    let bytes = to_bytes(request.into_body(), if write { 16_384 } else { 1 }).await;
    // Even malformed JSON is refused only after current authority is established.
    let (command, expected) = match bytes {
        Ok(bytes) if headers_valid && write => parse(&bytes, register, id),
        Ok(bytes) if headers_valid && !write && bytes.is_empty() => (Command::Report, None),
        _ => (Command::Invalid, None),
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state
            .native
            .lock_company(&slug)
            .map_err(|_| Box::new(domain_error("", true)))?;
        match credential {
            documents::Credential::Human(cookie) => checked(
                owner
                    .original_accrual(&cookie, &SystemClock, command.action(), expected.as_deref())
                    .map_err(|_| Box::new(domain_error("", true)))?,
            ),
            documents::Credential::Service(key) => service_checked(
                owner
                    .original_accrual_service(
                        &key,
                        &SystemClock,
                        command.action(),
                        expected.as_deref(),
                    )
                    .map_err(|_| Box::new(domain_error("", true)))?,
            ),
        }
    })
    .await;
    match result {
        Ok(Ok(value)) => response(
            if value["ok"] == false {
                StatusCode::UNPROCESSABLE_ENTITY
            } else {
                StatusCode::OK
            },
            value,
        ),
        Ok(Err(r)) => *r,
        _ => domain_error("", true),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_source_wire() {
        let base = json!({"confirm":true,"accrualType":"prepaid_expense","description":"Synthetic","totalAmount":0.01,"recognitionPeriods":2,"firstRecognitionDate":"2026-01-01","resultAccountNo":"2000"});
        assert!(matches!(
            parse(&serde_json::to_vec(&base).unwrap(), true, None).0,
            Command::Register(_)
        ));
        for (key, value) in [
            ("documentId", Value::Null),
            ("documentId", json!(9_007_199_254_740_992_i64)),
            ("actor", json!("user:forged")),
            ("expectedActor", Value::Null),
            ("note", Value::Null),
            ("confirm", json!(false)),
            ("totalAmount", json!("1.00")),
            ("idempotencyKey", json!("key")),
        ] {
            let mut v = base.clone();
            v[key] = value;
            assert!(
                matches!(
                    parse(&serde_json::to_vec(&v).unwrap(), true, None).0,
                    Command::Invalid
                ),
                "{key}"
            );
        }
        assert!(matches!(
            parse(
                br#"{"confirm":true,"confirm":true,"period":1}"#,
                false,
                Some(1)
            )
            .0,
            Command::Invalid
        ));
        assert!(matches!(
            parse(
                br#"{"confirm":true,"period":1,"date":"2026-01-01"}"#,
                false,
                Some(1)
            )
            .0,
            Command::Recognize(1, _)
        ));
        assert!(matches!(
            parse(br#"{"confirm":true,"periodIndex":1}"#, false, Some(1)).0,
            Command::Invalid
        ));
    }
    #[tokio::test]
    async fn safe_failures_and_authority_precedence() {
        assert_eq!(
            domain_error("access_denied", true).status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            domain_error("invalid_credentials", true).status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            domain_error("accrual_refused", false).status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        let r = domain_error("private SQL path", false);
        assert_eq!(r.status(), StatusCode::SERVICE_UNAVAILABLE);
        let b = to_bytes(r.into_body(), 4096).await.unwrap();
        assert!(!String::from_utf8_lossy(&b).contains("private SQL"));
    }
}
