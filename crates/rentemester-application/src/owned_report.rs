//! Current-authorization fenced report on the same owned handles, never a file snapshot.
use crate::{
    access::Permission,
    control_fence::{
        self, Committed, CompanySelection, CookieAuthorization, Failure, SessionRequirement,
    },
};
use rentemester_engine::trial_balance::{Period, Report};
use rentemester_runtime::auth::{Clock, VerifiedCookie};
use rusqlite::Connection;
/// The existing fence serializes control authority and company observation. Its
/// transactions commit without business writes; no journal/audit/idempotency effect.
pub fn trial_balance(
    control: &mut Connection,
    company: &mut Connection,
    cookie: &VerifiedCookie,
    clock: &dyn Clock,
    selection: CompanySelection<'_>,
    period: Period,
) -> Result<Committed<Report>, Failure> {
    control_fence::execute_cookie(
        control,
        company,
        CookieAuthorization {
            cookie,
            clock,
            requirement: SessionRequirement::Current,
        },
        selection,
        Permission::CompanyRead,
        |tx| rentemester_storage::trial_balance_owned(tx, period),
    )
}

/// Complete display snapshot under the same current-session and company-read fence.
pub fn cockpit(
    control: &mut Connection,
    company: &mut Connection,
    cookie: &VerifiedCookie,
    clock: &dyn Clock,
    selection: CompanySelection<'_>,
    request: CockpitRequest<'_>,
) -> Result<Committed<rentemester_storage::cockpit::Snapshot>, Failure> {
    let slug = selection.slug;
    control_fence::execute_cookie(
        control,
        company,
        CookieAuthorization {
            cookie,
            clock,
            requirement: SessionRequirement::Current,
        },
        selection,
        Permission::CompanyRead,
        |tx| {
            rentemester_storage::cockpit::read(
                tx,
                slug,
                request.today,
                request.year,
                request.account,
            )
        },
    )
}
pub struct CockpitRequest<'a> {
    pub today: &'a str,
    pub year: Option<&'a str>,
    pub account: Option<&'a str>,
}

pub fn vat(
    control: &mut Connection,
    company: &mut Connection,
    context: crate::documents::Context<'_>,
    today: &str,
    year: Option<&str>,
) -> Result<Committed<serde_json::Value>, Failure> {
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
        |tx| rentemester_storage::vat_report::read(tx, slug, today, year),
    )
}
