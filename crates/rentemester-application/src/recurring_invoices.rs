//! Explicit manual recurring generation shares ordinary invoice issuance and current authority.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
};
use rentemester_runtime::namespace::NamespaceGuard;
use rentemester_storage::recurring_invoices::{self as recurring, Prepared};
use rusqlite::Connection;
use serde_json::Value;
pub enum Action<'a> {
    List,
    Create(&'a recurring::Create),
    Generate(i64, &'a recurring::Generate),
    Retire(i64, &'a recurring::Retire),
}
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    action: Action<'_>,
    today: &str,
    now: &str,
) -> Result<Committed<Value>, Failure> {
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
        if matches!(action, Action::List) {
            Permission::CompanyRead
        } else {
            Permission::CompanyDraftWrite
        },
        |tx, actor| match action {
            Action::List => recurring::list(tx, slug),
            Action::Create(i) => recurring::create(tx, i, actor, now),
            Action::Retire(id, i) => recurring::retire(tx, id, i, actor),
            Action::Generate(id, i) => match recurring::prepare(tx, id, i, today, now)? {
                Prepared::Existing(value) => {
                    crate::sales::verify_invoice(tx, guard, value["documentId"].as_i64().unwrap())?;
                    Ok(value)
                }
                Prepared::Issue(pending) => {
                    let issued = crate::sales::issue_payload(tx, guard, &pending.payload, actor)?;
                    recurring::finish(tx, &pending, &issued, actor)
                }
            },
        },
    )
}
