//! Effective ownership snapshots and current full-scope authority in ONE canonical control transaction.
use crate::{
    access::{self, CompanyRole, Permission, WorkspaceRole},
    mfa_login::{error, failure},
    password_login::{Failure, FailureStage},
};
use rentemester_engine::ownership::{Endpoint, Fact};
use rentemester_runtime::auth::{Clock, CookieVerifier, VerifiedCookie};
use rentemester_storage::{ownership as storage, workspace_context::Catalog};
use rusqlite::{Connection, TransactionBehavior};
use serde_json::{Value, json};
use std::collections::BTreeSet;
pub use storage::{Apply, Propose, Review};
pub enum Action<'a> {
    Read(&'a str),
    History {
        snapshot_id: Option<&'a str>,
        as_of: Option<&'a str>,
    },
    Propose(&'a Propose),
    Review(&'a Review),
    Apply(&'a Apply),
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
    let permission = match action {
        Action::Read(_) | Action::History { .. } => Permission::CompanyOwnershipRead,
        Action::Propose(_) => Permission::CompanyOwnershipManage,
        _ => Permission::CompanyAdmin,
    };
    let write = matches!(
        action,
        Action::Propose(_) | Action::Review(_) | Action::Apply(_)
    );
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
            || !access::company_role_allows(company, permission)
        {
            return Err(error("access_denied"));
        }
        let mut visible = BTreeSet::new();
        let mut authorized = BTreeSet::new();
        for c in &catalog.companies {
            let f = rentemester_storage::control_access::observe_human(
                &tx,
                &current.subject,
                &current.session_id,
                &c.slug,
            )?;
            if let Some(role) = f.company_role {
                let r: CompanyRole =
                    serde_json::from_value(json!(role)).map_err(|_| error("invalid_control"))?;
                if access::company_role_allows(r, Permission::CompanyOwnershipRead) {
                    visible.insert(c.slug.clone());
                }
                if !c.archived && access::company_role_allows(r, permission) {
                    authorized.insert(c.slug.clone());
                }
            }
        }
        let (corporate_read, corporate_write) =
            crate::parties::company_sets(&tx, &current.subject, catalog)?;
        let scopes = Scopes {
            visible: &visible,
            authorized: &authorized,
            corporate_read: &corporate_read,
            corporate_write: &corporate_write,
        };
        let actor = format!("user:{}", current.subject);
        let pid = current.subject.as_str();
        match action {
            Action::Read(as_of) => read_graph(&tx, as_of, &scopes),
            Action::History { snapshot_id, as_of } => {
                let ids = if let Some(id) = snapshot_id {
                    vec![id.to_owned()]
                } else {
                    storage::ids(&tx)?
                };
                let mut history = vec![];
                for id in ids {
                    let s = match storage::load(&tx, &id) {
                        Ok(s) => s,
                        Err(e) if e.code == "ownership_not_found" => continue,
                        Err(e) => return Err(e),
                    };
                    if accessible(&tx, &s.facts, &scopes)? {
                        history.push(s.public);
                    }
                }
                let mut v = json!({"history":history});
                if let Some(at) = as_of {
                    let graph = read_graph(&tx, at, &scopes)?;
                    if graph["partial"] == false {
                        v["projection"] = storage::projection(&graph);
                    }
                }
                Ok(v)
            }
            Action::Propose(input) => {
                let facts = storage::normalized(&input.facts)?;
                require_write(&tx, &facts, &scopes)?;
                let mut affected = BTreeSet::from([slug.to_owned()]);
                affected.extend(facts.iter().flat_map(Fact::companies));
                // The returned diff includes prior observations for these
                // endpoints, so producing it must not disclose hidden sources.
                for (_, old) in storage::approved(&tx)? {
                    if facts.iter().any(|f| {
                        f.owner == old.owner && f.owned_company_slug == old.owned_company_slug
                    }) {
                        require_write(&tx, std::slice::from_ref(&old), &scopes)?;
                    }
                }
                if let Some(raw_id) = &input.snapshot_id {
                    let id = rentemester_engine::company_knowledge::text(raw_id, 80)?;
                    match storage::load(&tx, &id) {
                        Ok(old) => {
                            require_write(&tx, &old.facts, &scopes)?;
                            affected.extend(old.facts.iter().flat_map(Fact::companies));
                        }
                        Err(e) if e.code == "ownership_not_found" => {}
                        Err(e) => return Err(e),
                    }
                }
                backup.require(&tx, clock, &affected.into_iter().collect::<Vec<_>>())?;
                let generated = format!(
                    "ownership-{}",
                    rentemester_runtime::auth::new_token()
                        .map_err(|_| error("entropy_unavailable"))?
                        .as_str()
                );
                Ok(
                    json!({"snapshot":storage::propose(&tx,input,&facts,&generated,&actor,pid,&stamp)?}),
                )
            }
            Action::Review(input) => {
                let s = storage::load(&tx, &input.snapshot_id)?;
                require_write(&tx, &s.facts, &scopes)?;
                let mut affected = BTreeSet::from([slug.to_owned()]);
                affected.extend(s.facts.iter().flat_map(Fact::companies));
                backup.require(&tx, clock, &affected.into_iter().collect::<Vec<_>>())?;
                Ok(json!({"snapshot":storage::review(&tx,input,&actor,pid,&stamp)?}))
            }
            Action::Apply(input) => {
                let s = storage::load(&tx, &input.snapshot_id)?;
                require_write(&tx, &s.facts, &scopes)?;
                let mut affected = BTreeSet::from([slug.to_owned()]);
                affected.extend(s.facts.iter().flat_map(Fact::companies));
                backup.require(&tx, clock, &affected.into_iter().collect::<Vec<_>>())?;
                storage::apply(&tx, input, &actor, pid, &stamp)
            }
        }
    })();
    match result {
        Ok(v) => {
            if write {
                tx.commit().map_err(|_| Failure {
                    stage: FailureStage::OutcomeUnknown,
                    error: error("commit_outcome_unknown"),
                    connection_reusable: false,
                })?;
            } else if tx.rollback().is_err() {
                return Err(failure(error("connection_refused"), false));
            }
            Ok(v)
        }
        Err(e) => {
            let reusable = tx.rollback().is_ok();
            Err(failure(e, reusable))
        }
    }
}
struct Scopes<'a> {
    visible: &'a BTreeSet<String>,
    authorized: &'a BTreeSet<String>,
    corporate_read: &'a BTreeSet<String>,
    corporate_write: &'a BTreeSet<String>,
}
fn accessible(
    tx: &rusqlite::Transaction<'_>,
    facts: &[Fact],
    s: &Scopes<'_>,
) -> Result<bool, rentemester_engine::Error> {
    for f in facts {
        if !f.companies().is_subset(s.visible) {
            return Ok(false);
        }
        if let Endpoint::Party { party_id } = &f.owner
            && (!rentemester_storage::parties::exists(tx, party_id)?
                || !rentemester_storage::parties::visible(tx, party_id, s.corporate_read)?)
        {
            return Ok(false);
        }
        for reference in &f.evidence_refs {
            if rentemester_storage::corporate_records::exists(tx, reference)?
                && !rentemester_storage::corporate_records::detail_visible(
                    tx,
                    reference,
                    s.corporate_read,
                    s.corporate_write,
                )?
            {
                return Ok(false);
            }
        }
    }
    Ok(true)
}
fn require_write(
    tx: &rusqlite::Transaction<'_>,
    facts: &[Fact],
    s: &Scopes<'_>,
) -> Result<(), rentemester_engine::Error> {
    if !accessible(tx, facts, s)? {
        return Err(error("ownership_not_found"));
    }
    if facts.iter().any(|f| !f.companies().is_subset(s.authorized)) {
        return Err(error("access_denied"));
    }
    Ok(())
}
fn read_graph(
    tx: &rusqlite::Transaction<'_>,
    as_of: &str,
    s: &Scopes<'_>,
) -> Result<Value, rentemester_engine::Error> {
    let at = rentemester_engine::ownership::date(as_of)?;
    let all: Vec<_> = storage::approved(tx)?
        .into_iter()
        .map(|(_, f)| f)
        .filter(|f| f.active(&at))
        .collect();
    let mut visible = vec![];
    for f in &all {
        if accessible(tx, std::slice::from_ref(f), s)? {
            visible.push(f.clone());
        }
    }
    Ok(storage::graph(&at, &all, &visible))
}
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
#[cfg(test)]
mod tests;
