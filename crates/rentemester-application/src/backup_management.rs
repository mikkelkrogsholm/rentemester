//! Own-factor management on an admitted control handle; no caller identity claims.
pub use crate::mfa_login::{Failure, Runtime};
use crate::mfa_login::{error, failure, now};
use rentemester_runtime::{
    auth::VerifiedCookie,
    mfa::{BackupCodeList, generate_backup_codes, reveal_backup_codes},
};
use rentemester_storage::{
    cookie_session::{self, SessionRequirement},
    mfa_login as store,
    password_credential::{self, NextFactor},
};
use rusqlite::Connection;
#[derive(Clone, Copy)]
pub enum Operation {
    View,
    Regenerate,
}
pub fn execute(
    control: &mut Connection,
    runtime: &Runtime<'_>,
    cookie: &VerifiedCookie,
    password: &str,
    operation: Operation,
) -> Result<BackupCodeList, Failure> {
    crate::control_fence::connection_ready(control)
        .map_err(|_| failure(error("connection_refused"), false))?;
    if !runtime.cookies.accepts_session(cookie) {
        return Err(failure(error("invalid_credentials"), true));
    }
    crate::password_login::execute_captured(
        control,
        password,
        |tx| password_credential::capture_session(tx, cookie.token()),
        |tx, factor, expected_session| {
            let now = now(runtime)?;
            if !runtime.cookies.accepts_session(cookie) || factor.next != NextFactor::Challenge {
                return Err(error("invalid_credentials"));
            }
            let current =
                cookie_session::resolve(tx, cookie.token(), now, SessionRequirement::Fresh)?;
            if current.subject != factor.subject || current.session_id != expected_session {
                return Err(error("invalid_credentials"));
            }
            let pool = store::managed_pool(tx, &current.subject)?;
            let existing = reveal_backup_codes(runtime.keys, pool.encrypted())
                .map_err(|_| error("invalid_control"))?;
            let codes = match operation {
                Operation::View => {
                    pool.finish(
                        tx,
                        None,
                        &current.session_id,
                        runtime.audit_key_version,
                        now,
                    )?;
                    existing
                }
                Operation::Regenerate => {
                    let cipher =
                        generate_backup_codes(runtime.keys).map_err(|e| error(e.code()))?;
                    let codes = reveal_backup_codes(runtime.keys, cipher.as_str())
                        .map_err(|_| error("invalid_control"))?;
                    pool.finish(
                        tx,
                        Some(cipher.as_str()),
                        &current.session_id,
                        runtime.audit_key_version,
                        now,
                    )?;
                    codes
                }
            };
            #[cfg(test)]
            if std::env::var("RM_MFA_FAIL_SQL").ok().as_deref() == Some("1") {
                tx.execute("INSERT INTO main.session(id) VALUES(NULL)", [])
                    .map_err(|_| error("invalid_control"))?;
            }
            #[cfg(test)]
            crate::mfa_login::checkpoint("before-commit");
            Ok(codes)
        },
    )
}
