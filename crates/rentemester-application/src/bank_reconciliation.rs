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
}
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    action: Action<'_>,
) -> Result<Committed<Value>, Failure> {
    let permission = if matches!(action, Action::Candidates(_)) {
        Permission::CompanyRead
    } else {
        Permission::CompanyLedgerPost
    };
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
            };
            let hash = storage::source_hash(tx, bank)?;
            let bytes=guard.read_bank_statement(&hash).map_err(|_|Error::new("bank_statement_original_unavailable","Archived bank statement missing or changed; restore original source before reconciliation"))?;
            storage::verify_source(tx, bank, &bytes)?;
            match action {
                Action::Candidates(id) => storage::candidates(tx, id),
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
