//! Bounded read-only review and one explicitly confirmed opening-balance transaction.
use super::*;
use rentemester_application::opening_balance::Action;
use rentemester_engine::opening_balance::{Apply, Input as Plan};
enum Input {
    Read,
    Plan(Box<Plan>),
    Apply(Box<Apply>),
    SyntheticPlan(Box<rentemester_storage::synthetic_import::Input>),
    SyntheticApply(Box<rentemester_storage::synthetic_import::Apply>),
}
fn parse_plan(bytes: &[u8]) -> Result<Plan, ()> {
    serde_json::from_slice(bytes).map_err(|_| ())
}
fn parse_apply(bytes: &[u8], header: Option<&str>) -> Result<Apply, ()> {
    let input: Apply = serde_json::from_slice(bytes).map_err(|_| ())?;
    if !input.confirm || header.is_some_and(|h| h != input.idempotency_key) {
        return Err(());
    }
    Ok(input)
}
fn parse_plan_input(bytes: &[u8]) -> Result<Input, ()> {
    parse_plan(bytes)
        .map(|v| Input::Plan(Box::new(v)))
        .or_else(|()| {
            serde_json::from_slice(bytes)
                .map(|v| Input::SyntheticPlan(Box::new(v)))
                .map_err(|_| ())
        })
}
fn parse_apply_input(bytes: &[u8], header: Option<&str>) -> Result<Input, ()> {
    parse_apply(bytes, header)
        .map(|v| Input::Apply(Box::new(v)))
        .or_else(|()| {
            let v: rentemester_storage::synthetic_import::Apply =
                serde_json::from_slice(bytes).map_err(|_| ())?;
            if !v.confirm || header.is_some_and(|h| h != v.idempotency_key) {
                return Err(());
            }
            Ok(Input::SyntheticApply(Box::new(v)))
        })
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let apply = request.uri().path().ends_with("/apply");
    let plan = request.uri().path().ends_with("/plan");
    let cookie = match proof(&state, &request, apply) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    if request.uri().query().is_some() || request.headers().contains_key("content-encoding") {
        return refused(StatusCode::BAD_REQUEST, "opening_balance_invalid");
    }
    let header = if request.headers().contains_key("idempotency-key") {
        let Some(value) = one(request.headers(), "idempotency-key") else {
            return refused(StatusCode::BAD_REQUEST, "invalid_idempotency_key");
        };
        Some(value.to_owned())
    } else {
        None
    };
    if !apply && header.is_some() {
        return refused(StatusCode::BAD_REQUEST, "opening_balance_invalid");
    }
    let slug = request
        .uri()
        .path()
        .split('/')
        .nth(3)
        .unwrap_or("")
        .to_owned();
    let method = request.method().clone();
    let input = if apply || plan {
        if method != axum::http::Method::POST
            || !one(request.headers(), "content-type")
                .is_some_and(|v| v.eq_ignore_ascii_case("application/json"))
        {
            return refused(StatusCode::BAD_REQUEST, "opening_balance_invalid");
        }
        let Ok(bytes) = to_bytes(request.into_body(), 64 * 1024).await else {
            return refused(StatusCode::PAYLOAD_TOO_LARGE, "opening_balance_invalid");
        };
        match if apply {
            parse_apply_input(&bytes, header.as_deref())
        } else {
            parse_plan_input(&bytes)
        } {
            Ok(input) => input,
            Err(()) => return refused(StatusCode::BAD_REQUEST, "opening_balance_invalid"),
        }
    } else {
        if method != axum::http::Method::GET || request.headers().contains_key("transfer-encoding")
        {
            return refused(StatusCode::BAD_REQUEST, "opening_balance_invalid");
        }
        let Ok(bytes) = to_bytes(request.into_body(), 1).await else {
            return refused(StatusCode::BAD_REQUEST, "opening_balance_invalid");
        };
        if !bytes.is_empty() {
            return refused(StatusCode::BAD_REQUEST, "opening_balance_invalid");
        }
        Input::Read
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        let action = match &input {
            Input::Read => Action::Read,
            Input::Plan(i) => Action::Plan(i),
            Input::Apply(i) => Action::Apply(i),
            Input::SyntheticPlan(i) => Action::SyntheticPlan(i),
            Input::SyntheticApply(i) => Action::SyntheticApply(i),
        };
        owner
            .opening_balance(&cookie, &SystemClock, action)
            .map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(attempt)) => match attempt.outcome {
            Ok(result) => response(StatusCode::OK, result.value),
            Err(f) => {
                let unknown = f.stage != FailureStage::NotCommitted;
                let code = f.error.code;
                let status = if unknown
                    || code.contains("unavailable")
                    || !f.control_reusable
                    || !f.company_reusable
                {
                    StatusCode::SERVICE_UNAVAILABLE
                } else if matches!(code, "access_denied" | "invalid_credentials") {
                    StatusCode::FORBIDDEN
                } else if matches!(
                    code,
                    "IDEMPOTENCY_CONFLICT"
                        | "IDEMPOTENCY_OUTCOME_EXPIRED"
                        | "RESTORED_KEY_RECONCILIATION_REQUIRED"
                ) {
                    StatusCode::CONFLICT
                } else if matches!(
                    code,
                    "opening_balance_invalid" | "invalid_idempotency_key" | "invalid_input"
                ) {
                    StatusCode::BAD_REQUEST
                } else {
                    StatusCode::UNPROCESSABLE_ENTITY
                };
                response(
                    status,
                    json!({"ok":false,"code":if unknown && apply {"outcome_unknown"}else{code},"errors":[f.error.message]}),
                )
            }
        },
        _ => refused(
            StatusCode::SERVICE_UNAVAILABLE,
            if apply {
                "outcome_unknown"
            } else {
                "opening_balance_source_unavailable"
            },
        ),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_envelopes_confirmation_and_keys_cannot_hide_duplicate_or_actor_fields() {
        let input = r#"{"cutOverDate":"2024-07-01","lines":[{"accountNo":"A","debitAmount":"12.34"},{"accountNo":"E","creditAmount":"12.34"}]}"#;
        assert!(parse_plan(input.as_bytes()).is_ok());
        assert!(
            parse_plan(
                input
                    .replace("\"lines\":", "\"actor\":\"user:caller\",\"lines\":")
                    .as_bytes()
            )
            .is_err()
        );
        assert!(
            parse_plan(
                input
                    .replace("\"lines\":", "\"cutOverDate\":\"2024-01-01\",\"lines\":")
                    .as_bytes()
            )
            .is_err()
        );
        let body = format!(
            r#"{{"input":{input},"planHash":"{}","idempotencyKey":"exact key","confirm":true}}"#,
            "a".repeat(64)
        );
        assert!(parse_apply(body.as_bytes(), None).is_ok());
        assert!(parse_apply(body.as_bytes(), Some("exact key")).is_ok());
        assert!(parse_apply(body.as_bytes(), Some("changed key")).is_err());
        assert!(parse_apply(body.replace("true", "false").as_bytes(), None).is_err());
        assert!(
            parse_apply(
                body.replace("\"confirm\":true", "\"confirm\":true,\"confirm\":true")
                    .as_bytes(),
                None
            )
            .is_err()
        );
    }
}

#[cfg(test)]
mod synthetic_tests {
    use super::*;
    #[test]
    fn source_envelopes_cannot_override_actor_or_confirmation_or_header_key() {
        let raw = r##"{"sourceSystem":"synthetic-csv","content":"# cutOverDate: 2025-07-01\r\n"}"##;
        assert!(matches!(
            parse_plan_input(raw.as_bytes()),
            Ok(Input::SyntheticPlan(_))
        ));
        assert!(
            parse_plan_input(
                raw.replace("\"content\":", "\"actor\":\"user:caller\",\"content\":")
                    .as_bytes()
            )
            .is_err()
        );
        let body = format!(
            r#"{{"input":{raw},"planHash":"{}","idempotencyKey":"source-key","confirm":true}}"#,
            "a".repeat(64)
        );
        assert!(matches!(
            parse_apply_input(body.as_bytes(), Some("source-key")),
            Ok(Input::SyntheticApply(_))
        ));
        assert!(parse_apply_input(body.as_bytes(), Some("changed-key")).is_err());
        assert!(parse_apply_input(body.replace("true", "false").as_bytes(), None).is_err());
    }
}
