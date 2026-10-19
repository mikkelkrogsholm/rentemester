//! Current workspace authorization while the host holds its single owning mutex.
use crate::{
    access::{Permission, WorkspaceRole},
    mfa_login::{error, failure},
    password_login::{Failure, FailureStage},
};
use rentemester_runtime::auth::{Clock, CookieVerifier, VerifiedCookie};
pub use rentemester_storage::company_creation::{Input, Settings, settings};
use rusqlite::{Connection, TransactionBehavior};
/// Prepare runs only after current WorkspaceManage authorization, under the control
/// writer lock. It creates a fresh company, never mutates an existing ledger.
pub fn execute(
    control: &mut Connection,
    cookies: &CookieVerifier,
    clock: &dyn Clock,
    proof: &VerifiedCookie,
    s: &Settings,
    registered_or_full: bool,
    prepare: impl FnOnce(&str) -> Result<(), rentemester_engine::Error>,
) -> Result<(), Failure> {
    crate::control_fence::connection_ready(control).map_err(|e| failure(e, false))?;
    if !cookies.accepts_session(proof) {
        return Err(failure(error("invalid_credentials"), true));
    }
    let tx = control
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| failure(error("control_busy_or_unavailable"), false))?;
    let auth = (|| {
        let now = clock
            .now_unix_ms()
            .map_err(|_| error("invalid_auth_clock"))?;
        let current = rentemester_storage::cookie_session::resolve(
            &tx,
            proof.token(),
            now,
            rentemester_storage::cookie_session::SessionRequirement::Current,
        )?;
        rentemester_storage::session_control::authorize(&tx, &current)?;
        let role = rentemester_storage::workspace_members::access(&tx, &current.subject)?
            .ok_or_else(|| error("access_denied"))?;
        let role: WorkspaceRole = serde_json::from_value(serde_json::json!(role))
            .map_err(|_| error("invalid_control"))?;
        if !crate::access::workspace_role_allows(role, Permission::WorkspaceManage) {
            return Err(error("access_denied"));
        }
        Ok(current.subject)
    })();
    let subject = match auth {
        Ok(s) => s,
        Err(e) => {
            let reusable =
                tx.rollback().is_ok() && crate::control_fence::connection_ready(control).is_ok();
            return Err(failure(e, reusable));
        }
    };
    if registered_or_full {
        let reusable =
            tx.rollback().is_ok() && crate::control_fence::connection_ready(control).is_ok();
        return Err(failure(
            rentemester_engine::Error::new(
                "company_exists",
                "company slug already registered or catalog limit reached",
            ),
            reusable,
        ));
    }
    if let Err(e) = prepare(&subject)
        .and_then(|()| rentemester_storage::company_creation::grant(&tx, s, &subject))
    {
        let _ = tx.rollback();
        return Err(Failure {
            stage: FailureStage::OutcomeUnknown,
            error: e,
            connection_reusable: false,
        });
    }
    tx.commit().map_err(|_| Failure {
        stage: FailureStage::OutcomeUnknown,
        error: error("company_creation_unconfirmed"),
        connection_reusable: false,
    })
}
