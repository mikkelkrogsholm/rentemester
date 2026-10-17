//! Native request-to-committed-link-to-mail sequence; no public acknowledgement mapper.
use crate::mfa_login::error;
use rentemester_engine::Error;
use rentemester_runtime::{
    auth::{Clock, CookieVerifier, new_reset_token, new_token},
    recovery::{RecoveryLinks, ResetSender, SensitiveUrl},
};
use rentemester_storage::{password_credential, reset_request as store};
use rusqlite::{Connection, TransactionBehavior};
pub struct Runtime<'a> {
    pub cookies: &'a CookieVerifier,
    pub clock: &'a dyn Clock,
    pub links: &'a RecoveryLinks,
    pub audit_key_version: u64,
}
#[derive(Debug, PartialEq, Eq)]
pub enum Stage {
    NotCommitted,
    OutcomeUnknown,
    Committed,
}
#[derive(Debug)]
pub struct Failure {
    pub stage: Stage,
    pub error: Error,
    pub connection_reusable: bool,
}
fn failure(code: &'static str, reusable: bool) -> Failure {
    Failure {
        stage: Stage::NotCommitted,
        error: Error::new(code, "recovery request refused"),
        connection_reusable: reusable,
    }
}
/// Unit acknowledgement contains neither account existence nor a bearer. Internal committed
/// delivery errors must be concealed by a future public mapper to avoid account enumeration.
pub fn execute(
    control: &mut Connection,
    runtime: &Runtime<'_>,
    sender: &mut dyn ResetSender,
    email: &str,
    redirect: Option<&str>,
) -> Result<(), Failure> {
    crate::control_fence::connection_ready(control)
        .map_err(|_| failure("connection_refused", false))?;
    if !sender.enabled() {
        return Err(failure("auth_delivery_disabled", true));
    }
    let email = password_credential::lookup_email(email).map_err(|e| failure(e.code, true))?;
    runtime
        .links
        .validate_redirect(redirect)
        .map_err(|_| failure("invalid_redirect", true))?;
    if runtime.audit_key_version > 9_007_199_254_740_991 {
        return Err(failure("invalid_auth_config", true));
    }
    let token = new_reset_token().map_err(|e| failure(e.code(), true))?;
    let id = new_token().map_err(|e| failure(e.code(), true))?;
    let hash = runtime
        .cookies
        .audit_identity_hash(&email)
        .map_err(|e| failure(e.code(), true))?;
    let message = runtime
        .links
        .message(&email, token.as_str(), redirect)
        .map_err(|_| failure("invalid_redirect", true))?;
    let tx = control
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| failure("control_busy_or_unavailable", false))?;
    let result = (|| {
        let now = runtime
            .clock
            .now_unix_ms()
            .map_err(|_| error("invalid_auth_clock"))?;
        let recipient = store::issue(
            &tx,
            &email,
            store::Issuance {
                token: token.as_str(),
                id: id.as_str(),
                now,
                identity_hash: &hash,
                audit_key_version: runtime.audit_key_version,
            },
        )?;
        #[cfg(test)]
        if std::env::var("RM_MFA_FAIL_SQL").ok().as_deref() == Some("1") {
            tx.execute("INSERT INTO main.session(id) VALUES(NULL)", [])
                .map_err(|_| error("invalid_control"))?;
        }
        #[cfg(test)]
        crate::mfa_login::checkpoint("before-commit");
        Ok(recipient)
    })();
    let recipient = match result {
        Ok(r) => r,
        Err(e) => {
            let reusable =
                tx.rollback().is_ok() && crate::control_fence::connection_ready(control).is_ok();
            return Err(Failure {
                stage: Stage::NotCommitted,
                error: e,
                connection_reusable: reusable,
            });
        }
    };
    tx.commit().map_err(|_| Failure {
        stage: Stage::OutcomeUnknown,
        error: error("commit_outcome_unknown"),
        connection_reusable: false,
    })?;
    #[cfg(test)]
    crate::mfa_login::checkpoint("request-committed");
    if recipient && sender.send(&message).is_err() {
        return Err(Failure {
            stage: Stage::Committed,
            error: error("auth_delivery_unconfirmed"),
            connection_reusable: crate::control_fence::connection_ready(control).is_ok(),
        });
    }
    Ok(())
}
/// Read-only redirect preview. A successful preview neither consumes nor reserves recovery.
pub fn callback(
    control: &mut Connection,
    runtime: &Runtime<'_>,
    token: &str,
    redirect: Option<&str>,
) -> Result<SensitiveUrl, crate::password_login::Failure> {
    use crate::mfa_login::failure;
    crate::control_fence::connection_ready(control)
        .map_err(|_| failure(error("connection_refused"), false))?;
    runtime
        .links
        .validate_redirect(redirect)
        .map_err(|_| failure(error("invalid_redirect"), true))?;
    let tx = control
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .map_err(|_| failure(error("control_busy_or_unavailable"), false))?;
    let result = (|| {
        tx.query_row("SELECT count(*) FROM main.sqlite_schema", [], |r| {
            r.get::<_, i64>(0)
        })
        .map_err(|_| error("invalid_control"))?;
        let now = runtime
            .clock
            .now_unix_ms()
            .map_err(|_| error("invalid_auth_clock"))?;
        let valid = match rentemester_storage::password_reset::inspect(&tx, token, now) {
            Ok(()) => true,
            Err(e) if e.code == "invalid_credentials" => false,
            Err(e) => return Err(e),
        };
        runtime
            .links
            .callback(token, redirect, valid)
            .map_err(|_| error("invalid_redirect"))
    })();
    let reusable = tx.rollback().is_ok() && crate::control_fence::connection_ready(control).is_ok();
    if !reusable {
        return Err(failure(error("connection_refused"), false));
    }
    result.map_err(|e| failure(e, true))
}
