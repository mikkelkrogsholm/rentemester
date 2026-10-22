//! Strict original source wire over the current admitted asset owner.
use super::*;
use rentemester_application::asset_original::{Action, Depreciate, Register, WriteOff};
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
        "asset_refused"
        | "invalid_date"
        | "document_refused"
        | "account_role_refused"
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
            "Asset operation refused by current accounting or evidence controls"
        }
        _ => "outcome_unknown",
    };
    response(
        status,
        json!({"ok":false,"appliedRules":["DK-ASSET-DEPR-001"],"errors":[safe]}),
    )
}
enum Command {
    Report,
    Register(Register),
    Depreciate(i64, Depreciate),
    WriteOff(WriteOff),
    Invalid,
    InvalidRegister,
}
impl Command {
    fn action(&self) -> Action<'_> {
        match self {
            Self::Report => Action::Report,
            Self::Register(v) => Action::Register(v),
            Self::Depreciate(id, v) => Action::Depreciate(*id, v),
            Self::WriteOff(v) => Action::WriteOff(v),
            Self::Invalid => Action::Invalid,
            Self::InvalidRegister => Action::InvalidRegister,
        }
    }
}
#[derive(Clone, Copy)]
enum Operation {
    Register,
    Depreciate,
    WriteOff,
}
fn valid_fields(fields: &Value, operation: Operation) -> bool {
    let Some(m) = fields.as_object() else {
        return false;
    };
    for (k, v) in m {
        let allowed = match operation {
            Operation::Register => matches!(
                k.as_str(),
                "name"
                    | "category"
                    | "acquisitionDate"
                    | "cost"
                    | "usefulLifeMonths"
                    | "documentId"
                    | "assetAccount"
                    | "depreciationAccount"
                    | "accumulatedAccount"
                    | "note"
            ),
            Operation::Depreciate => matches!(k.as_str(), "period" | "date"),
            Operation::WriteOff => matches!(
                k.as_str(),
                "name"
                    | "category"
                    | "acquisitionDate"
                    | "cost"
                    | "documentId"
                    | "expenseAccount"
                    | "date"
                    | "thresholdRuleSource"
                    | "confirmImmediateWriteOff"
                    | "paymentAccount"
                    | "note"
            ),
        };
        if !allowed || v.is_null() {
            return false;
        }
        let valid = match k.as_str() {
            "documentId" => v
                .as_i64()
                .is_some_and(|n| (1..=9_007_199_254_740_991).contains(&n)),
            "period" | "usefulLifeMonths" => v.as_u64().is_some_and(|n| (1..=12000).contains(&n)),
            "cost" => v.as_f64().is_some_and(|n| {
                n.is_finite() && n > 0.0 && n <= (9_007_199_254_740_991_i64 as f64 / 100.0)
            }),
            "confirmImmediateWriteOff" => v.is_boolean(),
            "date" | "acquisitionDate" => v
                .as_str()
                .is_some_and(|s| rentemester_engine::fiscal_year::Date::parse(s).is_ok()),
            _ => v.as_str().is_some_and(|s| {
                s.len() <= 4096
                    && (k == "note" || !s.trim().is_empty())
                    && !s
                        .chars()
                        .any(|c| c.is_control() && !(k == "note" && matches!(c, '\n' | '\t')))
            }),
        };
        if !valid {
            return false;
        }
    }
    true
}
fn parse(bytes: &[u8], operation: Operation, id: Option<i64>) -> (Command, Option<String>) {
    let Ok(wire) = serde_json::from_slice::<Write>(bytes) else {
        return (Command::Invalid, None);
    };
    if !wire.confirm || !valid_fields(&wire.fields, operation) {
        return (Command::Invalid, None);
    }
    let command = match operation {
        Operation::Register => serde_json::from_value(wire.fields)
            .map(Command::Register)
            .unwrap_or(Command::Invalid),
        Operation::WriteOff => serde_json::from_value(wire.fields)
            .map(Command::WriteOff)
            .unwrap_or(Command::Invalid),
        Operation::Depreciate => match id {
            Some(id) => serde_json::from_value(wire.fields)
                .map(|v| Command::Depreciate(id, v))
                .unwrap_or(Command::Invalid),
            None => Command::Invalid,
        },
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
    let operation = if path.ends_with("/register") {
        Operation::Register
    } else if path.ends_with("/write-off") {
        Operation::WriteOff
    } else {
        Operation::Depreciate
    };
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
        Ok(bytes) if headers_valid && write => parse(&bytes, operation, id),
        Ok(bytes) if headers_valid && !write && bytes.is_empty() => (Command::Report, None),
        _ => (Command::Invalid, None),
    };
    let command = if matches!(operation, Operation::Register) && matches!(command, Command::Invalid)
    {
        Command::InvalidRegister
    } else {
        command
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
                    .original_asset(&cookie, &SystemClock, command.action(), expected.as_deref())
                    .map_err(|_| Box::new(domain_error("", true)))?,
            ),
            documents::Credential::Service(key) => service_checked(
                owner
                    .original_asset_service(
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
    let reply = match result {
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
    };
    if matches!(operation, Operation::WriteOff) && reply.status() != StatusCode::OK {
        let (parts, body) = reply.into_parts();
        if let Ok(bytes) = to_bytes(body, 16384).await
            && let Ok(mut value) = serde_json::from_slice::<Value>(&bytes)
        {
            // Only the generic error fallback has the default depreciation
            // rule; preserve all actual domain/journal refusal evidence.
            if value["appliedRules"] == json!(["DK-ASSET-DEPR-001"]) {
                value["appliedRules"] = json!(["DK-ASSET-WRITEOFF-001"]);
            }
            return response(parts.status, value);
        }
        return domain_error("", true);
    }
    reply
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_asset_source_wire() {
        let register = json!({"confirm":true,"name":"Synthetic","category":"Equipment","acquisitionDate":"2026-01-01","cost":125,"usefulLifeMonths":3,"documentId":1});
        assert!(matches!(
            parse(
                &serde_json::to_vec(&register).unwrap(),
                Operation::Register,
                None
            )
            .0,
            Command::Register(_)
        ));
        for (key, value) in [
            ("documentId", Value::Null),
            ("documentId", json!(9_007_199_254_740_992_i64)),
            ("actor", json!("user:forged")),
            ("expectedActor", Value::Null),
            ("note", Value::Null),
            ("assetAccount", Value::Null),
            ("method", json!("linear")),
            ("confirm", json!(false)),
            ("cost", json!("125")),
            ("acquisitionDate", json!("2026-02-30")),
            ("usefulLifeMonths", json!(12001)),
            ("idempotencyKey", json!("key")),
        ] {
            let mut v = register.clone();
            v[key] = value;
            assert!(
                matches!(
                    parse(&serde_json::to_vec(&v).unwrap(), Operation::Register, None).0,
                    Command::Invalid
                ),
                "{key}"
            );
        }
        assert!(matches!(
            parse(
                br#"{"confirm":true,"confirm":true,"period":1,"date":"2026-01-01"}"#,
                Operation::Depreciate,
                Some(1)
            )
            .0,
            Command::Invalid
        ));
        assert!(matches!(
            parse(
                br#"{"confirm":true,"period":1,"date":"2026-01-01"}"#,
                Operation::Depreciate,
                Some(1)
            )
            .0,
            Command::Depreciate(1, _)
        ));
        for bytes in [
            br#"{"confirm":true,"period":1}"#.as_slice(),
            br#"{"confirm":true,"date":"2026-01-01"}"#.as_slice(),
        ] {
            assert!(matches!(
                parse(bytes, Operation::Depreciate, Some(1)).0,
                Command::Invalid
            ));
        }
        let mut writeoff = json!({"confirm":true,"name":"Synthetic","category":"Equipment","acquisitionDate":"2026-01-01","date":"2026-01-01","cost":250,"documentId":1,"expenseAccount":"2000","thresholdRuleSource":"Explicit advisor source","confirmImmediateWriteOff":false});
        assert!(matches!(
            parse(
                &serde_json::to_vec(&writeoff).unwrap(),
                Operation::WriteOff,
                None
            )
            .0,
            Command::WriteOff(_)
        ));
        for v in [Value::Null, json!("false"), json!(0)] {
            writeoff["confirmImmediateWriteOff"] = v;
            assert!(matches!(
                parse(
                    &serde_json::to_vec(&writeoff).unwrap(),
                    Operation::WriteOff,
                    None
                )
                .0,
                Command::Invalid
            ));
        }
    }
    #[tokio::test]
    async fn curated_refusals_and_auth_status_win_without_leaking_internal_errors() {
        assert_eq!(
            domain_error("access_denied", true).status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            domain_error("invalid_credentials", true).status(),
            StatusCode::UNAUTHORIZED
        );
        for code in [
            "asset_refused",
            "invoice_refused",
            "journal_refused",
            "document_evidence_unavailable",
            "period_closed",
        ] {
            assert_eq!(
                domain_error(code, false).status(),
                StatusCode::UNPROCESSABLE_ENTITY
            );
        }
        let r = domain_error("private SQL path", false);
        assert_eq!(r.status(), StatusCode::SERVICE_UNAVAILABLE);
        let b = to_bytes(r.into_body(), 4096).await.unwrap();
        assert!(!String::from_utf8_lossy(&b).contains("private SQL"));
    }
}
