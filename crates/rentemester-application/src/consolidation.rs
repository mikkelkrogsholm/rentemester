//! Authenticated workspace authority and canonical control transaction. No ledger writes.
use crate::{
    access::{self, CompanyRole, Permission, WorkspaceRole},
    mfa_login::{error, failure},
    password_login::{Failure, FailureStage},
};
use rentemester_engine::{
    Error,
    consolidation::{Profile, refused},
    fiscal_year::Date,
};
use rentemester_runtime::auth::{Clock, CookieVerifier, VerifiedCookie};
use rentemester_storage::{consolidation as store, workspace_context::Catalog};
use rusqlite::{Connection, TransactionBehavior};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeSet;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Exact {
    pub profile_id: String,
    pub profile_hash: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Id {
    pub profile_id: String,
}
#[derive(Clone, Copy)]
pub enum Action<'a> {
    Current(&'a str),
    Available(&'a str),
    Plan {
        profile_id: &'a str,
        from: &'a str,
        as_of: &'a str,
    },
    Propose(&'a Profile),
    Approve(&'a Exact),
    Revoke(&'a Id),
}
pub fn execute(
    db: &mut Connection,
    cookies: &CookieVerifier,
    clock: &dyn Clock,
    proof: &VerifiedCookie,
    catalog: &Catalog,
    slug: &str,
    action: Action<'_>,
) -> Result<Value, Failure> {
    crate::control_fence::connection_ready(db).map_err(|e| failure(e, false))?;
    if !cookies.accepts_session(proof) || proof.workspace() != catalog.workspace {
        return Err(failure(error("invalid_credentials"), true));
    }
    rentemester_storage::workspace_context::validate_catalog(catalog)
        .map_err(|e| failure(e, true))?;
    let write = !matches!(
        action,
        Action::Current(_) | Action::Available(_) | Action::Plan { .. }
    );
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| failure(error("control_busy_or_unavailable"), false))?;
    let result = (|| -> Result<Value, Error> {
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
        let f = rentemester_storage::control_access::observe_human(
            &tx,
            &current.subject,
            &current.session_id,
            slug,
        )?;
        let role: WorkspaceRole = serde_json::from_value(json!(
            f.workspace_role.ok_or_else(|| error("access_denied"))?
        ))
        .map_err(|_| error("invalid_control"))?;
        if !access::workspace_role_allows(
            role,
            if write {
                Permission::WorkspaceManage
            } else {
                Permission::WorkspaceGroupRead
            },
        ) {
            return Err(error("access_denied"));
        }
        let mut visible = BTreeSet::new();
        let mut admins = BTreeSet::new();
        let registered: BTreeSet<_> = catalog.companies.iter().map(|c| c.slug.clone()).collect();
        for c in &catalog.companies {
            let f = rentemester_storage::control_access::observe_human(
                &tx,
                &current.subject,
                &current.session_id,
                &c.slug,
            )?;
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
        if !slug.is_empty() && !visible.contains(slug) {
            return Err(error("access_denied"));
        }
        let manifest = rentemester_storage::intercompany::current(&tx, &registered)?;
        let states = match &manifest {
            Some((m, _)) => store::states(&tx, m)?,
            None => Default::default(),
        };
        let actor = format!("user:{}", current.subject);
        let require = |p: &Profile| -> Result<(), Error> {
            if p.companies(&manifest.as_ref().ok_or_else(refused)?.0)?
                .is_subset(&admins)
            {
                Ok(())
            } else {
                Err(error("access_denied"))
            }
        };
        match action {
            Action::Propose(input) => {
                let p = input
                    .clone()
                    .normalize(&manifest.as_ref().ok_or_else(refused)?.0)?;
                require(&p)?;
                store::propose(&tx, &manifest.as_ref().unwrap().0, &p, &actor, &at)
            }
            Action::Approve(exact) => {
                let s = states.get(&exact.profile_id).ok_or_else(refused)?;
                require(&s.profile)?;
                store::approve(&tx, s, &exact.profile_hash, &actor, &at)
            }
            Action::Revoke(id) => {
                let s = states.get(&id.profile_id).ok_or_else(refused)?;
                require(&s.profile)?;
                store::revoke(&tx, s, &actor, &at)
            }
            Action::Current(d) | Action::Available(d) => {
                Date::parse(d)?;
                let mut available = vec![];
                let mut full = vec![];
                if let Some((m, _)) = &manifest {
                    for state in states.values() {
                        let p = &state.profile;
                        let group = m
                            .groups
                            .iter()
                            .find(|g| g.id == p.group_id)
                            .ok_or_else(refused)?;
                        if p.companies(m)?.is_subset(&visible) {
                            full.push(state);
                        }
                        if state.status == "approved"
                            && p.interval.active(d)
                            && group
                                .memberships
                                .iter()
                                .filter(|r| r.interval.active(d))
                                .all(|r| visible.contains(&r.company_slug))
                        {
                            let mut row = json!({"id":p.id,"groupId":p.group_id,"currency":p.currency,"validFrom":p.interval.valid_from});
                            if let Some(end) = &p.interval.valid_to_exclusive {
                                row["validToExclusive"] = json!(end);
                            }
                            available.push(row);
                        }
                    }
                }
                if matches!(action, Action::Available(_)) {
                    Ok(
                        json!({"scope":"consolidation-report-profiles","asOf":d,"profiles":available}),
                    )
                } else {
                    Ok(
                        json!({"scope":"native-consolidation","asOf":d,"profiles":full,"availableProfiles":available}),
                    )
                }
            }
            Action::Plan {
                profile_id,
                from,
                as_of,
            } => {
                Date::parse(from)?;
                Date::parse(as_of)?;
                if from > as_of {
                    return Err(refused());
                }
                let state = states
                    .get(profile_id)
                    .filter(|s| s.status == "approved")
                    .ok_or_else(refused)?;
                let p = &state.profile;
                let (m, mh) = manifest.as_ref().ok_or_else(refused)?;
                let group = m
                    .groups
                    .iter()
                    .find(|g| g.id == p.group_id)
                    .ok_or_else(refused)?;
                let active: Vec<_> = group
                    .memberships
                    .iter()
                    .filter(|r| r.interval.active(as_of))
                    .collect();
                if active.iter().any(|r| !visible.contains(&r.company_slug)) {
                    return Ok(
                        json!({"scope":"consolidated-report","period":{"from":from,"to":as_of},"status":"blocked","blockers":["all active group companies must be visible"],"rawCompanySums":[],"appliedEliminations":[],"consolidatedFigures":null,"sourceSnapshots":[]}),
                    );
                }
                let mut blockers = vec![];
                if !p.interval.active(as_of) {
                    blockers.push("consolidation profile is not active at report date");
                }
                if active.iter().any(|r| {
                    catalog
                        .companies
                        .iter()
                        .any(|c| c.slug == r.company_slug && c.archived)
                }) {
                    blockers.push("active group company is archived");
                }
                if active.len() > 1 {
                    let mut incoming = std::collections::BTreeMap::<&str, u32>::new();
                    for edge in group.ownership.iter().filter(|e| e.interval.active(as_of)) {
                        *incoming.entry(&edge.child_company_slug).or_default() += edge.basis_points;
                    }
                    let roots: Vec<_> = active
                        .iter()
                        .filter(|r| {
                            incoming.get(r.company_slug.as_str()).copied().unwrap_or(0) == 0
                        })
                        .collect();
                    if roots.len() != 1
                        || active.iter().any(|r| {
                            r.company_slug != roots[0].company_slug
                                && incoming.get(r.company_slug.as_str()).copied().unwrap_or(0)
                                    != 10000
                        })
                    {
                        blockers.push("first consolidated-report slice requires one root and 100 percent ownership of every child");
                    }
                }
                let applied=rentemester_storage::eliminations::applied(&tx,as_of)?.into_iter().map(|e|json!({"eliminationId":e.elimination_id,"payloadHash":e.payload_hash,"payload":e.payload})).collect::<Vec<_>>();
                Ok(
                    json!({"profile":p,"profileHash":state.profile_hash,"manifestHash":mh,"companies":active.iter().map(|r|&r.company_slug).collect::<Vec<_>>(),"blockers":blockers,"applied":applied}),
                )
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
