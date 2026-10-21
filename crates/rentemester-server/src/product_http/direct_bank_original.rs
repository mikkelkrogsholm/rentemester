//! Original direct-payable intent over the same source-bound composite writer.
use super::*;
use rentemester_application::direct_bank_payable_correction::Action;
use rentemester_storage::{
    bank_original::ExpectedPrincipal,
    direct_bank_payable_correction::{ApplyInput, PlanInput},
};
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Input {
    document_id: i64,
    bank_transaction_id: i64,
    bill_date: String,
    due_date: String,
    expense_account_no: String,
    #[serde(default, deserialize_with = "text")]
    vat_treatment: Option<String>,
    #[serde(default, deserialize_with = "positive")]
    vendor_id: Option<i64>,
    #[serde(default, deserialize_with = "text")]
    note: Option<String>,
    #[serde(default, deserialize_with = "text")]
    plan_hash: Option<String>,
    #[serde(default, deserialize_with = "text")]
    reason: Option<String>,
    #[serde(default, deserialize_with = "text")]
    idempotency_key: Option<String>,
    #[serde(default, deserialize_with = "boolean")]
    confirm: Option<bool>,
    #[serde(default, deserialize_with = "principal")]
    expected_principal: Option<ExpectedPrincipal>,
}
fn text<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    String::deserialize(d).map(Some)
}
fn positive<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
    i64::deserialize(d).map(Some)
}
fn boolean<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<bool>, D::Error> {
    bool::deserialize(d).map(Some)
}
fn principal<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<ExpectedPrincipal>, D::Error> {
    ExpectedPrincipal::deserialize(d).map(Some)
}
enum Command {
    Plan(PlanInput),
    Apply(ApplyInput, Option<ExpectedPrincipal>),
}
impl Command {
    fn action(&self) -> Action<'_> {
        match self {
            Self::Plan(i) => Action::OriginalPlan(i),
            Self::Apply(input, expected) => Action::OriginalApply {
                input,
                expected: expected.as_ref(),
            },
        }
    }
}
fn parse(bytes: &[u8], apply: bool) -> Result<Command, &'static str> {
    let i: Input = serde_json::from_slice(bytes).map_err(|_| "invalid_input")?;
    if !(1..=9_007_199_254_740_991).contains(&i.document_id)
        || !(1..=9_007_199_254_740_991).contains(&i.bank_transaction_id)
        || i.vendor_id
            .is_some_and(|v| !(1..=9_007_199_254_740_991).contains(&v))
        || [&i.bill_date, &i.due_date, &i.expense_account_no]
            .iter()
            .any(|s| s.is_empty() || s.len() > 256)
        || i.note.as_ref().is_some_and(|s| s.len() > 4096)
        || i.vat_treatment
            .as_deref()
            .is_some_and(|s| !matches!(s, "standard" | "exempt" | "non_deductible"))
    {
        return Err("invalid_input");
    }
    let source = PlanInput {
        document_id: i.document_id,
        bank_transaction_id: i.bank_transaction_id,
        bill_date: i.bill_date,
        due_date: i.due_date,
        expense_account_no: i.expense_account_no,
        vat_treatment: i.vat_treatment,
        vendor_id: i.vendor_id,
        note: i.note,
    };
    if !apply {
        if i.plan_hash.is_some()
            || i.reason.is_some()
            || i.idempotency_key.is_some()
            || i.confirm.is_some()
            || i.expected_principal.is_some()
        {
            return Err("invalid_input");
        }
        return Ok(Command::Plan(source));
    }
    if i.confirm != Some(true) {
        return Err("confirmation_required");
    }
    if let Some(expected) = &i.expected_principal {
        expected.validate().map_err(|_| "invalid_input")?;
    }
    let hash = i.plan_hash.ok_or("invalid_input")?;
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("invalid_input");
    }
    let reason = i.reason.ok_or("invalid_input")?;
    if reason.trim().is_empty() || reason.trim().chars().count() > 1000 {
        return Err("invalid_input");
    }
    let key = i.idempotency_key.ok_or("invalid_idempotency_key")?;
    rentemester_storage::idempotency::validate_key(&key).map_err(|_| "invalid_idempotency_key")?;
    Ok(Command::Apply(
        ApplyInput {
            document_id: source.document_id,
            bank_transaction_id: source.bank_transaction_id,
            bill_date: source.bill_date,
            due_date: source.due_date,
            expense_account_no: source.expense_account_no,
            vat_treatment: source.vat_treatment,
            vendor_id: source.vendor_id,
            note: source.note,
            plan_hash: hash,
            reason,
            idempotency_key: Some(key),
            confirm: true,
        },
        i.expected_principal,
    ))
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let apply = request.uri().path().ends_with("/direct-payable-apply");
    let credential = match documents::credential(&state, &request, apply) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    if request.method() != axum::http::Method::POST
        || request.uri().query().is_some()
        || request.headers().contains_key("content-encoding")
        || request.headers().contains_key("idempotency-key")
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
    let bytes = match to_bytes(request.into_body(), 16 * 1024).await {
        Ok(b) => b,
        Err(_) => return refused(StatusCode::PAYLOAD_TOO_LARGE, "body_too_large"),
    };
    let command = match parse(&bytes, apply) {
        Ok(i) => i,
        Err(code) => return refused(StatusCode::BAD_REQUEST, code),
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        Ok::<_, ()>(match credential {
            documents::Credential::Human(cookie) => documents::outcome(
                owner
                    .direct_bank_payable_correction(&cookie, &SystemClock, command.action())
                    .map_err(|_| ())?,
            ),
            documents::Credential::Service(key) => documents::service_outcome(
                owner
                    .direct_bank_payable_correction_service(&key, &SystemClock, command.action())
                    .map_err(|_| ())?,
            ),
        })
    })
    .await;
    match result {
        Ok(Ok(Ok(v))) => response(StatusCode::OK, v),
        Ok(Ok(Err(r))) => *r,
        _ => refused(
            StatusCode::SERVICE_UNAVAILABLE,
            if apply {
                "outcome_unknown"
            } else {
                "unavailable"
            },
        ),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_direct_wire_is_flat_strict_and_keeps_raw_reason_key() {
        let plan = json!({"documentId":1,"bankTransactionId":2,"billDate":"2026-06-16","dueDate":"2026-07-16","expenseAccountNo":"3000"});
        assert!(parse(&serde_json::to_vec(&plan).unwrap(), false).is_ok());
        for key in ["vatTreatment", "vendorId", "note", "expectedPrincipal"] {
            let mut bad = plan.clone();
            bad[key] = Value::Null;
            assert!(parse(&serde_json::to_vec(&bad).unwrap(), false).is_err());
        }
        let mut apply = plan.clone();
        apply["confirm"] = json!(true);
        apply["planHash"] = json!("a".repeat(64));
        apply["reason"] = json!(" raw reason ");
        apply["idempotencyKey"] = json!("synthetic-exact-key");
        apply["expectedPrincipal"] =
            json!({"kind":"service-account","subjectId":"service_synthetic"});
        let Command::Apply(i, expected) =
            parse(&serde_json::to_vec(&apply).unwrap(), true).unwrap()
        else {
            panic!()
        };
        assert_eq!(i.reason, " raw reason ");
        assert_eq!(i.idempotency_key.as_deref(), Some("synthetic-exact-key"));
        assert_eq!(expected.unwrap().subject_id, "service_synthetic");
        for bad in [
            json!(null),
            json!("user:u"),
            json!({"kind":"user","subjectId":"u","actor":"forged"}),
        ] {
            let mut value = apply.clone();
            value["expectedPrincipal"] = bad;
            assert!(parse(&serde_json::to_vec(&value).unwrap(), true).is_err());
        }
        let duplicate = String::from_utf8(serde_json::to_vec(&plan).unwrap())
            .unwrap()
            .replacen("{", "{\"documentId\":9,", 1);
        assert!(parse(duplicate.as_bytes(), false).is_err());
        apply["confirm"] = json!(false);
        assert_eq!(
            parse(&serde_json::to_vec(&apply).unwrap(), true).err(),
            Some("confirmation_required")
        );
    }
}
