//! Runtime-bound own-session lifecycle; admitted control connection, no provider logout.
use crate::mfa_login::{error, failure};
pub use crate::password_login::{Failure, FailureStage};
use rentemester_runtime::auth::{Clock, CookieVerifier, VerifiedCookie};
use rentemester_storage::cookie_session::{self, SessionRequirement};
use rentemester_storage::session_control as store;
use rusqlite::{Connection, TransactionBehavior};
pub use store::{Operation, SessionSummary};
pub struct Runtime<'a> {
    pub cookies: &'a CookieVerifier,
    pub clock: &'a dyn Clock,
    pub audit_key_version: u64,
}
pub struct Outcome {
    pub sessions: Vec<SessionSummary>,
    pub deleted: usize,
    pub clear_cookies: Vec<String>,
}
pub fn execute(
    control: &mut Connection,
    runtime: &Runtime<'_>,
    proof: &VerifiedCookie,
    op: Operation<'_>,
) -> Result<Outcome, Failure> {
    crate::control_fence::connection_ready(control)
        .map_err(|_| failure(error("connection_refused"), false))?;
    if !runtime.cookies.accepts_session(proof) {
        return Err(failure(error("invalid_credentials"), true));
    }
    let tx = control
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| failure(error("control_busy_or_unavailable"), false))?;
    let result = (|| {
        let now = runtime
            .clock
            .now_unix_ms()
            .map_err(|_| error("invalid_auth_clock"))?;
        store::check_now(now)?;
        if runtime.audit_key_version > 9_007_199_254_740_991 {
            return Err(error("invalid_auth_config"));
        }
        let value = if matches!(op, Operation::SignOut) {
            store::sign_out(&tx, proof.token(), runtime.audit_key_version, now)?
        } else {
            let current =
                cookie_session::resolve(&tx, proof.token(), now, SessionRequirement::Fresh)?;
            store::execute(&tx, &current, op, runtime.audit_key_version, now)?
        };
        #[cfg(test)]
        if std::env::var("RM_MFA_FAIL_SQL").ok().as_deref() == Some("1") {
            tx.execute("INSERT INTO main.session(id) VALUES(NULL)", [])
                .map_err(|_| error("invalid_control"))?;
        }
        #[cfg(test)]
        crate::mfa_login::checkpoint("before-commit");
        Ok(value)
    })();
    let value = match result {
        Ok(v) => v,
        Err(e) => {
            let reusable =
                tx.rollback().is_ok() && crate::control_fence::connection_ready(control).is_ok();
            return Err(failure(e, reusable));
        }
    };
    tx.commit().map_err(|_| Failure {
        stage: FailureStage::OutcomeUnknown,
        error: error("commit_outcome_unknown"),
        connection_reusable: false,
    })?;
    Ok(Outcome {
        sessions: value.sessions,
        deleted: value.deleted,
        clear_cookies: if value.clear {
            runtime.cookies.clear_native_session_cookies()
        } else {
            Vec::new()
        },
    })
}
