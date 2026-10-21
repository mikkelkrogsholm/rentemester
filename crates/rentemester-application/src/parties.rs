//! Canonical workspace parties authorized through current company sets on the SAME control writer.
use crate::{
    access::{self, CompanyRole, Permission, WorkspaceRole},
    mfa_login::{error, failure},
    password_login::{Failure, FailureStage},
};
use rentemester_runtime::auth::{Clock, CookieVerifier, VerifiedCookie};
use rentemester_storage::{parties as storage, workspace_context::Catalog};
use rusqlite::{Connection, TransactionBehavior};
use serde_json::{Value, json};
use std::collections::BTreeSet;
pub use storage::{Alias, Assertion, Create, Role};
pub enum Action<'a> {
    List {
        query: Option<&'a str>,
        cursor: usize,
        limit: usize,
    },
    Inspect(&'a str),
    Create(&'a Create),
    Role(&'a str, &'a Role),
    Assertion(&'a str, &'a Assertion),
    Alias(&'a str, &'a Alias),
}
/// Full live company sets from trusted catalog/current control. No workspace-owner bypass.
pub fn company_sets(
    tx: &rusqlite::Transaction<'_>,
    subject: &str,
    catalog: &Catalog,
) -> Result<(BTreeSet<String>, BTreeSet<String>), rentemester_engine::Error> {
    let mut read = BTreeSet::new();
    let mut write = BTreeSet::new();
    for c in &catalog.companies {
        let role = rentemester_storage::control_access::observe_company_role(tx, subject, &c.slug)?;
        if let Some(role) = role {
            let role: CompanyRole =
                serde_json::from_value(json!(role)).map_err(|_| error("invalid_control"))?;
            if access::company_role_allows(role, Permission::CompanyRead) {
                read.insert(c.slug.clone());
            }
            if !c.archived && access::company_role_allows(role, Permission::CompanyMasterData) {
                write.insert(c.slug.clone());
            }
        }
    }
    Ok((read, write))
}
#[allow(clippy::too_many_arguments)]
pub fn execute_scoped(
    db: &mut Connection,
    cookies: &CookieVerifier,
    clock: &dyn Clock,
    proof: &VerifiedCookie,
    catalog: &Catalog,
    slug: &str,
    action: Action<'_>,
    backup: &mut dyn crate::backup_status::CompanyScopes,
) -> Result<Value, Failure> {
    crate::control_fence::connection_ready(db).map_err(|e| failure(e, false))?;
    if !cookies.accepts_session(proof) || proof.workspace() != catalog.workspace {
        return Err(failure(error("invalid_credentials"), true));
    }
    rentemester_storage::workspace_context::validate_catalog(catalog)
        .map_err(|e| failure(e, true))?;
    let write = !matches!(action, Action::List { .. } | Action::Inspect(_));
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| failure(error("control_busy_or_unavailable"), false))?;
    let result = (|| {
        if write {
            rentemester_storage::workspace_transfer::require_business_write(&tx)?;
        }
        let now = clock
            .now_unix_ms()
            .map_err(|_| error("invalid_auth_clock"))?;
        let stamp = rentemester_engine::company_knowledge::timestamp(now)?;
        let current = rentemester_storage::cookie_session::resolve(
            &tx,
            proof.token(),
            now,
            rentemester_storage::cookie_session::SessionRequirement::Current,
        )?;
        rentemester_storage::session_control::authorize(&tx, &current)?;
        rentemester_storage::workspace_members::company(catalog, slug, !write)?;
        let facts = rentemester_storage::control_access::observe_human(
            &tx,
            &current.subject,
            &current.session_id,
            slug,
        )?;
        let workspace: WorkspaceRole = serde_json::from_value(json!(
            facts.workspace_role.ok_or_else(|| error("access_denied"))?
        ))
        .map_err(|_| error("invalid_control"))?;
        let company: CompanyRole = serde_json::from_value(json!(
            facts.company_role.ok_or_else(|| error("access_denied"))?
        ))
        .map_err(|_| error("invalid_control"))?;
        if !access::workspace_role_allows(workspace, Permission::WorkspaceRead)
            || !access::company_role_allows(
                company,
                if write {
                    Permission::CompanyMasterData
                } else {
                    Permission::CompanyRead
                },
            )
        {
            return Err(error("access_denied"));
        }
        let (visible, writable) = company_sets(&tx, &current.subject, catalog)?;
        let actor = format!("user:{}", current.subject);
        let id = match action {
            Action::List {
                query,
                cursor,
                limit,
            } => return storage::list(&tx, &visible, query, cursor, limit),
            Action::Inspect(id) => {
                return storage::inspect(&tx, id, &visible).map(|p| json!({"party":p}));
            }
            Action::Create(input) => {
                if let Some(id) = input.party_id.as_deref() {
                    let id = rentemester_engine::company_knowledge::trim(id);
                    if storage::exists(&tx, id)? && !storage::visible(&tx, id, &visible)? {
                        return Err(error("party_not_found"));
                    }
                }
                backup.require(&tx, clock, &[slug.to_owned()])?;
                let generated = format!(
                    "party-{}",
                    rentemester_runtime::auth::new_token()
                        .map_err(|_| error("entropy_unavailable"))?
                        .as_str()
                );
                let id = storage::create(&tx, input, &generated, &actor, &stamp)?;
                storage::role(
                    &tx,
                    &id,
                    slug,
                    &Role {
                        role: input.role.clone(),
                        defaults: input.defaults.clone(),
                    },
                    &actor,
                    &stamp,
                )?;
                id
            }
            Action::Role(id, input) => {
                storage::inspect(&tx, id, &visible)?;
                backup.require(&tx, clock, &[slug.to_owned()])?;
                storage::role(&tx, id, slug, input, &actor, &stamp)?;
                id.into()
            }
            Action::Assertion(id, input) => {
                authorize_update(&tx, id, &visible, &writable)?;
                let mut scopes = storage::scopes(&tx, id)?;
                scopes.insert(slug.to_owned());
                backup.require(&tx, clock, &scopes.into_iter().collect::<Vec<_>>())?;
                storage::assertion(&tx, id, input, &actor, &stamp)?;
                id.into()
            }
            Action::Alias(id, input) => {
                authorize_update(&tx, id, &visible, &writable)?;
                let mut scopes = storage::scopes(&tx, id)?;
                scopes.insert(slug.to_owned());
                backup.require(&tx, clock, &scopes.into_iter().collect::<Vec<_>>())?;
                storage::alias(&tx, id, input, &actor, &stamp)?;
                id.into()
            }
        };
        storage::inspect(&tx, &id, &visible).map(|p| json!({"party":p}))
    })();
    match result {
        Ok(value) => {
            if write {
                tx.commit().map_err(|_| Failure {
                    stage: FailureStage::OutcomeUnknown,
                    error: error("commit_outcome_unknown"),
                    connection_reusable: false,
                })?;
            } else if tx.rollback().is_err() {
                return Err(failure(error("connection_refused"), false));
            }
            Ok(value)
        }
        Err(e) => {
            let reusable = tx.rollback().is_ok();
            Err(failure(e, reusable))
        }
    }
}
fn authorize_update(
    tx: &rusqlite::Transaction<'_>,
    id: &str,
    visible: &BTreeSet<String>,
    writable: &BTreeSet<String>,
) -> Result<(), rentemester_engine::Error> {
    storage::inspect(tx, id, visible)?;
    let scopes = storage::scopes(tx, id)?;
    if scopes.is_empty() || !scopes.iter().all(|s| writable.contains(s)) {
        return Err(error("access_denied"));
    }
    Ok(())
}
#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) fn execute(
    db: &mut Connection,
    cookies: &CookieVerifier,
    clock: &dyn Clock,
    proof: &VerifiedCookie,
    catalog: &Catalog,
    slug: &str,
    action: Action<'_>,
) -> Result<Value, Failure> {
    execute_scoped(
        db,
        cookies,
        clock,
        proof,
        catalog,
        slug,
        action,
        &mut crate::backup_status::UnlockedTestScopes,
    )
}
