//! Original bank intent on the same held namespace, current authority and source proofs.
use crate::access::Permission;
use rentemester_engine::Error;
use rentemester_runtime::{auth::Clock, namespace::NamespaceGuard};
use rentemester_storage::{
    bank_original::{self as store, Action},
    bank_reconciliation as native,
    idempotency::PrincipalKind,
};
use rusqlite::Transaction;
use serde_json::{Value, json};
pub(crate) fn mutation(action: Action<'_>) -> bool {
    matches!(action, Action::Link(_) | Action::Correct(_))
}
pub(crate) fn permission(action: Action<'_>) -> Permission {
    if mutation(action) {
        Permission::CompanyLedgerPost
    } else {
        Permission::CompanyRead
    }
}
fn bank_source(tx: &Transaction<'_>, guard: &mut NamespaceGuard, id: i64) -> Result<(), Error> {
    let hash = native::source_hash(tx, id)?;
    let bytes=guard.read_bank_statement(&hash).map_err(|_|Error::new("bank_statement_original_unavailable","Archived bank statement missing or changed; restore original source before reconciliation"))?;
    native::verify_source(tx, id, &bytes)
}
fn journals(
    tx: &Transaction<'_>,
    guard: &mut NamespaceGuard,
    ids: impl IntoIterator<Item = i64>,
) -> Result<(), Error> {
    // Full hash/line integrity precedes natural or keyed replay, including the
    // counter-lines that the bank movement alone cannot establish.
    store::verify_journals(tx)?;
    for id in ids {
        crate::bank_reconciliation::verify_journal_original(tx, guard, id)?;
    }
    Ok(())
}
pub(crate) fn read_authorized(
    tx: &Transaction<'_>,
    guard: &mut NamespaceGuard,
    action: Action<'_>,
    subject: &str,
    kind: PrincipalKind,
    clock: &dyn Clock,
) -> Result<Value, Error> {
    let result = (|| match action {
        Action::Suggestions(input) => {
            crate::bank::verify_query_sources(tx, guard)?;
            Ok(store::envelope(
                rentemester_storage::bank_matches::suggest(tx, input)?,
                false,
            ))
        }
        Action::Link(input) => {
            input.validate()?;
            bank_source(tx, guard, input.bank_transaction_id)?;
            journals(tx, guard, [input.journal_entry_id])?;
            store::link(tx, input, subject, kind)
        }
        Action::Plan { bank, replacement } => {
            bank_source(tx, guard, bank)?;
            let plan = native::correction::plan(tx, bank, replacement)?;
            journals(
                tx,
                guard,
                [
                    plan["plan"]["currentJournalEntryId"].as_i64().unwrap(),
                    plan["plan"]["reversalJournalEntryId"].as_i64().unwrap(),
                    replacement,
                ],
            )?;
            Ok(store::envelope(plan, false))
        }
        Action::Correct(input) => {
            if let Some(expected) = &input.expected_principal {
                expected.assert_current(kind, subject)?;
            }
            bank_source(tx, guard, input.input.bank_transaction_id)?;
            journals(
                tx,
                guard,
                native::correction::source_journals(tx, &input.input)?,
            )?;
            let now = clock.now_unix_ms().map_err(|_| {
                Error::new("clock_unavailable", "trusted receipt clock unavailable")
            })?;
            store::correct(tx, input, subject, kind, now)
        }
    })();
    match result {
        Err(error)
            if matches!(
                error.code,
                "bank_original_validation" | "bank_original_link_conflict"
            ) =>
        {
            Ok(store::envelope(
                json!({"ok":false,"errors":[error.message]}),
                matches!(action, Action::Link(_)),
            ))
        }
        value => value,
    }
}
#[cfg(test)]
mod tests;
