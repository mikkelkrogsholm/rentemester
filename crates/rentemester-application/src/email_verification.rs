//! Native persisted email verification. No HTTP/mail/session authority.
use crate::mfa_login::{error, failure};
pub use crate::password_login::{Failure, FailureStage};
use rentemester_runtime::{
    auth::{AuthError, Clock, new_token},
    email_token::{EmailToken, EmailTokenSigner, VerifiedEmailToken},
};
use rentemester_storage::{email_verification as store, password_credential};
use rusqlite::{Connection, Transaction, TransactionBehavior};
pub struct Runtime<'a> {
    pub signer: &'a EmailTokenSigner,
    pub clock: &'a dyn Clock,
    pub audit_key_version: u64,
}
struct FixedClock(i64);
impl Clock for FixedClock {
    fn now_unix_ms(&self) -> Result<i64, AuthError> {
        Ok(self.0)
    }
}
fn intent<'a>(proof: &'a VerifiedEmailToken, workspace: &'a str) -> store::Intent<'a> {
    store::Intent {
        digest: proof.digest(),
        email: proof.email(),
        workspace,
        issued_at_seconds: proof.issued_at_seconds(),
        expires_at_seconds: proof.expires_at_seconds(),
    }
}
fn write<T>(
    control: &mut Connection,
    runtime: &Runtime<'_>,
    operation: impl FnOnce(&Transaction<'_>, &FixedClock) -> Result<T, rentemester_engine::Error>,
) -> Result<T, Failure> {
    crate::control_fence::connection_ready(control)
        .map_err(|_| failure(error("connection_refused"), false))?;
    if runtime.audit_key_version > 9_007_199_254_740_991 {
        return Err(failure(error("invalid_auth_config"), true));
    }
    #[cfg(test)]
    crate::mfa_login::checkpoint("email-unlocked");
    let tx = control
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| failure(error("control_busy_or_unavailable"), false))?;
    let result = (|| {
        let now = runtime
            .clock
            .now_unix_ms()
            .map_err(|_| error("invalid_auth_clock"))?;
        let value = operation(&tx, &FixedClock(now))?;
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
        Ok(value) => value,
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
    Ok(value)
}
/// Internal issuance result; adapters must conceal account existence in public acknowledgements.
/// A token is released only after its matching intent commits. Unknown commits release none.
pub fn issue(
    control: &mut Connection,
    runtime: &Runtime<'_>,
    email: &str,
) -> Result<Option<EmailToken>, Failure> {
    write(control, runtime, |tx, clock| {
        let email = password_credential::lookup_email(email)?;
        let token = runtime
            .signer
            .issue(&email, clock)
            .map_err(|e| error(e.code()))?;
        let proof = runtime
            .signer
            .verify(token.as_str(), clock)
            .map_err(|e| error(e.code()))?;
        let id = new_token().map_err(|e| error(e.code()))?;
        let known = store::issue(
            tx,
            &intent(&proof, runtime.signer.workspace()),
            id.as_str(),
            clock.0,
        )?;
        Ok(known.then_some(token))
    })
}
/// Verify the original bearer again using the single trusted time sampled AFTER writer admission.
/// Never accepts a previously verified proof as completion authority.
pub fn complete(
    control: &mut Connection,
    runtime: &Runtime<'_>,
    token: &str,
) -> Result<(), Failure> {
    write(control, runtime, |tx, clock| {
        let proof = runtime
            .signer
            .verify(token, clock)
            .map_err(|e| error(e.code()))?;
        store::complete(
            tx,
            &intent(&proof, runtime.signer.workspace()),
            clock.0,
            runtime.audit_key_version,
        )
    })
}
