//! Seven source draft operations over the native append-only writer.
//! Authority and duplicate JSON keys are checked by the admitted application.
use crate::{accounting_drafts as native, invalid};
use rentemester_engine::{Error, fiscal_year::Date};
use rusqlite::Transaction;
use serde_json::{Value, json};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Operation {
    Create,
    Revise,
    Submit,
    Reject,
    ApproveAndPost,
    Show,
    List,
}
fn usage(message: &str) -> Error {
    Error::new("invalid_input", message)
}
fn stable_id(value: &Value) -> Result<&str, Error> {
    let id = value
        .as_str()
        .ok_or_else(|| usage("draftId required"))?
        .trim();
    if id.is_empty()
        || id.len() > 64
        || !id.as_bytes()[0].is_ascii_lowercase()
        || !id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(usage("lowercase stable draft id required"));
    }
    Ok(id)
}
fn source_state(mut state: Value) -> Value {
    if let Some(object) = state.as_object_mut() {
        if object
            .get("reason")
            .and_then(Value::as_str)
            .is_none_or(|s| s.is_empty())
        {
            object.remove("reason");
        }
        if object
            .get("journalEntryId")
            .and_then(Value::as_i64)
            .is_none_or(|id| id <= 0)
        {
            object.remove("journalEntryId");
        }
    }
    state
}
fn validate_input(op: Operation, input: &Value) -> Result<(), Error> {
    let object = input
        .as_object()
        .ok_or_else(|| usage("input object required"))?;
    let fields: &[&str] = match op {
        Operation::List => &[],
        Operation::Show => &["draftId"],
        Operation::Create => &["draftId", "payload", "confirm"],
        Operation::Revise => &["draftId", "expectedEventHash", "payload", "confirm"],
        Operation::Reject => &["draftId", "expectedEventHash", "reason", "confirm"],
        Operation::Submit | Operation::ApproveAndPost => {
            &["draftId", "expectedEventHash", "confirm"]
        }
    };
    if object.len() != fields.len() || object.keys().any(|k| !fields.contains(&k.as_str())) {
        return Err(usage("exact operation fields required"));
    }
    if op != Operation::List {
        stable_id(&input["draftId"])?;
    }
    if fields.contains(&"confirm") && input["confirm"] != true {
        return Err(usage("explicit confirm true required"));
    }
    if fields.contains(&"expectedEventHash") {
        let hash = input["expectedEventHash"].as_str().unwrap_or("");
        if hash.len() != 64
            || !hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(usage("exact lowercase SHA256 expectedEventHash required"));
        }
    }
    if op == Operation::Reject {
        let reason = input["reason"].as_str().unwrap_or("").trim();
        if reason.is_empty() || reason.len() > 1000 {
            return Err(usage("rejection reason must contain 1 through 1000 bytes"));
        }
    }
    if fields.contains(&"payload") {
        let payload = input["payload"]
            .as_object()
            .ok_or_else(|| usage("payload object required"))?;
        if payload.contains_key("createdBy") || payload.contains_key("createdByProgram") {
            return Err(usage("caller attribution forbidden"));
        }
        if serde_json::to_vec(&input["payload"])
            .map_err(invalid)?
            .len()
            > 262144
        {
            return Err(usage("payload exceeds 262144 bytes"));
        }
        // Wire numerics remain source numerics; the native normalizer and engine
        // retain sole authority over their accounting meaning.
        for key in [
            "documentId",
            "sourceBankTransactionId",
            "amountForeign",
            "amountDkk",
            "fxRateToDkk",
        ] {
            if let Some(value) = payload.get(key).filter(|v| !v.is_null())
                && !value.is_number()
            {
                return Err(usage("source numeric payload required"));
            }
        }
        if let Some(lines) = payload.get("lines").and_then(Value::as_array) {
            for line in lines {
                for key in ["debitAmount", "creditAmount"] {
                    if let Some(value) = line.get(key)
                        && !value.is_number()
                    {
                        return Err(usage("source numeric line amounts required"));
                    }
                }
            }
        }
    }
    if serde_json::to_vec(input).map_err(invalid)?.len() > 270 * 1024 {
        return Err(usage("input exceeds 270 KiB"));
    }
    Ok(())
}
/// Execute inside the caller's currently authorized transaction. A business
/// refusal rolls back its own effects even when the caller commits afterwards.
pub fn execute(
    tx: &Transaction<'_>,
    op: Operation,
    input: &Value,
    actor: &str,
    latest: Date,
) -> Result<Value, Error> {
    validate_input(op, input)?;
    if matches!(op, Operation::List | Operation::Show) {
        let states = native::list(tx)?;
        let states = states
            .as_array()
            .ok_or_else(|| invalid("draft list unavailable"))?;
        return if op == Operation::List {
            Ok(
                json!({"ok":true,"accountingDrafts":states.iter().cloned().map(source_state).collect::<Vec<_>>()}),
            )
        } else {
            let id = stable_id(&input["draftId"])?;
            Ok(
                json!({"ok":true,"accountingDraft":states.iter().find(|state| state["id"] == id).cloned().map(source_state)}),
            )
        };
    }
    let action = match op {
        Operation::Create => "create",
        Operation::Revise => "revise",
        Operation::Submit => "submit",
        Operation::Reject => "reject",
        Operation::ApproveAndPost => "approve-and-post",
        Operation::Show | Operation::List => unreachable!(),
    };
    let id = stable_id(&input["draftId"])?;
    let parsed: native::Input = serde_json::from_value(input.clone()).map_err(invalid)?;
    // Require the native writer fence before opening a savepoint: opening a
    // savepoint must not make a deferred transaction appear to be a writer.
    crate::journal_post::require_current(tx)?;
    tx.execute_batch("SAVEPOINT original_accounting_draft")
        .map_err(invalid)?;
    let result: Result<Value, Error> = (|| {
        let mut receipt = None;
        let mut state = source_state(native::change_with_receipt(
            tx,
            id,
            action,
            &parsed,
            actor,
            latest,
            &mut receipt,
        )?);
        if op == Operation::ApproveAndPost {
            let receipt = match receipt {
                Some(actual) => actual,
                None => {
                    let id = state["journalEntryId"]
                        .as_i64()
                        .filter(|id| *id > 0)
                        .ok_or_else(|| native::refused("posted journal evidence missing"))?;
                    tx.query_row("SELECT id,entry_no,entry_hash FROM journal_entries WHERE id=?1", [id], |r| {
                        Ok(json!({"ok":true,"entryId":r.get::<_, i64>(0)?,"entryNo":r.get::<_, String>(1)?,"entryHash":r.get::<_, String>(2)?,"appliedRules":[],"errors":[]}))
                    }).map_err(invalid)?
                }
            };
            state["journal"] = receipt;
        }
        Ok(json!({"ok":true,"accountingDraft":state}))
    })();
    match result {
        Ok(value) => {
            tx.execute_batch("RELEASE original_accounting_draft")
                .map_err(invalid)?;
            Ok(value)
        }
        Err(error) => {
            tx.execute_batch(
                "ROLLBACK TO original_accounting_draft; RELEASE original_accounting_draft",
            )
            .map_err(invalid)?;
            if error.code == "draft_refused" {
                Ok(json!({"ok":false,"errors":[error.message]}))
            } else {
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests;
