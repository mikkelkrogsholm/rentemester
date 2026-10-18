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
    Payables { as_of: &'a str, status: &'a str },
    Register(&'a rentemester_storage::payables::Register),
    Pay(i64, &'a rentemester_storage::payables::Payment),
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
        Action::Payables { .. } => Permission::CompanyRead,
        Action::Post(_) | Action::Register(_) | Action::Pay(_, _) => Permission::CompanyLedgerPost,
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
            Action::Payables { as_of, status } => {
                rentemester_storage::payables::list(tx, slug, as_of, status)
            }
            Action::Register(_) | Action::Pay(_, _) => {
                let id = match action {
                    Action::Register(input) => input.document_id,
                    Action::Pay(id, _) => rentemester_storage::payables::document_id(tx, id)?,
                    _ => unreachable!(),
                };
                let original = rentemester_storage::documents::evidence(tx, id)?;
                guard
                    .read_document(&original.path, &original.hash)
                    .map_err(|_| {
                        Error::new(
                            "document_evidence_unavailable",
                            "payable original unavailable or integrity mismatch",
                        )
                    })?;
                let now = clock
                    .now_unix_ms()
                    .map_err(|e| Error::new(e.code(), "posting clock unavailable"))?;
                let latest = rentemester_runtime::posting_time::latest_date(now, 45)
                    .map_err(|e| Error::new(e.code(), "posting horizon unavailable"))?;
                let latest = Date::parse(&latest)?;
                match action {
                    Action::Register(input) => {
                        rentemester_storage::payables::register(tx, input, subject, latest)
                    }
                    Action::Pay(id, input) => {
                        rentemester_storage::payables::pay(tx, id, input, subject, latest)
                    }
                    _ => unreachable!(),
                }
            }
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
