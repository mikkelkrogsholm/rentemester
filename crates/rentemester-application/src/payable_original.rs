//! Original payable contracts inside the existing expense authority/source transaction.
use rentemester_engine::{Error, fiscal_year::Date};
use rentemester_runtime::{auth::Clock, namespace::NamespaceGuard};
use rentemester_storage::{
    idempotency::PrincipalKind,
    payable_original::{self as storage, Action},
};
use rusqlite::Transaction;
use serde_json::Value;
pub(crate) fn mutation(action: Action<'_>) -> bool {
    !matches!(action, Action::List(_))
}
pub(crate) fn authorized(
    tx: &Transaction<'_>,
    guard: &mut NamespaceGuard,
    action: Action<'_>,
    subject: &str,
    kind: PrincipalKind,
    clock: &dyn Clock,
) -> Result<Value, Error> {
    authorized_program(tx, guard, action, subject, kind, clock, false)
}
pub(crate) fn authorized_agent(
    tx: &Transaction<'_>,
    guard: &mut NamespaceGuard,
    action: Action<'_>,
    subject: &str,
    kind: PrincipalKind,
    clock: &dyn Clock,
) -> Result<Value, Error> {
    authorized_program(tx, guard, action, subject, kind, clock, true)
}
fn authorized_program(
    tx: &Transaction<'_>,
    guard: &mut NamespaceGuard,
    action: Action<'_>,
    subject: &str,
    kind: PrincipalKind,
    clock: &dyn Clock,
    agent: bool,
) -> Result<Value, Error> {
    let now = clock
        .now_unix_ms()
        .map_err(|e| Error::new(e.code(), "trusted payable clock unavailable"))?;
    if let Action::List(query) = action {
        crate::bank::verify_query_sources(tx, guard)?;
        let utc = rentemester_runtime::backup_keys::timestamp(now)
            .map_err(|e| Error::new(e, "trusted UTC date unavailable"))?;
        return storage::list(tx, query, &utc[..10]);
    }
    let document = match action {
        Action::Register(i) => {
            i.validate()?;
            i.document_id
        }
        Action::Pay(i) => {
            i.validate()?;
            rentemester_storage::payables::document_id(tx, i.payable_id)?
        }
        _ => unreachable!(),
    };
    let original = rentemester_storage::documents::evidence(tx, document)?;
    guard
        .read_document(&original.path, &original.hash)
        .map_err(|_| {
            Error::new(
                "document_evidence_unavailable",
                "Payable original unavailable or integrity mismatch",
            )
        })?;
    rentemester_storage::bank_original::verify_journals(tx)?;
    if let Action::Pay(i) = action {
        let hash =
            rentemester_storage::bank_reconciliation::source_hash(tx, i.bank_transaction_id)?;
        let bytes = guard.read_bank_statement(&hash).map_err(|_| {
            Error::new(
                "bank_statement_original_unavailable",
                "Payable bank original unavailable or integrity mismatch",
            )
        })?;
        rentemester_storage::bank_reconciliation::verify_source(tx, i.bank_transaction_id, &bytes)?;
    }
    let latest = rentemester_runtime::posting_time::latest_date(now, 45)
        .map_err(|e| Error::new(e.code(), "trusted payable posting horizon unavailable"))?;
    (if agent {
        storage::mutate_agent
    } else {
        storage::mutate
    })(tx, action, subject, kind, Date::parse(&latest)?, now)
}
#[cfg(test)]
mod tests;
