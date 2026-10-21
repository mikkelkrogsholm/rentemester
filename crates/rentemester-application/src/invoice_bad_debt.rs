//! Explicit source-backed principal writeoff under current human authority.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
};
use rentemester_engine::{Error, fiscal_year::Date};
use rentemester_runtime::namespace::NamespaceGuard;
use rentemester_storage::invoice_bad_debt::{self as storage, WriteOff};
use rusqlite::{Connection, Transaction};
use serde_json::Value;

pub fn write_off(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    input: &WriteOff,
) -> Result<Committed<Value>, Failure> {
    write_off_with_sources(control, company, context, input, |tx, id| {
        crate::sales::verify_invoice(tx, guard, id)?;
        for bank in rentemester_storage::reminder_claims::required_banks(tx, id)? {
            let hash = rentemester_storage::bank_reconciliation::source_hash(tx, bank)?;
            let bytes = guard.read_bank_statement(&hash).map_err(|_| {
                Error::new(
                    "bank_statement_original_unavailable",
                    "bad-debt writeoff requires its exact existing payment source",
                )
            })?;
            rentemester_storage::bank_reconciliation::verify_source(tx, bank, &bytes)?;
        }
        Ok(())
    })
}
fn write_off_with_sources(
    control: &mut Connection,
    company: &mut Connection,
    context: Context<'_>,
    input: &WriteOff,
    mut verify: impl FnMut(&Transaction<'_>, i64) -> Result<(), Error>,
) -> Result<Committed<Value>, Failure> {
    let clock = context.clock;
    control_fence::execute_cookie_both(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock,
            requirement: SessionRequirement::Current,
        },
        context.selection,
        Permission::CompanyLedgerPost,
        false,
        false,
        |_, tx, actor| {
            verify(tx, input.invoice_document_id)?;
            let today = clock
                .now_unix_ms()
                .and_then(|now| rentemester_runtime::posting_time::latest_date(now, 0))
                .map_err(|_| {
                    Error::new("clock_unavailable", "trusted writeoff date unavailable")
                })?;
            storage::write_off(tx, input, actor, Date::parse(&today)?)
        },
    )
}

#[cfg(test)]
mod tests;
