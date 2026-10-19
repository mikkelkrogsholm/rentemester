//! Control-only invitation/member operations, authorized under the same writer as effects.
use crate::{
    mfa_login::{error, failure},
    password_login::{Failure, FailureStage},
    verification_request,
};
pub use invites::Issue;
use rentemester_engine::Error;
use rentemester_runtime::{
    auth::{Clock, CookieVerifier, VerifiedCookie, new_token},
    invitation::InvitationKey,
    recovery::VerificationSender,
};
use rentemester_storage::{
    workspace_context::Catalog, workspace_invitations as invites, workspace_members as members,
};
use rusqlite::{Connection, Transaction, TransactionBehavior};
use serde_json::{Value, json};
pub enum Action<'a> {
    Members,
    Invitations,
    Issue(&'a Issue),
    Cancel(&'a str),
    Access {
        user: &'a str,
        role: Option<&'a str>,
    },
    Company {
        user: &'a str,
        slug: &'a str,
        role: Option<&'a str>,
    },
}
pub struct Runtime<'a> {
    pub cookies: &'a CookieVerifier,
    pub clock: &'a dyn Clock,
    pub key: &'a InvitationKey,
    pub catalog: &'a Catalog,
}
fn transact<T>(
    db: &mut Connection,
    work: impl FnOnce(&Transaction<'_>) -> Result<T, Error>,
) -> Result<T, Failure> {
    crate::control_fence::connection_ready(db).map_err(|e| failure(e, false))?;
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| failure(error("control_busy_or_unavailable"), false))?;
    match work(&tx) {
        Ok(v) => {
            tx.commit().map_err(|_| Failure {
                stage: FailureStage::OutcomeUnknown,
                error: error("commit_outcome_unknown"),
                connection_reusable: false,
            })?;
            Ok(v)
        }
        Err(e) => {
            let reusable = tx.rollback().is_ok();
            Err(failure(e, reusable))
        }
    }
}
fn now(r: &Runtime<'_>) -> Result<i64, Error> {
    r.clock.now_unix_ms().map_err(|e| error(e.code()))
}
fn admin(
    tx: &Transaction<'_>,
    r: &Runtime<'_>,
    proof: &VerifiedCookie,
    slug: Option<&str>,
    permission: crate::access::Permission,
) -> Result<String, Error> {
    if !r.cookies.accepts_session(proof)
        || proof.workspace() != r.catalog.workspace
        || proof.workspace() != r.key.workspace()
    {
        return Err(error("invalid_credentials"));
    }
    let current = rentemester_storage::cookie_session::resolve(
        tx,
        proof.token(),
        now(r)?,
        rentemester_storage::cookie_session::SessionRequirement::Current,
    )?;
    rentemester_storage::session_control::authorize(tx, &current)?;
    let role = members::access(tx, &current.subject)?.ok_or_else(|| error("access_denied"))?;
    let role: crate::access::WorkspaceRole =
        serde_json::from_value(json!(role)).map_err(|_| error("invalid_control"))?;
    if !crate::access::workspace_role_allows(role, permission) {
        return Err(error("access_denied"));
    }
    if let Some(slug) = slug {
        members::company(r.catalog, slug, false)?;
        let role = members::membership(tx, &current.subject, slug)?
            .ok_or_else(|| error("access_denied"))?;
        let role: crate::access::CompanyRole =
            serde_json::from_value(json!(role)).map_err(|_| error("invalid_control"))?;
        if !crate::access::company_role_allows(role, crate::access::Permission::CompanyAdmin) {
            return Err(error("access_denied"));
        }
    }
    Ok(current.subject)
}
/// Delivery is invoked only after issued intent commits and outside SQLite. No bearer in result.
pub fn execute(
    db: &mut Connection,
    r: &Runtime<'_>,
    proof: &VerifiedCookie,
    action: Action<'_>,
    deliver: &mut dyn FnMut(&str, &str) -> Result<(), ()>,
) -> Result<Value, Failure> {
    rentemester_storage::workspace_context::validate_catalog(r.catalog)
        .map_err(|e| failure(e, true))?;
    match action {
        Action::Issue(input) => {
            let email = invites::canonical(&input.email).map_err(|e| failure(e, true))?;
            let (id, token) = r
                .key
                .generate()
                .map_err(|e| failure(error(e.code()), true))?;
            let th = r
                .key
                .digest("token", token.as_str())
                .map_err(|e| failure(error(e.code()), true))?;
            let eh = r
                .key
                .digest("email", &email)
                .map_err(|e| failure(error(e.code()), true))?;
            transact(db, |tx| {
                let user = admin(
                    tx,
                    r,
                    proof,
                    Some(&input.company_slug),
                    crate::access::Permission::WorkspaceMembersManage,
                )?;
                invites::issue(
                    tx,
                    r.catalog,
                    input,
                    invites::Issuance {
                        id: &id,
                        token_hash: &th,
                        email_hash: &eh,
                        now: now(r)?,
                    },
                    &format!("user:{user}"),
                )
            })?;
            let delivered = deliver(&email, token.as_str()).is_ok();
            let i = transact(db, |tx| {
                invites::status(
                    tx,
                    &id,
                    if delivered {
                        "delivery_confirmed"
                    } else {
                        "delivery_failed"
                    },
                    "system:invitation-delivery",
                )
            })?;
            if !delivered {
                return Err(failure(error("auth_delivery_unconfirmed"), true));
            }
            Ok(json!({"invitation":i}))
        }
        _ => transact(db, |tx| {
            let slug = match &action {
                Action::Company { slug, .. } => Some(*slug),
                _ => None,
            };
            let permission = if matches!(action, Action::Members | Action::Invitations) {
                crate::access::Permission::WorkspaceMembersRead
            } else {
                crate::access::Permission::WorkspaceMembersManage
            };
            let user = admin(tx, r, proof, slug, permission)?;
            let actor = format!("user:{user}");
            match action {
                Action::Members => members::list(tx, r.catalog, &user),
                Action::Invitations => {
                    Ok(json!({"invitations":invites::list(tx,r.catalog,&user)?}))
                }
                Action::Cancel(id) => {
                    let list = invites::list(tx, r.catalog, &user)?;
                    if !list.iter().any(|i| i.invitation_id == id) {
                        return Err(error("access_denied"));
                    }
                    Ok(json!({"invitation":invites::status(tx,id,"cancelled",&actor)?}))
                }
                Action::Access { user, role } => {
                    members::change(tx, r.catalog, user, None, role, &actor)
                }
                Action::Company { user, slug, role } => {
                    members::change(tx, r.catalog, user, Some(slug), role, &actor)
                }
                Action::Issue(_) => unreachable!(),
            }
        }),
    }
}
pub struct Claim<'a> {
    pub token: &'a str,
    pub name: &'a str,
    pub password: &'a str,
}
pub fn claim(
    db: &mut Connection,
    r: &Runtime<'_>,
    verification: &verification_request::Runtime<'_>,
    sender: &mut dyn VerificationSender,
    input: Claim<'_>,
) -> Result<Value, Failure> {
    if r.key.workspace() != r.catalog.workspace
        || r.cookies.workspace() != r.catalog.workspace
        || verification.cookies.workspace() != r.catalog.workspace
        || input.token.len() != 43
        || !input
            .token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(failure(error("invitation_refused"), true));
    }
    let digest = r
        .key
        .digest("token", input.token)
        .map_err(|e| failure(error(e.code()), true))?;
    transact(db, |tx| invites::inspect(tx, &digest, now(r)?, r.catalog))?;
    crate::password_reset::validate_password(input.password).map_err(|e| failure(e, true))?;
    let hash = rentemester_runtime::password::hash_password(input.password)
        .map_err(|e| failure(error(e.code()), true))?;
    let uid = new_token().map_err(|e| failure(error(e.code()), true))?;
    let aid = new_token().map_err(|e| failure(error(e.code()), true))?;
    let (id, email) = transact(db, |tx| {
        let i = invites::inspect(tx, &digest, now(r)?, r.catalog)?;
        let id = invites::identity(
            tx,
            &i,
            invites::Identity {
                user: uid.as_str(),
                account: aid.as_str(),
                name: input.name,
                hash: hash.as_str(),
            },
            now(r)?,
        )?;
        Ok((id, i.email))
    })?;
    verification_request::execute(db, verification, sender, &email, None, None).map_err(|f| {
        Failure {
            stage: if f.stage == crate::reset_request::Stage::OutcomeUnknown {
                FailureStage::OutcomeUnknown
            } else {
                FailureStage::NotCommitted
            },
            error: f.error,
            connection_reusable: f.connection_reusable,
        }
    })?;
    let ready = transact(db, |tx| {
        let i = invites::inspect(tx, &digest, now(r)?, r.catalog)?;
        invites::accept(tx, &i, &id)
    })?;
    Ok(
        json!({"accepted":true,"accessReady":ready,"nextStep":if ready{"sign-in"}else{"verify-email-and-enable-mfa"}}),
    )
}
#[cfg(test)]
mod tests;
