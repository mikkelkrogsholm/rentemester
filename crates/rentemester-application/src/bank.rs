//! Bank browser use cases under the current-session company fence.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
};
use rentemester_engine::Error;
use rentemester_runtime::namespace::NamespaceGuard;
pub use rentemester_storage::bank::AccountInput;
use rusqlite::Connection;
use serde_json::Value;
pub enum Action<'a> {
    Accounts,
    Read {
        today: &'a str,
        year: Option<&'a str>,
    },
    Create(&'a AccountInput),
    Import {
        content: &'a str,
        account: Option<&'a str>,
        now: &'a str,
    },
}
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    action: Action<'_>,
) -> Result<Committed<Value>, Failure> {
    let permission = match action {
        Action::Accounts | Action::Read { .. } => Permission::CompanyRead,
        Action::Create(_) => Permission::CompanyAdmin,
        Action::Import { .. } => Permission::CompanyLedgerPost,
    };
    let slug = context.selection.slug;
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
        |tx, subject| match action {
            Action::Accounts => rentemester_storage::bank::accounts(tx, slug),
            Action::Read { today, year } => rentemester_storage::bank::read(tx, slug, today, year),
            Action::Create(input) => rentemester_storage::bank::create_account(tx, input, subject),
            Action::Import {
                content,
                account,
                now,
            } => {
                let rows = rentemester_engine::bank::parse(content)?;
                let archive = guard.publish_bank_statement(content).map_err(|_| {
                    Error::new(
                        "bank_original_unavailable",
                        "bank statement original could not be preserved",
                    )
                })?;
                rentemester_storage::bank::import(
                    tx, content, &rows, account, subject, now, &archive,
                )
            }
        },
    )
}
