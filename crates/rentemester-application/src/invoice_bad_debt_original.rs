//! Original bad-debt admission through the shared held sales owner.
use rentemester_engine::{Error, fiscal_year::Date};
use rentemester_runtime::namespace::NamespaceGuard;
use rentemester_storage::invoice_bad_debt_original as storage;
use rusqlite::Transaction;
use serde_json::Value;
pub(crate) fn apply(
    tx: &Transaction<'_>,
    guard: &mut NamespaceGuard,
    body: &Value,
    context: crate::invoice_lifecycle::Context<'_>,
) -> Result<Value, Error> {
    storage::validate(body)?;
    let id = storage::invoice_id(tx, body)?;
    crate::sales::verify_invoice(tx, guard, id)?;
    if !crate::periods::verify_sources(tx, guard)?.is_empty() {
        return Err(Error::new(
            "invoice_evidence_unavailable",
            "period source integrity mismatch",
        ));
    }
    let mut banks = rentemester_storage::reminder_claims::required_banks(tx, id)?;
    banks.sort_unstable();
    banks.dedup();
    for bank in banks {
        let hash = rentemester_storage::bank_reconciliation::source_hash(tx, bank)?;
        let bytes = guard.read_bank_statement(&hash).map_err(|_| {
            Error::new(
                "bank_statement_original_unavailable",
                "exact archived claim bank source required",
            )
        })?;
        rentemester_storage::bank_reconciliation::verify_source(tx, bank, &bytes)?;
    }
    let ms = context
        .clock
        .now_unix_ms()
        .map_err(|_| Error::new("clock_unavailable", "trusted claim clock unavailable"))?;
    let today = rentemester_runtime::posting_time::latest_date(ms, 0)
        .map_err(|_| Error::new("clock_unavailable", "trusted claim date unavailable"))?;
    storage::apply(
        tx,
        body,
        id,
        context.subject,
        context.principal,
        Date::parse(&today)?,
    )
}
