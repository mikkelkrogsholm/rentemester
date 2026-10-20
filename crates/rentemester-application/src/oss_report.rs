//! Read-only OSS base preparation under current company authority, never submission.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
};
use rentemester_engine::{Error, fiscal_year::Date};
use rentemester_runtime::namespace::NamespaceGuard;
use rusqlite::Connection;
use serde_json::Value;

pub fn report(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    from: Date,
    to: Date,
) -> Result<Committed<Value>, Failure> {
    control_fence::execute_cookie_subject_mode(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Current,
        },
        context.selection,
        Permission::CompanyRead,
        true,
        |tx, _| {
            if !crate::periods::verify_sources(tx, guard)?.is_empty() {
                return Err(Error::new(
                    "oss_report_source_unavailable",
                    "OSS report source assurance differs",
                ));
            }
            crate::archive_import::verify_physical(tx, guard)?;
            rentemester_storage::oss_report::read(tx, from, to)
        },
    )
}
