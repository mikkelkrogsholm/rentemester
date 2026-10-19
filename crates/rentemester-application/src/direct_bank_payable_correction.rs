//! Current human authorization plus physical source proof across ONE composite company transaction.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
};
use rentemester_engine::{Error, fiscal_year::Date};
use rentemester_runtime::namespace::NamespaceGuard;
use rentemester_storage::direct_bank_payable_correction::{self as storage, ApplyInput, PlanInput};
use rusqlite::Connection;
use serde_json::{Value, json};
pub enum Action<'a> {
    Plan(&'a PlanInput),
    Apply(&'a ApplyInput),
}
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    action: Action<'_>,
) -> Result<Committed<Value>, Failure> {
    let permission = match action {
        Action::Plan(_) => Permission::CompanyRead,
        Action::Apply(_) => Permission::CompanyLedgerPost,
    };
    let clock = context.clock;
    control_fence::execute_cookie_subject(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock,
            requirement: SessionRequirement::Current,
        },
        context.selection,
        permission,
        |tx, subject| {
            let input = match action {
                Action::Plan(i) => i.clone(),
                Action::Apply(i) => i.source(),
            };
            input.validate()?;
            let original = rentemester_storage::documents::evidence(tx, input.document_id)?;
            guard
                .read_document(&original.path, &original.hash)
                .map_err(|_| {
                    Error::new(
                        "document_evidence_unavailable",
                        "Correction purchase original missing or changed",
                    )
                })?;
            let hash = rentemester_storage::bank_reconciliation::source_hash(
                tx,
                input.bank_transaction_id,
            )?;
            let bytes = guard.read_bank_statement(&hash).map_err(|_| {
                Error::new(
                    "bank_statement_original_unavailable",
                    "Correction bank archive missing or changed",
                )
            })?;
            rentemester_storage::bank_reconciliation::verify_source(
                tx,
                input.bank_transaction_id,
                &bytes,
            )?;
            let now = clock
                .now_unix_ms()
                .map_err(|e| Error::new(e.code(), "trusted correction clock unavailable"))?;
            let latest = Date::parse(
                &rentemester_runtime::posting_time::latest_date(now, 45)
                    .map_err(|e| Error::new(e.code(), "trusted posting horizon unavailable"))?,
            )?;
            match action {
                Action::Plan(i) => Ok(json!({"ok":true,"plan":storage::plan(tx,i,latest)?})),
                Action::Apply(i) => storage::apply(tx, i, subject, latest, now),
            }
        },
    )
}
