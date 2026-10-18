//! Asset use cases verify immutable purchase originals under current user authority.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
};
use rentemester_engine::{Error, fiscal_year::Date};
use rentemester_runtime::namespace::NamespaceGuard;
use rusqlite::Connection;
use serde_json::Value;
pub enum Action<'a> {
    List,
    Next(i64),
    Register(&'a rentemester_storage::assets::Register),
    Depreciate(i64, &'a rentemester_storage::assets::Depreciate),
}
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    action: Action<'_>,
    today: &str,
) -> Result<Committed<Value>, Failure> {
    let permission = match action {
        Action::List | Action::Next(_) => Permission::CompanyRead,
        _ => Permission::CompanyLedgerPost,
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
        |tx, actor| {
            let id = match action {
                Action::List => {
                    return rentemester_storage::assets::list(tx, slug);
                }
                Action::Next(id) => return rentemester_storage::assets::next(tx, id),
                Action::Register(i) => i.purchase_document_id,
                Action::Depreciate(id, _) => rentemester_storage::assets::document_id(tx, id)?,
            };
            let original = rentemester_storage::documents::evidence(tx, id)?;
            guard
                .read_document(&original.path, &original.hash)
                .map_err(|_| {
                    Error::new(
                        "document_evidence_unavailable",
                        "asset purchase original unavailable or integrity mismatch",
                    )
                })?;
            let now = context
                .clock
                .now_unix_ms()
                .map_err(|e| Error::new(e.code(), "asset clock unavailable"))?;
            let latest = rentemester_runtime::posting_time::latest_date(now, 45)
                .map_err(|e| Error::new(e.code(), "asset posting horizon unavailable"))?;
            match action {
                Action::Register(i) => {
                    rentemester_storage::assets::register(tx, i, actor, Date::parse(today)?)
                }
                Action::Depreciate(id, i) => rentemester_storage::assets::depreciate(
                    tx,
                    id,
                    i,
                    actor,
                    today,
                    Date::parse(&latest)?,
                ),
                _ => unreachable!(),
            }
        },
    )
}
