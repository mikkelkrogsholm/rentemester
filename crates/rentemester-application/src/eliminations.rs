//! Current authority and source-bound elimination lifecycle. No legal-entity writes.
use crate::{
    access::{self, CompanyRole, Permission, WorkspaceRole},
    mfa_login::{error, failure},
    password_login::{Failure, FailureStage},
};
use rentemester_engine::{
    Error,
    eliminations::{self as policy, Input, refused},
};
use rentemester_runtime::auth::{Clock, CookieVerifier, VerifiedCookie};
use rentemester_storage::{eliminations as store, workspace_context::Catalog};
use rusqlite::{Connection, TransactionBehavior};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeSet;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Proposal {
    #[serde(flatten)]
    pub input: Input,
    pub expected_payload_hash: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Exact {
    pub elimination_id: String,
    pub payload_hash: String,
}
#[derive(Clone, Copy)]
pub enum Action<'a> {
    Current,
    Overview(&'a str),
    Plan(&'a Input),
    Propose(&'a Proposal),
    Approve(&'a Exact),
    Reject(&'a Exact),
    Apply(&'a Exact),
    Reverse(&'a Exact),
}
#[derive(Clone, Copy)]
enum Mode {
    Prepare,
    Execute,
}
pub fn prepare(
    db: &mut Connection,
    cookies: &CookieVerifier,
    clock: &dyn Clock,
    proof: &VerifiedCookie,
    catalog: &Catalog,
    anchor: Option<&str>,
    action: Action<'_>,
) -> Result<Value, Failure> {
    run(
        db,
        cookies,
        clock,
        proof,
        catalog,
        anchor,
        action,
        None,
        Mode::Prepare,
        None,
    )
}
#[allow(clippy::too_many_arguments)]
pub fn execute_scoped(
    db: &mut Connection,
    cookies: &CookieVerifier,
    clock: &dyn Clock,
    proof: &VerifiedCookie,
    catalog: &Catalog,
    anchor: Option<&str>,
    action: Action<'_>,
    fresh: Option<&Value>,
    backup: &mut dyn crate::backup_status::CompanyScopes,
) -> Result<Value, Failure> {
    run(
        db,
        cookies,
        clock,
        proof,
        catalog,
        anchor,
        action,
        fresh,
        Mode::Execute,
        Some(backup),
    )
}
#[allow(clippy::too_many_arguments)]
fn run(
    db: &mut Connection,
    cookies: &CookieVerifier,
    clock: &dyn Clock,
    proof: &VerifiedCookie,
    catalog: &Catalog,
    anchor: Option<&str>,
    action: Action<'_>,
    fresh: Option<&Value>,
    mode: Mode,
    backup: Option<&mut dyn crate::backup_status::CompanyScopes>,
) -> Result<Value, Failure> {
    crate::control_fence::connection_ready(db).map_err(|e| failure(e, false))?;
    if !cookies.accepts_session(proof) || proof.workspace() != catalog.workspace {
        return Err(failure(error("invalid_credentials"), true));
    }
    rentemester_storage::workspace_context::validate_catalog(catalog)
        .map_err(|e| failure(e, true))?;
    let mutation = !matches!(
        action,
        Action::Current | Action::Overview(_) | Action::Plan(_)
    );
    let write = mutation && matches!(mode, Mode::Execute);
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| failure(error("control_busy_or_unavailable"), false))?;
    let result = (|| -> Result<Value, Error> {
        if write {
            rentemester_storage::workspace_transfer::require_business_write(&tx)?;
        }
        let now = clock
            .now_unix_ms()
            .map_err(|_| error("invalid_auth_clock"))?;
        let at = rentemester_engine::company_knowledge::timestamp(now)?;
        let current = rentemester_storage::cookie_session::resolve(
            &tx,
            proof.token(),
            now,
            rentemester_storage::cookie_session::SessionRequirement::Current,
        )?;
        rentemester_storage::session_control::authorize(&tx, &current)?;
        let mut visible = BTreeSet::new();
        let mut admins = BTreeSet::new();
        let mut workspace_ok = false;
        for c in &catalog.companies {
            let f = rentemester_storage::control_access::observe_human(
                &tx,
                &current.subject,
                &current.session_id,
                &c.slug,
            )?;
            if let Some(r) = f.workspace_role {
                let r: WorkspaceRole =
                    serde_json::from_value(json!(r)).map_err(|_| error("invalid_control"))?;
                workspace_ok = access::workspace_role_allows(
                    r,
                    if mutation {
                        Permission::WorkspaceManage
                    } else {
                        Permission::WorkspaceRead
                    },
                );
            }
            if let Some(r) = f.company_role {
                let r: CompanyRole =
                    serde_json::from_value(json!(r)).map_err(|_| error("invalid_control"))?;
                if access::company_role_allows(r, Permission::CompanyOwnershipRead) {
                    visible.insert(c.slug.clone());
                }
                if !c.archived && access::company_role_allows(r, Permission::CompanyAdmin) {
                    admins.insert(c.slug.clone());
                }
            }
        }
        if !workspace_ok || anchor.is_some_and(|s| !visible.contains(s)) {
            return Err(error("access_denied"));
        }
        let states = store::states(&tx)?;
        let actor = format!("user:{}", current.subject);
        match action {
            Action::Current => {
                return Ok(
                    json!({"rows":states.values().filter(|s|s.payload.companies().is_subset(&visible)).collect::<Vec<_>>()}),
                );
            }
            Action::Overview(d) => {
                let rows=store::applied(&tx,d)?.into_iter().filter(|s|s.payload.companies().is_subset(&visible)).map(|s|json!({"status":"applied","eliminationId":s.elimination_id,"payloadHash":s.payload_hash,"eventHash":s.event_hash,"payload":s.payload,"blockers":[]})).collect::<Vec<_>>();
                return Ok(json!({"scope":"consolidation-eliminations","asOf":d,"rows":rows}));
            }
            _ => {}
        }
        let (input, old) = match action {
            Action::Plan(i) => (i.clone().normalize()?, None),
            Action::Propose(i) => (i.input.clone().normalize()?, None),
            Action::Approve(e) | Action::Reject(e) | Action::Apply(e) | Action::Reverse(e) => {
                let s = states.get(&e.elimination_id).ok_or_else(refused)?;
                if s.payload_hash != e.payload_hash {
                    return Err(refused());
                }
                if !(if mutation { &admins } else { &visible }).is_superset(&s.payload.companies())
                {
                    return Err(error("access_denied"));
                }
                (s.payload.input(), Some(s))
            }
            _ => return Err(refused()),
        };
        let registered = catalog.companies.iter().map(|c| c.slug.clone()).collect();
        let manifest = rentemester_storage::intercompany::current(&tx, &registered)?
            .ok_or_else(refused)?
            .0;
        let mappings = rentemester_storage::intercompany::states(&tx, &manifest)?;
        let mapping = mappings.get(&input.mapping_id).ok_or_else(refused)?;
        let mut endpoints = mapping.mapping.companies();
        if let Some(previous) = old {
            endpoints.extend(previous.payload.companies());
        }
        if !endpoints.is_subset(if mutation { &admins } else { &visible }) {
            return Err(error("access_denied"));
        }
        let needs_fresh = matches!(
            action,
            Action::Plan(_) | Action::Propose(_) | Action::Approve(_) | Action::Apply(_)
        );
        if matches!(mode, Mode::Prepare) {
            return Ok(
                json!({"snapshot":if needs_fresh{serde_json::to_value(&input).map_err(|_|refused())?}else{Value::Null}}),
            );
        }
        let payload = if needs_fresh {
            if mapping.status != "approved" {
                return Err(refused());
            }
            let fresh = fresh.ok_or_else(refused)?;
            if fresh["scope"] != "intercompany-reconciliation" || fresh["asOf"] != input.as_of {
                return Err(refused());
            }
            let row = fresh["rows"]
                .as_array()
                .ok_or_else(refused)?
                .iter()
                .find(|r| r["mappingId"] == input.mapping_id)
                .ok_or_else(refused)?;
            let payload = policy::derive(&input, &mapping.mapping, &mapping.mapping_hash, row)?;
            if let Some(old) = old {
                if !old.payload.same_current_snapshot(&payload)? {
                    return Err(refused());
                }
                old.payload.clone()
            } else {
                payload
            }
        } else {
            old.ok_or_else(refused)?.payload.clone()
        };
        let hash = rentemester_storage::intercompany::sha(&payload.canonical()?);
        if let Action::Plan(_) = action {
            return Ok(json!({"plan":{"payload":payload,"payloadHash":hash}}));
        }
        if let Action::Propose(p) = action
            && p.expected_payload_hash != hash
        {
            return Err(refused());
        }
        let kind = match action {
            Action::Propose(_) => "proposed",
            Action::Approve(_) => "approved",
            Action::Reject(_) => "rejected",
            Action::Apply(_) => "applied",
            Action::Reverse(_) => "reversed",
            _ => return Err(refused()),
        };
        endpoints.extend(payload.companies());
        if !endpoints.is_subset(&admins) {
            return Err(error("access_denied"));
        }
        backup
            .ok_or_else(|| error("backup_observation_required"))?
            .require(&tx, clock, &endpoints.into_iter().collect::<Vec<_>>())?;
        store::append(&tx, &payload, kind, &actor, &at)
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

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub fn execute(
    db: &mut Connection,
    cookies: &CookieVerifier,
    clock: &dyn Clock,
    proof: &VerifiedCookie,
    catalog: &Catalog,
    anchor: Option<&str>,
    action: Action<'_>,
    fresh: Option<&Value>,
) -> Result<Value, Failure> {
    execute_scoped(
        db,
        cookies,
        clock,
        proof,
        catalog,
        anchor,
        action,
        fresh,
        &mut crate::backup_status::UnlockedTestScopes,
    )
}
