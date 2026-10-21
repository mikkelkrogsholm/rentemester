//! Read-only current principal/source admission shared by original invoice clients.
use rentemester_engine::Error;
use rentemester_runtime::{auth::Clock, namespace::NamespaceGuard};
use rentemester_storage::invoice_original::{self as storage, Kind, Query};
use rusqlite::Transaction;
use serde_json::Value;
pub(crate) fn authorized(
    tx: &Transaction<'_>,
    guard: &mut NamespaceGuard,
    kind: Kind,
    query: &Query,
    clock: &dyn Clock,
) -> Result<Value, Error> {
    let now = clock
        .now_unix_ms()
        .map_err(|e| Error::new(e.code(), "trusted invoice clock unavailable"))?;
    let today = rentemester_runtime::posting_time::latest_date(now, 0)
        .map_err(|e| Error::new(e.code(), "trusted Danish invoice date unavailable"))?;
    if !crate::periods::verify_sources(tx, guard)?.is_empty() {
        return Err(Error::new(
            "invoice_original_source_invalid",
            "Current invoice journal/source evidence differs",
        ));
    }
    let mut q = tx
        .prepare("SELECT id FROM documents WHERE document_type='issued_invoice' ORDER BY id")
        .map_err(|_| {
            Error::new(
                "invoice_original_source_invalid",
                "Invoice register unavailable",
            )
        })?;
    let ids = q
        .query_map([], |r| r.get::<_, i64>(0))
        .map_err(|_| {
            Error::new(
                "invoice_original_source_invalid",
                "Invoice register unavailable",
            )
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| {
            Error::new(
                "invoice_original_source_invalid",
                "Invoice register unavailable",
            )
        })?;
    let mut banks = std::collections::BTreeSet::new();
    for id in ids {
        crate::sales::verify_invoice(tx, guard, id)?;
        let mut q=tx.prepare("SELECT bank_transaction_id FROM invoice_payments WHERE invoice_document_id=?1 AND bank_transaction_id IS NOT NULL UNION SELECT bank_transaction_id FROM invoice_refunds WHERE invoice_document_id=?1 AND bank_transaction_id IS NOT NULL UNION SELECT bank_transaction_id FROM invoice_claim_payments WHERE invoice_document_id=?1 AND bank_transaction_id IS NOT NULL").map_err(|_|Error::new("invoice_original_source_invalid","Invoice bank links unavailable"))?;
        let linked = q
            .query_map([id], |r| r.get::<_, i64>(0))
            .map_err(|_| {
                Error::new(
                    "invoice_original_source_invalid",
                    "Invoice bank links unavailable",
                )
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| {
                Error::new(
                    "invoice_original_source_invalid",
                    "Invoice bank links unavailable",
                )
            })?;
        banks.extend(linked);
    }
    for id in banks {
        let hash = rentemester_storage::bank_reconciliation::source_hash(tx, id)?;
        let bytes = guard.read_bank_statement(&hash).map_err(|_| {
            Error::new(
                "bank_statement_original_unavailable",
                "Invoice bank original unavailable or changed",
            )
        })?;
        rentemester_storage::bank_reconciliation::verify_source(tx, id, &bytes)?;
    }
    storage::read(tx, kind, query, &today)
}
#[cfg(test)]
mod tests;
