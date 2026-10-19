//! Read-only reviewed plan and one explicitly confirmed atomic correction.
use super::*;
use rentemester_application::direct_bank_payable_correction::Action;
use rentemester_storage::direct_bank_payable_correction::{ApplyInput, PlanInput};
enum Input {
    Plan(Box<PlanInput>),
    Apply(Box<ApplyInput>),
}
fn parse_apply(bytes: &[u8], header: Option<&str>) -> Result<ApplyInput, ()> {
    let mut input: ApplyInput = serde_json::from_slice(bytes).map_err(|_| ())?;
    if !input.confirm {
        return Err(());
    }
    match (header, input.idempotency_key.as_deref()) {
        (Some(h), Some(b)) if h != b => return Err(()),
        (Some(h), _) => input.idempotency_key = Some(h.to_owned()),
        (None, None) => return Err(()),
        _ => {}
    }
    Ok(input)
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let apply = request.uri().path().ends_with("/apply");
    let cookie = match proof(&state, &request, apply) {
        Ok(c) => c,
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
    let slug = request
        .uri()
        .path()
        .split('/')
        .nth(3)
        .unwrap_or("")
        .to_owned();
    let header = if request.headers().contains_key("idempotency-key") {
        let Some(h) = one(request.headers(), "idempotency-key") else {
            return refused(StatusCode::BAD_REQUEST, "invalid_idempotency_key");
        };
        Some(h.to_owned())
    } else {
        None
    };
    if !apply && header.is_some() {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let Ok(bytes) = to_bytes(request.into_body(), 16 * 1024).await else {
        return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
    };
    let input = if apply {
        match parse_apply(&bytes, header.as_deref()) {
            Ok(i) => Input::Apply(Box::new(i)),
            Err(()) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
        }
    } else {
        match serde_json::from_slice::<PlanInput>(&bytes) {
            Ok(i) => Input::Plan(Box::new(i)),
            Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
        }
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        let action = match &input {
            Input::Plan(i) => Action::Plan(i),
            Input::Apply(i) => Action::Apply(i),
        };
        owner
            .direct_bank_payable_correction(&cookie, &SystemClock, action)
            .map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(attempt)) => {
            if attempt.outcome.as_ref().is_err_and(|f| {
                f.stage == FailureStage::NotCommitted && f.error.code == "access_denied"
            }) {
                return refused(StatusCode::FORBIDDEN, "access_denied");
            }
            if let Err(f) = &attempt.outcome
                && f.stage == FailureStage::NotCommitted
                && matches!(
                    f.error.code,
                    "IDEMPOTENCY_CONFLICT" | "IDEMPOTENCY_OUTCOME_EXPIRED"
                )
            {
                return response(
                    StatusCode::CONFLICT,
                    json!({"ok":false,"code":f.error.code,"errors":[f.error.message]}),
                );
            }
            match super::documents::outcome(attempt) {
                Ok(v) => response(StatusCode::OK, v),
                Err(r) => *r,
            }
        }
        _ => response(
            StatusCode::SERVICE_UNAVAILABLE,
            json!({"ok":false,"code":if apply{"outcome_unknown"}else{"unavailable"},"errors":["Read canonical payables, bank and journal before another action"]}),
        ),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keys_are_exact_and_json_duplicates_cannot_be_hidden_by_header() {
        let common = r#""documentId":1,"bankTransactionId":2,"billDate":"2026-01-10","dueDate":"2026-02-10","expenseAccountNo":"3000","planHash":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","reason":"synthetic correction","confirm":true"#;
        let body = format!("{{{common}}}");
        assert!(parse_apply(body.as_bytes(), None).is_err());
        assert_eq!(
            parse_apply(body.as_bytes(), Some("original key"))
                .unwrap()
                .idempotency_key
                .as_deref(),
            Some("original key")
        );
        let body = format!("{{{common},\"idempotencyKey\":\"body key\"}}");
        assert!(parse_apply(body.as_bytes(), Some("different key")).is_err());
        assert!(parse_apply(body.as_bytes(), Some("body key")).is_ok());
        let duplicate = format!("{{{common},\"documentId\":3}}");
        assert!(parse_apply(duplicate.as_bytes(), Some("original key")).is_err());
        let forged = format!("{{{common},\"actor\":\"user:forged\"}}");
        assert!(parse_apply(forged.as_bytes(), Some("original key")).is_err());
    }
}
