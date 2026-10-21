//! Corporate records and current full-scope authority in ONE canonical control transaction.
use crate::{
    access::{self, CompanyRole, Permission, WorkspaceRole},
    mfa_login::{error, failure},
    password_login::{Failure, FailureStage},
};
use rentemester_runtime::auth::{Clock, CookieVerifier, VerifiedCookie};
use rentemester_storage::{corporate_records as storage, workspace_context::Catalog};
use rusqlite::{Connection, TransactionBehavior};
use serde_json::{Value, json};
pub use storage::{Enrich, Ingest, LinkInput, Supersede};
pub enum Action<'a> {
    List { cursor: usize, limit: usize },
    Inspect(&'a str),
    Download(&'a str),
    Ingest(&'a Ingest, &'a [u8]),
    Link(&'a str, &'a LinkInput),
    Enrich(&'a str, &'a Enrich),
    Supersede(&'a str, &'a Supersede),
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
    let write = !matches!(
        action,
        Action::List { .. } | Action::Inspect(_) | Action::Download(_)
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
        let (visible, writable) = crate::parties::company_sets(&tx, &current.subject, catalog)?;
        let actor = format!("user:{}", current.subject);
        match action {
            Action::List { cursor, limit } => storage::list(&tx, &visible, cursor, limit),
            Action::Inspect(id) => {
                require_visible(&tx, id, &visible, &writable)?;
                storage::inspect(&tx, id).map(|r| json!({"record":r}))
            }
            Action::Download(id) => {
                require_visible(&tx, id, &visible, &writable)?;
                let bytes = storage::download(&tx, id)?;
                Ok(json!({"bytes":bytes,"sha256":storage::hash(&bytes)}))
            }
            Action::Ingest(input, bytes) => {
                let mut affected = std::collections::BTreeSet::from([slug.to_owned()]);
                if let Some(raw_id) = &input.record_id {
                    let id = rentemester_engine::company_knowledge::trim(raw_id);
                    if storage::exists(&tx, id)? {
                        require_write(&tx, id, &visible, &writable)?;
                        affected.extend(storage::scopes(&tx, id)?);
                    }
                }
                let mut links = input.links.clone().unwrap_or_default();
                if links.len() > 4096 {
                    return Err(error("corporate_record_refused"));
                }
                links.push(storage::Link {
                    r#type: "company".into(),
                    id: slug.into(),
                });
                for link in &links {
                    authorize_link(link, &visible, &writable)?;
                    if link.r#type == "company" {
                        affected.insert(
                            rentemester_engine::company_knowledge::trim(&link.id).to_owned(),
                        );
                    }
                }
                backup.require(&tx, clock, &affected.into_iter().collect::<Vec<_>>())?;
                let generated = format!(
                    "record-{}",
                    rentemester_runtime::auth::new_token()
                        .map_err(|_| error("entropy_unavailable"))?
                        .as_str()
                );
                storage::ingest(&tx, input, bytes, &generated, &links, &actor)
                    .map(|r| json!({"record":r}))
            }
            Action::Link(id, input) => {
                require_write(&tx, id, &visible, &writable)?;
                check_id(id, input.record_id.as_deref())?;
                let link = storage::Link {
                    r#type: input.r#type.clone(),
                    id: input.id.clone(),
                };
                authorize_link(&link, &visible, &writable)?;
                let mut affected = storage::scopes(&tx, id)?;
                affected.insert(slug.to_owned());
                if link.r#type == "company" {
                    affected
                        .insert(rentemester_engine::company_knowledge::trim(&link.id).to_owned());
                }
                backup.require(&tx, clock, &affected.into_iter().collect::<Vec<_>>())?;
                storage::link(&tx, id, &link, &actor, &stamp).map(|r| json!({"record":r}))
            }
            Action::Enrich(id, input) => {
                require_write(&tx, id, &visible, &writable)?;
                check_id(id, input.record_id.as_deref())?;
                let mut affected = storage::scopes(&tx, id)?;
                affected.insert(slug.to_owned());
                backup.require(&tx, clock, &affected.into_iter().collect::<Vec<_>>())?;
                storage::enrich(&tx, id, input, &actor, &stamp)
            }
            Action::Supersede(id, input) => {
                require_write(&tx, id, &visible, &writable)?;
                check_id(id, input.record_id.as_deref())?;
                require_write(&tx, &input.replacement_record_id, &visible, &writable)?;
                // The replacement is read-only evidence; only the old record's
                // metadata is appended. Keep authorization, without locking an
                // unrelated company merely because it supplies a source.
                let mut affected = storage::scopes(&tx, id)?;
                affected.insert(slug.to_owned());
                backup.require(&tx, clock, &affected.into_iter().collect::<Vec<_>>())?;
                storage::supersede(&tx, id, input, &actor, &stamp)
            }
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
fn check_id(id: &str, body: Option<&str>) -> Result<(), rentemester_engine::Error> {
    if body.is_some_and(|v| v != id) {
        return Err(error("corporate_record_refused"));
    }
    Ok(())
}
fn authorize_link(
    link: &storage::Link,
    _visible: &std::collections::BTreeSet<String>,
    write: &std::collections::BTreeSet<String>,
) -> Result<(), rentemester_engine::Error> {
    if link.r#type == "company"
        && !write.contains(rentemester_engine::company_knowledge::trim(&link.id))
    {
        return Err(error("access_denied"));
    }
    Ok(())
}
pub fn require_visible(
    tx: &rusqlite::Transaction<'_>,
    id: &str,
    read: &std::collections::BTreeSet<String>,
    write: &std::collections::BTreeSet<String>,
) -> Result<(), rentemester_engine::Error> {
    if !storage::detail_visible(tx, id, read, write)? {
        return Err(error("corporate_record_not_found"));
    }
    Ok(())
}
fn require_write(
    tx: &rusqlite::Transaction<'_>,
    id: &str,
    read: &std::collections::BTreeSet<String>,
    write: &std::collections::BTreeSet<String>,
) -> Result<(), rentemester_engine::Error> {
    require_visible(tx, id, read, write)?;
    if !storage::scopes(tx, id)?.iter().all(|s| write.contains(s)) {
        return Err(error("access_denied"));
    }
    Ok(())
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
