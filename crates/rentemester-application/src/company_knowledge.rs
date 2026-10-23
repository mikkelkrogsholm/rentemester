//! Current authenticated company authority and knowledge effects in ONE control writer.
use crate::{
    access::{self, CompanyRole, Permission, WorkspaceRole},
    mfa_login::{error, failure},
    password_login::{Failure, FailureStage},
};
use rentemester_runtime::auth::{Clock, CookieVerifier, VerifiedCookie};
use rentemester_storage::{company_knowledge as storage, workspace_context::Catalog};
use rusqlite::{Connection, TransactionBehavior};
use serde_json::{Value, json};
pub use storage::{Proposal, Review, Supersede};
pub enum Action<'a> {
    Read {
        as_of: Option<&'a str>,
        include_proposed: bool,
    },
    Propose(&'a Proposal),
    Review(&'a Review),
    Supersede(&'a Supersede),
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
    let write = !matches!(action, Action::Read { .. });
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
                    Permission::CompanyKnowledgeManage
                } else {
                    Permission::CompanyKnowledgeRead
                },
            )
        {
            return Err(error("access_denied"));
        }
        // Rejected reviews and supersessions still target an existing
        // company-scoped assertion. Establish its visibility before exposing
        // the selected company's backup fence; approved review does this in
        // the source lookup below.
        match &action {
            Action::Review(i) if i.decision != "approved" => {
                storage::inspect(&tx, slug, &i.assertion_id)?;
            }
            Action::Supersede(i) => {
                storage::inspect(&tx, slug, &i.assertion_id)?;
            }
            _ => {}
        }
        let source = match &action {
            Action::Propose(i) => Some((i.source.kind.clone(), i.source.r#ref.clone())),
            Action::Supersede(i) => Some((
                i.replacement.source.kind.clone(),
                i.replacement.source.r#ref.clone(),
            )),
            Action::Review(i) if i.decision == "approved" => {
                let old = storage::inspect(&tx, slug, &i.assertion_id)?;
                Some((
                    old["source"]["kind"]
                        .as_str()
                        .ok_or_else(|| error("invalid_control"))?
                        .into(),
                    old["source"]["ref"]
                        .as_str()
                        .ok_or_else(|| error("invalid_control"))?
                        .into(),
                ))
            }
            _ => None,
        };
        if let Some((_, reference)) = source.as_ref().filter(|s| s.0 == "party") {
            let (visible, _) = crate::parties::company_sets(&tx, &current.subject, catalog)?;
            let id = rentemester_engine::company_knowledge::trim(reference);
            if !rentemester_storage::parties::visible(&tx, id, &visible)? {
                return Err(error("knowledge_source_unavailable"));
            }
        }
        if let Some((_, reference)) = source.as_ref().filter(|s| s.0 == "corporate_record") {
            let (visible, writable) = crate::parties::company_sets(&tx, &current.subject, catalog)?;
            crate::corporate_records::require_visible(
                &tx,
                rentemester_engine::company_knowledge::trim(reference),
                &visible,
                &writable,
            )
            .map_err(|_| error("knowledge_source_unavailable"))?;
        }
        if write {
            backup.require(&tx, clock, &[slug.to_owned()])?;
        }
        let id = if matches!(action, Action::Propose(_) | Action::Supersede(_)) {
            format!(
                "knowledge-{}",
                rentemester_runtime::auth::new_token()
                    .map_err(|_| error("entropy_unavailable"))?
                    .as_str()
            )
        } else {
            String::new()
        };
        match action {
            Action::Read {
                as_of,
                include_proposed,
            } => storage::read(&tx, slug, as_of.unwrap_or(&stamp), include_proposed),
            Action::Propose(i) => storage::propose(&tx, slug, i, &id, &current.subject, &stamp)
                .map(|v| json!({"assertion":v})),
            Action::Review(i) => storage::review(&tx, slug, i, &current.subject, &stamp)
                .map(|v| json!({"assertion":v})),
            Action::Supersede(i) => storage::supersede(&tx, slug, i, &id, &current.subject, &stamp),
        }
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

/// Control-owned knowledge uses the same current service admission and budget charge
/// as service context. Authority, usage and append-only effects share this writer.
#[allow(clippy::too_many_arguments)]
pub fn execute_service_scoped(
    db: &mut Connection,
    clock: &dyn Clock,
    key: &rentemester_runtime::service_key::ServiceKey,
    catalog: &Catalog,
    slug: &str,
    action: Action<'_>,
    backup: &mut dyn crate::backup_status::CompanyScopes,
) -> Result<Value, Failure> {
    crate::control_fence::connection_ready(db).map_err(|e| failure(e, false))?;
    if key.workspace() != catalog.workspace {
        return Err(failure(error("invalid_credentials"), true));
    }
    rentemester_storage::workspace_context::validate_catalog(catalog)
        .map_err(|e| failure(e, true))?;
    let write = !matches!(action, Action::Read { .. });
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| failure(error("control_busy_or_unavailable"), false))?;
    let result = (|| {
        if write {
            rentemester_storage::workspace_transfer::require_business_write(&tx)?;
        }
        rentemester_storage::workspace_members::company(catalog, slug, !write)?;
        let now = clock
            .now_unix_ms()
            .map_err(|_| error("invalid_auth_clock"))?;
        let observed =
            rentemester_storage::service_context::observe(&tx, key.lookup_hash(), now, catalog)?;
        let role = rentemester_storage::control_access::observe_company_role(
            &tx,
            observed.subject(),
            slug,
        )?
        .ok_or_else(|| error("access_denied"))?;
        let role: CompanyRole =
            serde_json::from_value(json!(role)).map_err(|_| error("invalid_control"))?;
        let workspace: WorkspaceRole = serde_json::from_value(json!(observed.workspace_role()))
            .map_err(|_| error("invalid_control"))?;
        if !access::workspace_role_allows(workspace, Permission::WorkspaceRead)
            || !access::company_role_allows(
                role,
                if write {
                    Permission::CompanyKnowledgeManage
                } else {
                    Permission::CompanyKnowledgeRead
                },
            )
        {
            return Err(error("access_denied"));
        }
        let source = match &action {
            Action::Propose(i) => Some((i.source.kind.clone(), i.source.r#ref.clone())),
            Action::Supersede(i) => {
                storage::inspect(&tx, slug, &i.assertion_id)?;
                Some((
                    i.replacement.source.kind.clone(),
                    i.replacement.source.r#ref.clone(),
                ))
            }
            Action::Review(i) => {
                let old = storage::inspect(&tx, slug, &i.assertion_id)?;
                Some((
                    old["source"]["kind"]
                        .as_str()
                        .ok_or_else(|| error("invalid_control"))?
                        .into(),
                    old["source"]["ref"]
                        .as_str()
                        .ok_or_else(|| error("invalid_control"))?
                        .into(),
                ))
            }
            _ => None,
        };
        // Never inherit a grantor's or workspace-wide source visibility.
        let selected = std::collections::BTreeSet::from([slug.to_owned()]);
        if let Some((kind, reference)) = source {
            let reference = rentemester_engine::company_knowledge::trim(&reference);
            if kind == "party" && !rentemester_storage::parties::visible(&tx, reference, &selected)?
            {
                return Err(error("knowledge_source_unavailable"));
            }
            if kind == "corporate_record" {
                crate::corporate_records::require_visible(&tx, reference, &selected, &selected)
                    .map_err(|_| error("knowledge_source_unavailable"))?;
            }
        }
        if write {
            backup.require(&tx, clock, &[slug.to_owned()])?;
        }
        // Backup inspection can take time. Resolve expiry/quota at execution time,
        // with the original exact credential still fenced by this held writer.
        let now = clock
            .now_unix_ms()
            .map_err(|_| error("invalid_auth_clock"))?;
        let current =
            rentemester_storage::service_context::observe(&tx, key.lookup_hash(), now, catalog)?;
        if !current.same_credential(&observed) {
            return Err(error("invalid_credentials"));
        }
        let stamp = rentemester_engine::company_knowledge::timestamp(now)?;
        let id = if matches!(action, Action::Propose(_) | Action::Supersede(_)) {
            format!(
                "knowledge-{}",
                rentemester_runtime::auth::new_token()
                    .map_err(|_| error("entropy_unavailable"))?
                    .as_str()
            )
        } else {
            String::new()
        };
        if rentemester_storage::workspace_transfer::current(&tx)?.mode == "active" {
            let charged =
                rentemester_storage::service_context::charge(&tx, key.lookup_hash(), now, catalog)?;
            if !charged.same_credential(&observed) {
                return Err(error("invalid_credentials"));
            }
        }
        let principal = storage::Principal::ServiceAccount(observed.subject());
        match action {
            Action::Read {
                as_of,
                include_proposed,
            } => storage::read(&tx, slug, as_of.unwrap_or(&stamp), include_proposed),
            Action::Propose(i) => storage::propose_attributed(&tx, slug, i, &id, principal, &stamp)
                .map(|v| json!({"assertion":v})),
            Action::Review(i) => storage::review_attributed(&tx, slug, i, principal, &stamp)
                .map(|v| json!({"assertion":v})),
            Action::Supersede(i) => {
                storage::supersede_attributed(&tx, slug, i, &id, principal, &stamp)
            }
        }
    })();
    match result {
        Ok(value) => {
            // Reads can consume service usage, but never change assertions/events.
            tx.commit().map_err(|_| Failure {
                stage: FailureStage::OutcomeUnknown,
                error: error("commit_outcome_unknown"),
                connection_reusable: false,
            })?;
            Ok(value)
        }
        Err(e) => {
            let reusable = tx.rollback().is_ok();
            Err(failure(e, reusable))
        }
    }
}
