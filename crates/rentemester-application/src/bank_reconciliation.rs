//! Current read/draft authority on the same held company writer and original namespace.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
};
use rentemester_engine::Error;
use rentemester_runtime::namespace::NamespaceGuard;
use rentemester_storage::bank_reconciliation as storage;
use rusqlite::Connection;
use serde_json::Value;
pub enum Action<'a> {
    Candidates(i64),
    Apply(&'a storage::Apply),
    CorrectionPlan { bank: i64, replacement: i64 },
    Correct(&'a storage::correction::Apply),
}
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    action: Action<'_>,
) -> Result<Committed<Value>, Failure> {
    let permission = if matches!(
        action,
        Action::Candidates(_) | Action::CorrectionPlan { .. }
    ) {
        Permission::CompanyRead
    } else {
        Permission::CompanyLedgerPost
    };
    let clock = context.clock;
    control_fence::execute_cookie_subject(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Current,
        },
        context.selection,
        permission,
        |tx, actor| {
            let bank = match action {
                Action::Candidates(id) => id,
                Action::Apply(i) => i.bank_transaction_id,
                Action::CorrectionPlan { bank, .. } => bank,
                Action::Correct(i) => i.bank_transaction_id,
            };
            let hash = storage::source_hash(tx, bank)?;
            let bytes=guard.read_bank_statement(&hash).map_err(|_|Error::new("bank_statement_original_unavailable","Archived bank statement missing or changed; restore original source before reconciliation"))?;
            storage::verify_source(tx, bank, &bytes)?;
            match action {
                Action::Candidates(id) => storage::candidates(tx, id),
                Action::CorrectionPlan { bank, replacement } => {
                    let planned = storage::correction::plan(tx, bank, replacement)?;
                    for journal in [
                        planned["plan"]["currentJournalEntryId"].as_i64().unwrap(),
                        planned["plan"]["reversalJournalEntryId"].as_i64().unwrap(),
                        replacement,
                    ] {
                        verify_journal_original(tx, guard, journal)?;
                    }
                    Ok(planned)
                }
                Action::Correct(input) => {
                    for journal in storage::correction::source_journals(tx, input)? {
                        verify_journal_original(tx, guard, journal)?;
                    }
                    storage::correction::apply(
                        tx,
                        input,
                        actor,
                        clock.now_unix_ms().map_err(|_| {
                            Error::new("clock_unavailable", "trusted receipt clock unavailable")
                        })?,
                    )
                }
                Action::Apply(input) => {
                    if let Some(id) = storage::document_id(tx, input.journal_entry_id)? {
                        let original = rentemester_storage::documents::evidence(tx, id)?;
                        guard
                            .read_document(&original.path, &original.hash)
                            .map_err(|_| {
                                Error::new(
                                    "bank_journal_original_unavailable",
                                    "Journal original missing or changed",
                                )
                            })?;
                    }
                    storage::apply(tx, input, actor)
                }
            }
        },
    )
}

fn verify_journal_original(
    tx: &rusqlite::Transaction<'_>,
    guard: &mut NamespaceGuard,
    journal: i64,
) -> Result<(), Error> {
    if let Some(id) = storage::document_id(tx, journal)? {
        let source = rentemester_storage::documents::evidence(tx, id)?;
        guard
            .read_document(&source.path, &source.hash)
            .map_err(|_| {
                Error::new(
                    "bank_journal_original_unavailable",
                    "Journal original missing or changed",
                )
            })?;
    }
    Ok(())
}
