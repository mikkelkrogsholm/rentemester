//! Annual exports share the annual preparation current-authority and source transaction.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
};
use rusqlite::Connection;
use serde_json::Value;
pub fn read(
    control: &mut Connection,
    company: &mut Connection,
    context: Context<'_>,
    start: &str,
    end: &str,
    kind: &str,
) -> Result<Committed<Value>, Failure> {
    let slug = context.selection.slug;
    control_fence::execute_cookie(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Current,
        },
        context.selection,
        Permission::CompanyRead,
        |tx| rentemester_storage::annual_exports::read(tx, slug, start, end, kind),
    )
}
