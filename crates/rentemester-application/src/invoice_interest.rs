//! Separate claim assessment, registration and posting under current human authority.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
};
use rentemester_engine::{Error, fiscal_year::Date};
use rentemester_runtime::namespace::NamespaceGuard;
use rentemester_storage::invoice_interest as storage;
use rusqlite::{Connection, Transaction};
use serde_json::Value;

pub enum Action<'a> {
    Calculate(&'a storage::Calculate),
    Register(&'a storage::Register),
    Post(&'a storage::Post),
    Correction(&'a storage::Correction),
    PostCorrection(&'a storage::PostCorrection),
}
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    action: Action<'_>,
) -> Result<Committed<Value>, Failure> {
    execute_with_sources(control, company, context, action, |tx, id| {
        crate::sales::verify_invoice(tx, guard, id)?;
        for bank in rentemester_storage::reminder_claims::required_banks(tx, id)? {
            let hash = rentemester_storage::bank_reconciliation::source_hash(tx, bank)?;
            let bytes = guard.read_bank_statement(&hash).map_err(|_| {
                Error::new(
                    "bank_statement_original_unavailable",
                    "interest requires its exact existing payment source",
                )
            })?;
            rentemester_storage::bank_reconciliation::verify_source(tx, bank, &bytes)?;
        }
        Ok(())
    })
}
fn execute_with_sources(
    control: &mut Connection,
    company: &mut Connection,
    context: Context<'_>,
    action: Action<'_>,
    mut verify: impl FnMut(&Transaction<'_>, i64) -> Result<(), Error>,
) -> Result<Committed<Value>, Failure> {
    let (permission, readonly, id) = match &action {
        Action::Calculate(input) => (Permission::CompanyRead, true, input.invoice_document_id),
        Action::Correction(input) => (Permission::CompanyRead, true, input.invoice_document_id),
        Action::Register(input) => (
            Permission::CompanyDraftWrite,
            false,
            input.invoice_document_id,
        ),
        Action::Post(input) => (
            Permission::CompanyLedgerPost,
            false,
            input.invoice_document_id,
        ),
        Action::PostCorrection(input) => (
            Permission::CompanyLedgerPost,
            false,
            input.invoice_document_id,
        ),
    };
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
        permission,
        readonly,
        false,
        |_, tx, actor| {
            verify(tx, id)?;
            match action {
                Action::Calculate(input) => storage::calculate(tx, input),
                Action::Correction(input) => storage::correction(tx, input),
                Action::Register(input) => {
                    let today = today(clock)?;
                    if input.as_of_date.as_str() > today.as_str() {
                        return Err(Error::new(
                            "interest_refused",
                            "claim registration date cannot exceed the current Danish date",
                        ));
                    }
                    storage::register(tx, input, actor)
                }
                Action::Post(input) => {
                    storage::post(tx, input, actor, Date::parse(&today(clock)?)?)
                }
                Action::PostCorrection(input) => {
                    storage::post_correction(tx, input, actor, Date::parse(&today(clock)?)?)
                }
            }
        },
    )
}
fn today(clock: &dyn rentemester_runtime::auth::Clock) -> Result<String, Error> {
    clock
        .now_unix_ms()
        .and_then(|now| rentemester_runtime::posting_time::latest_date(now, 0))
        .map_err(|_| Error::new("clock_unavailable", "trusted interest date unavailable"))
}

#[cfg(test)]
mod tests;
