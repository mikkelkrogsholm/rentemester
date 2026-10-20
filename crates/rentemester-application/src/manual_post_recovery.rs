//! Current-principal inspection and explicit acknowledgement; never executes a post.
use crate::{
    access::Permission,
    control_fence::{
        self, Committed, CompanySelection, CookieAuthorization, Failure, SessionRequirement,
    },
};
use rentemester_runtime::auth::{Clock, VerifiedCookie};
use rusqlite::Connection;
use serde_json::Value;
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    cookie: &VerifiedCookie,
    clock: &dyn Clock,
    selection: CompanySelection<'_>,
    acknowledge: Option<i64>,
) -> Result<Committed<Value>, Failure> {
    control_fence::execute_cookie_subject(
        control,
        company,
        CookieAuthorization {
            cookie,
            clock,
            requirement: SessionRequirement::Current,
        },
        selection,
        if acknowledge.is_some() {
            Permission::CompanyLedgerPost
        } else {
            Permission::CompanyRead
        },
        |tx, subject| match acknowledge {
            Some(id) => rentemester_storage::manual_post_recovery::acknowledge(tx, subject, id),
            None => rentemester_storage::manual_post_recovery::inventory(tx, subject),
        },
    )
}
