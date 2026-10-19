//! One current-authority control transaction; trusted host journal observations only.
use crate::{
    access::{self, CompanyRole, Permission, WorkspaceRole},
    mfa_login::error,
};
use rentemester_engine::Error;
use rentemester_runtime::auth::{Clock, CookieVerifier, VerifiedCookie};
use rentemester_storage::{intercompany_dispositions as storage, workspace_context::Catalog};
use rusqlite::Transaction;
use serde_json::{Value, json};
pub use storage::Action;
/// Constructed only after current session, both-company and source-scope admission.
/// Not deserializable and never a reusable grant outside its held control transaction.
pub struct Authorized {
    requirements: storage::Requirements,
    principal: storage::Principal,
    at: String,
}
impl Authorized {
    pub fn requirements(&self) -> &storage::Requirements {
        &self.requirements
    }
}
pub fn is_write(action: &Action) -> bool {
    !matches!(action, Action::Plan(_) | Action::Status { .. })
}
pub fn authorize(
    tx: &Transaction<'_>,
    cookies: &CookieVerifier,
    clock: &dyn Clock,
    proof: &VerifiedCookie,
    catalog: &Catalog,
    anchor: Option<&str>,
    action: &mut Action,
) -> Result<Authorized, Error> {
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
        anchor.unwrap_or(""),
    )?;
    let role: WorkspaceRole = serde_json::from_value(json!(
        facts.workspace_role.ok_or_else(|| error("access_denied"))?
    ))
    .map_err(|_| error("invalid_control"))?;
    let write = is_write(action);
    if !access::workspace_role_allows(
        role,
        if write {
            Permission::WorkspaceManage
        } else {
            Permission::WorkspaceRead
        },
    ) {
        return Err(error("access_denied"));
    }
    if write {
        rentemester_storage::workspace_transfer::require_business_write(tx)?;
    }
    let requirements = storage::prepare(tx, action)?;
    if anchor.is_some_and(|s| {
        !requirements
            .dispositions
            .first()
            .is_some_and(|d| d.left.company_slug == s || d.right.company_slug == s)
    }) {
        return Err(error("access_denied"));
    }
    for slug in &requirements.company_slugs {
        rentemester_storage::workspace_members::company(catalog, slug, !write)?;
        let facts = rentemester_storage::control_access::observe_human(
            tx,
            &current.subject,
            &current.session_id,
            slug,
        )?;
        let role: CompanyRole = serde_json::from_value(json!(
            facts.company_role.ok_or_else(|| error("access_denied"))?
        ))
        .map_err(|_| error("invalid_control"))?;
        if !access::company_role_allows(
            role,
            if write {
                Permission::CompanyOwnershipManage
            } else {
                Permission::CompanyOwnershipRead
            },
        ) {
            return Err(error("access_denied"));
        }
    }
    let (read, master) = crate::parties::company_sets(tx, &current.subject, catalog)?;
    for id in &requirements.party_ids {
        if !rentemester_storage::parties::visible(tx, id, &read)? {
            return Err(error("party_not_found"));
        }
    }
    for id in &requirements.evidence_record_ids {
        crate::corporate_records::require_visible(tx, id, &read, &master)?;
    }
    Ok(Authorized {
        requirements,
        principal: storage::Principal {
            kind: "user".into(),
            id: current.subject,
        },
        at: rentemester_engine::company_knowledge::timestamp(now)?,
    })
}
pub fn execute(
    tx: &Transaction<'_>,
    authorized: &Authorized,
    action: &Action,
    snapshots: &[storage::JournalSnapshot],
) -> Result<Value, Error> {
    storage::execute(
        tx,
        action,
        snapshots,
        &format!("user:{}", authorized.principal.id),
        &authorized.principal,
        &authorized.at,
    )
}
