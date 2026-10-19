//! Current read authority retained on the same control source barrier as financial collection.
use crate::{
    access::{self, Permission, WorkspaceRole},
    mfa_login::error,
};
use rentemester_engine::{
    Error,
    cfo_analytics::{Query, Selection},
};
use rentemester_runtime::auth::{Clock, CookieVerifier, VerifiedCookie};
use rentemester_storage::workspace_context::Catalog;
use rusqlite::Transaction;
use serde_json::json;
use std::collections::BTreeSet;
#[derive(PartialEq, Eq)]
pub struct Authorized {
    pub companies: Vec<String>,
    pub partial: bool,
    pub visible: BTreeSet<String>,
    subject: String,
    pub group_supported: bool,
}
impl Authorized {
    pub fn selection(&self) -> Selection {
        Selection {
            companies: self.companies.clone(),
            partial: self.partial,
        }
    }
}
pub fn authorize(
    tx: &Transaction<'_>,
    cookies: &CookieVerifier,
    clock: &dyn Clock,
    proof: &VerifiedCookie,
    catalog: &Catalog,
    query: &Query,
) -> Result<Authorized, Error> {
    query.validate()?;
    if !cookies.accepts_session(proof) || proof.workspace() != catalog.workspace {
        return Err(error("invalid_credentials"));
    }
    rentemester_storage::workspace_context::validate_catalog(catalog)?;
    let now = clock
        .now_unix_ms()
        .map_err(|_| error("invalid_auth_clock"))?;
    let current = rentemester_storage::cookie_session::resolve(
        tx,
        proof.token(),
        now,
        rentemester_storage::cookie_session::SessionRequirement::Current,
    )?;
    rentemester_storage::session_control::authorize(tx, &current)?;
    let facts = rentemester_storage::control_access::observe_human(
        tx,
        &current.subject,
        &current.session_id,
        "",
    )?;
    let role: WorkspaceRole = serde_json::from_value(json!(
        facts.workspace_role.ok_or_else(|| error("access_denied"))?
    ))
    .map_err(|_| error("invalid_control"))?;
    if !access::workspace_role_allows(role, Permission::WorkspaceRead) {
        return Err(error("access_denied"));
    }
    let (visible, _) = crate::parties::company_sets(tx, &current.subject, catalog)?;
    let mut requested = if query.company_slugs.is_empty() {
        catalog
            .companies
            .iter()
            .filter(|c| !c.archived)
            .map(|c| c.slug.clone())
            .collect::<Vec<_>>()
    } else {
        query.company_slugs.clone()
    };
    requested.sort();
    requested.dedup();
    // Unknown and hidden explicit references share a refusal for a direct company.
    // Portfolio never discloses their identities or their count.
    let companies = requested
        .iter()
        .filter(|s| visible.contains(*s) && catalog.companies.iter().any(|c| c.slug == **s))
        .cloned()
        .collect::<Vec<_>>();
    let partial = companies.len() != requested.len();
    if query.scope == "company" && partial {
        return Err(error("access_denied"));
    }
    let mut group_supported = true;
    if query.scope == "group" {
        let registered = catalog.companies.iter().map(|c| c.slug.clone()).collect();
        group_supported = match rentemester_storage::intercompany::current(tx, &registered)? {
            Some((manifest, _)) => {
                let states = rentemester_storage::consolidation::states(tx, &manifest)?;
                match states.get(query.group_profile_id.as_deref().unwrap_or("")) {
                    Some(state) if state.status == "approved" => {
                        let group = manifest
                            .groups
                            .iter()
                            .find(|g| g.id == state.profile.group_id)
                            .ok_or_else(|| error("cfo_source_unavailable"))?;
                        group
                            .memberships
                            .iter()
                            .filter(|m| m.interval.active(&query.to))
                            .all(|m| companies.contains(&m.company_slug))
                    }
                    _ => false,
                }
            }
            None => false,
        };
    }
    Ok(Authorized {
        companies,
        partial,
        visible,
        subject: current.subject,
        group_supported,
    })
}

/// Fixed control-only selection preflight for the canonical group delegate.
pub fn current(
    db: &mut rusqlite::Connection,
    cookies: &CookieVerifier,
    clock: &dyn Clock,
    proof: &VerifiedCookie,
    catalog: &Catalog,
    query: &Query,
) -> Result<Authorized, crate::password_login::Failure> {
    crate::control_fence::connection_ready(db).map_err(|e| crate::mfa_login::failure(e, false))?;
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|_| crate::mfa_login::failure(error("control_busy_or_unavailable"), false))?;
    let result = authorize(&tx, cookies, clock, proof, catalog, query);
    if tx.rollback().is_err() {
        return Err(crate::mfa_login::failure(
            error("connection_refused"),
            false,
        ));
    }
    result.map_err(|e| crate::mfa_login::failure(e, true))
}
