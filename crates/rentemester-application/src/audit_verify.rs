//! Read-only full accounting observation under the current authorized subject.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
    service_company::{self, ServiceFailure, ServiceResult},
};
use rentemester_engine::Error;
use rentemester_runtime::namespace::NamespaceGuard;
use rusqlite::{Connection, Transaction};
use serde_json::{Value, json};
fn original_evidence(tx: &Transaction<'_>, guard: &mut NamespaceGuard) -> Result<Value, Error> {
    guard.ensure_current().map_err(|_| {
        Error::new(
            "audit_namespace_unavailable",
            "owned physical namespace is unavailable",
        )
    })?;
    let errors = crate::periods::audit_evidence(tx, guard)?;
    let documents: i64 = tx
        .query_row("SELECT count(*) FROM documents", [], |row| row.get(0))
        .map_err(|_| {
            Error::new(
                "audit_originals_unavailable",
                "document register unavailable",
            )
        })?;
    let bank_transactions: i64 = tx
        .query_row("SELECT count(*) FROM bank_transactions", [], |row| {
            row.get(0)
        })
        .map_err(|_| {
            Error::new(
                "audit_originals_unavailable",
                "bank source register unavailable",
            )
        })?;
    guard.ensure_current().map_err(|_| {
        Error::new(
            "audit_namespace_unavailable",
            "owned physical namespace changed during verification",
        )
    })?;
    Ok(
        json!({"available":true,"ok":errors.is_empty(),"checked":{"namespace":1,"documents":documents,"bankTransactions":bank_transactions},"errors":errors}),
    )
}
pub fn read(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
) -> Result<Committed<Value>, Failure> {
    let slug = context.selection.slug;
    control_fence::execute_cookie_both(
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
        false,
        |_, tx, _| {
            rentemester_storage::audit_verify::report(tx, slug, original_evidence(tx, guard))
        },
    )
}
pub fn read_service(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: service_company::Context<'_>,
) -> Result<ServiceResult, ServiceFailure> {
    let slug = context.selection.slug;
    service_company::execute_read(control, company, context, |tx| {
        rentemester_storage::audit_verify::report(tx, slug, original_evidence(tx, guard))
    })
}
#[cfg(test)]
mod tests;
