//! Authenticated workspace authority and canonical control transaction. No ledger writes.
use crate::{
    access::{self, CompanyRole, Permission, WorkspaceRole},
    mfa_login::{error, failure},
    password_login::{Failure, FailureStage},
};
use rentemester_engine::{
    Error,
    fiscal_year::Date,
    intercompany::{Manifest, Mapping, refused},
};
use rentemester_runtime::auth::{Clock, CookieVerifier, VerifiedCookie};
use rentemester_storage::{intercompany as store, workspace_context::Catalog};
use rusqlite::{Connection, TransactionBehavior};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeSet;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Exact {
    pub mapping_id: String,
    pub mapping_hash: String,
}
#[derive(Clone, Copy)]
pub enum Action<'a> {
    Current,
    Reconciliation(&'a str),
    Manifest(&'a Manifest),
    Propose(&'a Mapping),
    Approve(&'a Exact),
    Revoke(&'a Exact),
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
    let write = !matches!(action, Action::Current | Action::Reconciliation(_));
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
                Permission::WorkspaceRead
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
        let old = store::current(&tx, &registered)?;
        let actor = format!("user:{}", current.subject);
        let require = |s: &BTreeSet<String>| {
            if s.is_subset(&admins) {
                Ok(())
            } else {
                Err(error("access_denied"))
            }
        };
        if let Action::Manifest(input) = action {
            let m = input.clone().normalize(&registered)?;
            let mut refs = m.companies();
            if let Some((previous, _)) = &old {
                refs.extend(previous.companies());
            }
            require(&refs)?;
            store::states(&tx, &m)?;
            return store::apply_manifest(&tx, &m, old.as_ref().map(|v| v.1.as_str()), &actor, &at);
        }
        let states = match &old {
            Some((m, _)) => store::states(&tx, m)?,
            None => Default::default(),
        };
        match action {
            Action::Current => {
                let current = old
                    .as_ref()
                    .filter(|(m, _)| m.companies().is_subset(&visible));
                let rows: Vec<_> = states
                    .values()
                    .filter(|s| s.mapping.companies().is_subset(&visible))
                    .collect();
                Ok(
                    json!({"manifest":current.map(|v|&v.0),"manifestHash":current.map(|v|&v.1),"mappings":rows,"consolidationAvailable":true}),
                )
            }
            Action::Reconciliation(d) => {
                Date::parse(d)?;
                let rows: Vec<_> = states
                    .values()
                    .filter(|s| {
                        s.status == "approved"
                            && s.mapping.interval.active(d)
                            && s.mapping.companies().is_subset(&visible)
                    })
                    .collect();
                Ok(json!({"scope":"intercompany-reconciliation","asOf":d,"rows":rows}))
            }
            Action::Propose(input) => {
                let m = input
                    .clone()
                    .normalize(&old.as_ref().ok_or_else(refused)?.0)?;
                require(&m.companies())?;
                store::append(&tx, &states, &m, "proposed", &actor, &at)
            }
            Action::Approve(exact) | Action::Revoke(exact) => {
                let s = states.get(&exact.mapping_id).ok_or_else(refused)?;
                require(&s.mapping.companies())?;
                if s.mapping_hash != exact.mapping_hash {
                    return Err(refused());
                }
                store::append(
                    &tx,
                    &states,
                    &s.mapping,
                    if matches!(action, Action::Approve(_)) {
                        "approved"
                    } else {
                        "revoked"
                    },
                    &actor,
                    &at,
                )
            }
            Action::Manifest(_) => Err(refused()),
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
