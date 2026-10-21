//! Company audit inspection through current human or scoped-service authorization.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
    service_company::{self, ServiceFailure, ServiceResult},
};
use rentemester_storage::audit_log::Query;
use rusqlite::Connection;
use serde_json::Value;

pub fn read(
    control: &mut Connection,
    company: &mut Connection,
    context: Context<'_>,
    query: &Query,
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
        |_, tx, _| rentemester_storage::audit_log::read(tx, slug, query),
    )
}

pub fn read_service(
    control: &mut Connection,
    company: &mut Connection,
    context: service_company::Context<'_>,
    query: &Query,
) -> Result<ServiceResult<Value>, ServiceFailure> {
    let slug = context.selection.slug;
    service_company::execute_read(control, company, context, |tx| {
        rentemester_storage::audit_log::read(tx, slug, query)
    })
}

#[cfg(test)]
mod tests;
