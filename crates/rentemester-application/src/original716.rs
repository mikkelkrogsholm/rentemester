//! Original operator reads: current credentials and canonical control snapshot only.
use crate::mfa_login::{error, failure};
use crate::password_login::Failure;
use rentemester_runtime::auth::{Clock, CookieVerifier, VerifiedCookie};
use rentemester_storage::workspace_context::Catalog;
use rusqlite::{Connection, TransactionBehavior};
use serde_json::Value;
use std::collections::BTreeSet;

/// The caller retains physical Owner admission throughout this read. Availability is an
/// owner observation of a registered slug, never a request-supplied filesystem path.
pub fn group_overview(
    control: &mut Connection,
    cookies: &CookieVerifier,
    clock: &dyn Clock,
    proof: &VerifiedCookie,
    catalog: &Catalog,
    as_of: &str,
    available: impl Fn(&str) -> bool,
) -> Result<Value, Failure> {
    crate::control_fence::connection_ready(control).map_err(|e| failure(e, false))?;
    if !cookies.accepts_session(proof) || proof.workspace() != catalog.workspace {
        return Err(failure(error("invalid_credentials"), true));
    }
    rentemester_engine::fiscal_year::Date::parse(as_of).map_err(|e| failure(e, true))?;
    rentemester_storage::workspace_context::validate_catalog(catalog).map_err(|e| failure(e, true))?;
    let tx = control.transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| failure(error("control_busy_or_unavailable"), false))?;
    let result = (|| {
        let now = clock.now_unix_ms().map_err(|_| error("invalid_auth_clock"))?;
        let context = rentemester_storage::workspace_context::current(&tx, proof.token(), now, catalog)?;
        let context: Value = serde_json::from_str(context.as_json()).map_err(|_| error("invalid_control"))?;
        let role = serde_json::from_value(context["workspaceRole"].clone()).map_err(|_| error("invalid_control"))?;
        if !crate::access::workspace_role_allows(role, crate::access::Permission::WorkspaceRead) {
            return Err(error("access_denied"));
        }
        let mut visible = BTreeSet::new();
        for company in context["companies"].as_array().ok_or_else(|| error("invalid_control"))? {
            let role = serde_json::from_value(company["role"].clone()).map_err(|_| error("invalid_control"))?;
            if crate::access::company_role_allows(role, crate::access::Permission::CompanyOwnershipRead) {
                visible.insert(company["slug"].as_str().ok_or_else(|| error("invalid_control"))?.to_owned());
            }
        }
        let registered = catalog.companies.iter().map(|c| c.slug.clone()).collect();
        let current = rentemester_storage::intercompany::current(&tx, &registered)?;
        Ok(rentemester_storage::original716::group_overview(
            current.as_ref().map(|v| &v.0), catalog, &visible, as_of, available,
        ))
    })();
    if tx.rollback().is_err() || crate::control_fence::connection_ready(control).is_err() {
        return Err(failure(error("connection_refused"), false));
    }
    result.map_err(|e| failure(e, true))
}
