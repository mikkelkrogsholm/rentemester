//! Verify original evidence and post the expense atomically under current roles.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
};
use rentemester_engine::{Error, fiscal_year::Date};
use rentemester_runtime::namespace::NamespaceGuard;
pub use rentemester_storage::expense::Input;
use rusqlite::Connection;
use serde_json::Value;
pub enum Action<'a> {
    Options(i64),
    Post(&'a Input),
}
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    action: Action<'_>,
) -> Result<Committed<Value>, Failure> {
    let permission = match action {
        Action::Options(_) => Permission::CompanyDocumentsRead,
        Action::Post(_) => Permission::CompanyLedgerPost,
    };
    let slug = context.selection.slug;
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
        |tx, subject| match action {
            Action::Options(id) => rentemester_storage::expense::options(tx, slug, id),
            Action::Post(input) => {
                let original = rentemester_storage::documents::evidence(tx, input.document_id)?;
                guard
                    .read_document(&original.path, &original.hash)
                    .map_err(|_| {
                        Error::new(
                            "document_evidence_unavailable",
                            "expense original unavailable or integrity mismatch",
                        )
                    })?;
                let now = clock
                    .now_unix_ms()
                    .map_err(|e| Error::new(e.code(), "posting clock unavailable"))?;
                let latest = rentemester_runtime::posting_time::latest_date(now, 45)
                    .map_err(|e| Error::new(e.code(), "posting horizon unavailable"))?;
                rentemester_storage::expense::post(tx, input, subject, Date::parse(&latest)?)
            }
        },
    )
}
