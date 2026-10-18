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
