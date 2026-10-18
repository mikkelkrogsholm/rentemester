//! Current-password change with unlocked KDFs and atomic credential/optional session rotation.
use crate::mfa_login::{error, failure};
pub use crate::password_login::{Failure, FailureStage};
use rentemester_runtime::{
    auth::{Clock, CookieVerifier, IssuedCookie, VerifiedCookie, new_token},
    password::{INPUT_BYTE_LIMIT, hash_password},
};
use rentemester_storage::{
    cookie_session::{self, SessionRequirement},
    password_change as store,
    password_credential::{self, NextFactor},
};
use rusqlite::Connection;
pub struct Runtime<'a> {
    pub cookies: &'a CookieVerifier,
    pub clock: &'a dyn Clock,
    pub audit_key_version: u64,
}
pub struct Outcome {
    pub cookie: Option<IssuedCookie>,
    pub user_json: String,
}
pub fn execute(
    control: &mut Connection,
    runtime: &Runtime<'_>,
    proof: &VerifiedCookie,
    current_password: &str,
    new_password: &str,
    revoke_others: bool,
) -> Result<Outcome, Failure> {
    crate::control_fence::connection_ready(control)
        .map_err(|_| failure(error("connection_refused"), false))?;
    if !runtime.cookies.accepts_session(proof) || current_password.len() > INPUT_BYTE_LIMIT {
        return Err(failure(error("invalid_credentials"), true));
    }
    crate::password_reset::validate_password(new_password).map_err(|e| failure(e, true))?;
    // Source also computes the replacement before verifying the old password. Both KDFs are
    // outside SQLite transactions; the serving owner supplies bounded endpoint admission.
    let hash = hash_password(new_password).map_err(|e| failure(error(e.code()), true))?;
    #[cfg(test)]
    crate::mfa_login::checkpoint("change-hashed");
    crate::password_login::execute_captured(
        control,
        current_password,
        |tx| password_credential::capture_session(tx, proof.token()),
        |tx, factor, expected_session| {
            let now = runtime.clock.now_unix_ms().map_err(|e| error(e.code()))?;
            let current =
                cookie_session::resolve(tx, proof.token(), now, SessionRequirement::Fresh)?;
            if factor.next != NextFactor::Challenge
                || factor.subject != current.subject
                || expected_session != current.session_id
            {
                return Err(error("invalid_credentials"));
            }
            let id = if revoke_others {
                Some(new_token().map_err(|e| error(e.code()))?)
            } else {
                None
            };
            let token = if revoke_others {
                Some(new_token().map_err(|e| error(e.code()))?)
            } else {
                None
            };
            let cookie = token
                .as_ref()
                .map(|t| {
                    runtime
                        .cookies
                        .issue_browser_session(t.as_str())
                        .map_err(|e| error(e.code()))
                })
                .transpose()?;
            let rotation = id
                .as_ref()
                .zip(token.as_ref())
                .map(|(id, token)| store::Rotation {
                    id: id.as_str(),
                    token: token.as_str(),
                });
            let user_json = store::complete(
                tx,
                &current,
                hash.as_str(),
                rotation,
                now,
                runtime.audit_key_version,
            )?;
            #[cfg(test)]
            if std::env::var("RM_MFA_FAIL_SQL").ok().as_deref() == Some("1") {
                tx.execute("INSERT INTO main.session(id) VALUES(NULL)", [])
                    .map_err(|_| error("invalid_control"))?;
            }
            #[cfg(test)]
            crate::mfa_login::checkpoint("before-commit");
            Ok(Outcome { cookie, user_json })
        },
    )
}
