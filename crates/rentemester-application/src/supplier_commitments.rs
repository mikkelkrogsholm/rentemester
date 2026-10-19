//! Current read/draft authority on the same held company writer and original namespace.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
};
use rentemester_engine::Error;
use rentemester_runtime::namespace::NamespaceGuard;
use rentemester_storage::supplier_commitments as storage;
use rusqlite::{Connection, Transaction};
use serde_json::Value;
pub enum Action<'a> {
    List(&'a str),
    Matches,
    Plan(&'a storage::Plan),
    Apply(&'a storage::Apply),
    Change(&'a storage::Change),
    Match(&'a storage::Match),
}
fn original(tx: &Transaction<'_>, guard: &mut NamespaceGuard, id: i64) -> Result<(), Error> {
    let source = rentemester_storage::documents::evidence(tx, id)?;
    guard
        .read_document(&source.path, &source.hash)
        .map_err(|_| {
            Error::new(
                "commitment_original_unavailable",
                "original missing or integrity mismatch",
            )
        })?;
    Ok(())
}
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    action: Action<'_>,
    now: &str,
) -> Result<Committed<Value>, Failure> {
    let slug = context.selection.slug;
    let permission = if matches!(action, Action::List(_) | Action::Matches | Action::Plan(_)) {
        Permission::CompanyRead
    } else {
        Permission::CompanyDraftWrite
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
        |tx, actor| match action {
            Action::List(start) => storage::list(tx, slug, start),
            Action::Matches => Ok(serde_json::json!({"rows":storage::matches(tx)?})),
            Action::Plan(input) => {
                for id in storage::source_documents(&input.commitment)? {
                    original(tx, guard, id)?;
                }
                storage::plan(tx, &input.commitment)
            }
            Action::Apply(input) => {
                for id in storage::source_documents(&input.commitment)? {
                    original(tx, guard, id)?;
                }
                storage::apply(tx, input, actor, now)
            }
            Action::Change(input) => storage::change(tx, input, actor, now),
            Action::Match(input) => {
                if let Some(id) = storage::evidence_document(tx, input)? {
                    original(tx, guard, id)?;
                }
                storage::match_occurrence(tx, input, actor, now)
            }
        },
    )
}
