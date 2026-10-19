//! Current control-writer transfer administration, with separately trusted signing keys.
use crate::{
    password_login::{Failure, FailureStage},
    portable_backup,
};
use rentemester_engine::Error;
use rentemester_runtime::{
    auth::{Clock, CookieVerifier, VerifiedCookie, new_token},
    backup_artifact::{SigningKey, TrustedPublicKey},
    workspace_transfer as wire,
};
use rentemester_storage::{workspace_context::Catalog, workspace_transfer as store};
use rusqlite::{Connection, TransactionBehavior};
use serde_json::Value;
pub enum Action<'a> {
    Status,
    Freeze(&'a store::Freeze),
    Cancel(&'a store::Cancel),
    Release(&'a store::Release, &'a SigningKey),
    Activate(&'a store::Activate, &'a TrustedPublicKey),
}
fn fail(error: Error, reusable: bool) -> Failure {
    Failure {
        stage: FailureStage::NotCommitted,
        error,
        connection_reusable: reusable,
    }
}
pub fn execute(
    db: &mut Connection,
    cookies: &CookieVerifier,
    clock: &dyn Clock,
    proof: &VerifiedCookie,
    catalog: &Catalog,
    companies: &[store::Company],
    action: Action<'_>,
) -> Result<Value, Failure> {
    crate::control_fence::connection_ready(db).map_err(|e| fail(e, false))?;
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| fail(store::refused(), false))?;
    let result = (|| {
        if !cookies.accepts_session(proof) || proof.workspace() != catalog.workspace {
            return Err(store::refused());
        }
        let now = clock.now_unix_ms().map_err(|_| store::refused())?;
        let current = rentemester_storage::cookie_session::resolve(
            &tx,
            proof.token(),
            now,
            if matches!(action, Action::Status) {
                rentemester_storage::cookie_session::SessionRequirement::Current
            } else {
                rentemester_storage::cookie_session::SessionRequirement::Fresh
            },
        )?;
        rentemester_storage::session_control::authorize(&tx, &current)?;
        let role = rentemester_storage::workspace_members::access(&tx, &current.subject)?
            .ok_or_else(store::refused)?;
        if !matches!(role.as_str(), "workspace_owner" | "member") {
            return Err(store::refused());
        }
        let full = portable_backup::authorize(&tx, cookies, clock, proof, catalog).is_ok();
        let s = store::current(&tx)?;
        if matches!(action, Action::Status) {
            let mut v = if full {
                s.value()
            } else {
                let mut v = store::State::redacted(&s);
                v["mode"] = serde_json::json!(s.mode);
                v
            };
            v["_full"] = serde_json::json!(full);
            return Ok(v);
        }
        if !full {
            return Err(Error::new(
                "access_denied",
                "current workspace and every company administration required",
            ));
        }
        let actor = format!("user:{}", current.subject);
        let at = rentemester_engine::company_knowledge::timestamp(now)?;
        let fact = match action {
            Action::Status => unreachable!(),
            Action::Freeze(i) => {
                s.reviewed(&i.expected_state_hash, i.confirm)?;
                let random = || {
                    new_token()
                        .map(|t| store::digest(t.as_str().as_bytes())[..32].to_owned())
                        .map_err(|_| store::refused())
                };
                store::Fact::Frozen {
                    transfer_id: random()?,
                    source_instance_id: random()?,
                    workspace_id: catalog.workspace.clone(),
                    destination: store::text(&i.destination, 10, 300)?,
                    reason: store::text(&i.reason, 10, 2000)?,
                    companies: companies.to_vec(),
                }
            }
            Action::Cancel(i) => {
                s.reviewed(&i.expected_state_hash, i.confirm)?;
                store::Fact::Cancelled {
                    reason: store::text(&i.reason, 10, 2000)?,
                }
            }
            Action::Release(i, key) => {
                i.challenge.validate()?;
                let challenge: wire::Challenge = serde_json::from_value(
                    serde_json::to_value(&i.challenge).map_err(|_| store::refused())?,
                )
                .map_err(|_| store::refused())?;
                challenge.validate().map_err(|_| store::refused())?;
                if s.mode == "moved"
                    && i.confirm
                    && s.history
                        .last()
                        .is_some_and(|e| e["previousStateHash"] == i.expected_state_hash)
                    && s.challenge.as_ref() == Some(&i.challenge)
                    && s.release.as_ref().is_some_and(|raw| {
                        wire::verify_release(
                            raw,
                            &TrustedPublicKey::from_bytes(&key.public_key_bytes()).unwrap(),
                        )
                        .is_ok_and(|r| {
                            r.reason == rentemester_engine::company_knowledge::trim(&i.reason)
                        })
                    })
                {
                    return Ok(s.value());
                }
                s.reviewed(&i.expected_state_hash, i.confirm)?;
                if i.challenge.companies != companies {
                    return Err(store::refused());
                }
                let reason = store::text(&i.reason, 10, 2000)?;
                let claims = wire::Release {
                    version: 1,
                    challenge,
                    actor: actor.clone(),
                    at: at.clone(),
                    reason: reason.clone(),
                };
                let release = wire::sign_release(&claims, key).map_err(|_| store::refused())?;
                store::Fact::Moved {
                    challenge: i.challenge.clone(),
                    release,
                    reason,
                }
            }
            Action::Activate(i, key) => {
                if s.mode == "active"
                    && i.confirm
                    && s.release.as_ref() == Some(&i.release)
                    && s.history.last().is_some_and(|e| {
                        e["previousStateHash"] == i.expected_state_hash
                            && e["fact"]["reason"]
                                == rentemester_engine::company_knowledge::trim(&i.reason)
                    })
                {
                    return Ok(s.value());
                }
                s.reviewed(&i.expected_state_hash, i.confirm)?;
                let claims = wire::verify_release(&i.release, key).map_err(|_| store::refused())?;
                let challenge: store::Challenge = serde_json::from_value(
                    serde_json::to_value(claims.challenge).map_err(|_| store::refused())?,
                )
                .map_err(|_| store::refused())?;
                if s.mode != "staged"
                    || s.challenge.as_ref() != Some(&challenge)
                    || challenge.companies != companies
                {
                    return Err(store::refused());
                }
                store::Fact::Activated {
                    release: i.release.clone(),
                    reason: store::text(&i.reason, 10, 2000)?,
                }
            }
        };
        Ok(store::append(&tx, fact, &actor, &at)?.value())
    })();
    if matches!(action, Action::Status) || result.is_err() {
        let clean = tx.rollback().is_ok();
        return result.map_err(|e| fail(e, clean));
    }
    let value = result.unwrap();
    tx.commit().map_err(|_| Failure {
        stage: FailureStage::OutcomeUnknown,
        error: store::refused(),
        connection_reusable: false,
    })?;
    Ok(value)
}

pub fn authorize_fresh(
    tx: &rusqlite::Transaction<'_>,
    cookies: &CookieVerifier,
    clock: &dyn Clock,
    proof: &VerifiedCookie,
    catalog: &Catalog,
) -> Result<(), Error> {
    portable_backup::authorize(tx, cookies, clock, proof, catalog)?;
    let now = clock.now_unix_ms().map_err(|_| store::refused())?;
    rentemester_storage::cookie_session::resolve(
        tx,
        proof.token(),
        now,
        rentemester_storage::cookie_session::SessionRequirement::Fresh,
    )?;
    Ok(())
}
