//! Complete an existing source-issued reset link, without HTTP or mail dispatch authority.
use crate::mfa_login::{error, failure};
pub use crate::password_login::{Failure, FailureStage};
use rentemester_runtime::{
    auth::{Clock, new_token},
    password::hash_password,
};
use rentemester_storage::password_reset as store;
use rusqlite::{Connection, TransactionBehavior};
pub struct Runtime<'a> {
    pub clock: &'a dyn Clock,
    pub audit_key_version: u64,
}
/// Match source policy on original UTF-16 code units, before the shared NFKC KDF.
pub fn validate_password(password: &str) -> Result<(), rentemester_engine::Error> {
    if !(12..=128).contains(&password.encode_utf16().count()) {
        Err(error("invalid_password"))
    } else {
        Ok(())
    }
}
/// KDF runs without a SQLite transaction. Only the writer's current unexpired token
/// authorizes mutation; no stale read or previously captured subject is trusted.
pub fn execute(
    control: &mut Connection,
    runtime: &Runtime<'_>,
    token: &str,
    password: &str,
) -> Result<(), Failure> {
    crate::control_fence::connection_ready(control)
        .map_err(|_| failure(error("connection_refused"), false))?;
    store::validate_token(token)
        .and_then(|_| validate_password(password))
        .map_err(|e| failure(e, true))?;
    if runtime.audit_key_version > 9_007_199_254_740_991 {
        return Err(failure(error("invalid_auth_config"), true));
    }
    let hash = hash_password(password).map_err(|e| failure(error(e.code()), true))?;
    let id = new_token().map_err(|e| failure(error(e.code()), true))?;
    #[cfg(test)]
    crate::mfa_login::checkpoint("reset-unlocked");
    let tx = control
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| failure(error("control_busy_or_unavailable"), false))?;
    let result = (|| {
        let now = runtime
            .clock
            .now_unix_ms()
            .map_err(|_| error("invalid_auth_clock"))?;
        store::complete(
            &tx,
            token,
            hash.as_str(),
            id.as_str(),
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
        Ok(())
    })();
    if let Err(e) = result {
        let reusable =
            tx.rollback().is_ok() && crate::control_fence::connection_ready(control).is_ok();
        return Err(failure(e, reusable));
    }
    tx.commit().map_err(|_| Failure {
        stage: FailureStage::OutcomeUnknown,
        error: error("commit_outcome_unknown"),
        connection_reusable: false,
    })?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_utf16_password_policy_precedes_normalization() {
        assert!(validate_password(&"😀".repeat(6)).is_ok());
        assert!(validate_password(&"😀".repeat(64)).is_ok());
        assert!(validate_password(&"😀".repeat(65)).is_err());
        assert!(validate_password(&"x".repeat(11)).is_err());
        assert!(validate_password(&"x".repeat(12)).is_ok());
        assert!(validate_password(&"x".repeat(128)).is_ok());
        assert!(validate_password(&"x".repeat(129)).is_err());
        assert!(validate_password(&" ".repeat(12)).is_ok());
        assert!(validate_password(&"\u{fb03}".repeat(4)).is_err());
    }
}
