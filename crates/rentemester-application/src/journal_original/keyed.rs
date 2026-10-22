//! Original wire intent wrapper over the existing receipt engine and unkeyed writer.
use super::*;
use rentemester_storage::idempotency::{self, Input, Operation, Outcome};

fn invalid() -> Error {
    Error::new("invalid_input", "invalid original keyed journal request")
}
fn positive_id(value: &Value) -> bool {
    value
        .as_f64()
        .is_some_and(|n| n > 0.0 && n.fract() == 0.0 && n <= 9_007_199_254_740_991.0)
}
fn optional(object: &Value, name: &str, valid: impl FnOnce(&Value) -> bool) -> Result<(), Error> {
    if object.get(name).is_some_and(|v| !valid(v)) {
        return Err(invalid());
    }
    Ok(())
}
/// Validate syntax only, never resolve selectors or consult current business state.
/// Keep every original field (including ignored lower-priority selectors) in intent.
pub(super) fn validate_keyed_business(operation: &str, business: &Value) -> Result<(), Error> {
    if operation == "reverse" {
        for field in ["entryId", "matchDocumentId"] {
            optional(business, field, positive_id)?;
        }
        for field in ["entryNo", "matchText", "matchDate"] {
            optional(business, field, Value::is_string)?;
        }
        return Ok(());
    }
    let input = &business["input"];
    if !input
        .get("text")
        .and_then(Value::as_str)
        .is_some_and(|s| !s.is_empty())
    {
        return Err(invalid());
    }
    for field in ["documentId", "sourceBankTransactionId"] {
        optional(input, field, positive_id)?;
    }
    for field in ["currency"] {
        optional(input, field, Value::is_string)?;
    }
    for field in ["amountForeign", "amountDkk", "fxRateToDkk"] {
        optional(input, field, |v| v.as_f64().is_some_and(f64::is_finite))?;
    }
    let lines = input
        .get("lines")
        .and_then(Value::as_array)
        .filter(|v| v.len() >= 2)
        .ok_or_else(invalid)?;
    for line in lines {
        if !line
            .get("accountNo")
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty())
        {
            return Err(invalid());
        }
        for field in ["debitAmount", "creditAmount"] {
            optional(line, field, |v| {
                v.as_f64().is_some_and(|n| n.is_finite() && n >= 0.0)
            })?;
        }
        for field in ["vatCode", "text"] {
            optional(line, field, Value::is_string)?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn keyed(
    tx: &Transaction<'_>,
    subject: &str,
    kind: PrincipalKind,
    clock: &dyn Clock,
    max_future_days: u32,
    action: Action,
    key: &str,
    intent: &Value,
    business: &mut Option<Value>,
) -> Result<Value, Error> {
    // Preserve the native historical admission gates even on exact replay.
    rentemester_storage::journal_post::require_current(tx)?;
    rentemester_storage::opening_balance::validate_history(tx)?;
    let now = clock
        .now_unix_ms()
        .map_err(|_| Error::new("invalid_auth_clock", "journal clock unavailable"))?;
    let operation = match &action {
        Action::Post(_) => Operation::JournalPost,
        Action::Reverse(_) => Operation::JournalReverse,
        _ => return Err(invalid()),
    };
    let actor = format!("user:{subject}");
    let program = match kind {
        PrincipalKind::User => "rentemester-rust",
        PrincipalKind::ServiceAccount => "service-principal",
    };
    match idempotency::execute(
        tx,
        Input {
            key,
            operation,
            principal_kind: kind,
            subject,
            payload: intent,
            created_by: &actor,
            created_by_program: program,
            now_ms: now,
        },
        |tx| {
            // The same unkeyed mutation performs selector resolution and all current
            // business validation only for a new intent, inside this admitted writer.
            super::apply(tx, subject, kind, clock, max_future_days, action, business)
        },
    ) {
        Ok(Outcome::Accepted {
            mut result,
            receipt,
        }) => {
            // Do not silently discard native restored-receipt provenance. This
            // format cannot prove compatibility with historical source hashes.
            if receipt.reconciliation.is_some() {
                return Err(Error::new(
                    "RESTORED_KEY_RECONCILIATION_REQUIRED",
                    "original wire receipt requires canonical reconciliation",
                ));
            }
            result["idempotency"] = json!({
                "replayed": receipt.replayed, "receiptId": receipt.receipt_id,
                "createdAt": receipt.created_at, "expiresAt": receipt.expires_at,
            });
            // This value remains inside the current outer fence until commit.
            Ok(result)
        }
        Ok(Outcome::Refused { code }) => {
            Ok(json!({"ok":false,"code":code,"errors":[code],"appliedRules":[]}))
        }
        Err(failure) => {
            if failure.rejection.is_some() {
                *business = failure.rejection;
            }
            Err(failure.error)
        }
    }
}
