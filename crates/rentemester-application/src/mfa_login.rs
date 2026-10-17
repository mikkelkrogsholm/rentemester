//! Native enrolled password/challenge/TOTP-or-backup/session orchestration; admitted control handle only.
pub use crate::password_login::{Failure, FailureStage};
use rentemester_engine::Error;
use rentemester_runtime::{
    auth::{
        Clock, CookieVerifier, IssuedCookie, VerifiedChallenge, new_challenge_token, new_token,
    },
    mfa::{
        BackupCodeList, MfaError, MfaKeyring, consume_backup_code, reveal_backup_codes, verify_totp,
    },
};
use rentemester_storage::{mfa_login as store, password_credential::NextFactor};
use rusqlite::{Connection, TransactionBehavior};
pub struct Runtime<'a> {
    pub cookies: &'a CookieVerifier,
    pub keys: &'a MfaKeyring,
    pub clock: &'a dyn Clock,
    pub audit_key_version: u64,
}
pub enum BeginOutcome {
    Challenge(IssuedCookie),
    EnrollmentRequired,
}
#[derive(Debug, PartialEq, Eq)]
pub enum Rejection {
    InvalidCode,
    ReplayedCode,
    AccountLocked,
}
pub enum VerifyOutcome {
    Session(IssuedCookie),
    Rejected(Rejection),
}
pub(crate) fn error(code: &'static str) -> Error {
    Error::new(code, "native MFA transaction refused")
}
pub(crate) fn failure(error: Error, reusable: bool) -> Failure {
    Failure {
        stage: FailureStage::NotCommitted,
        error,
        connection_reusable: reusable,
    }
}
pub(crate) fn now(runtime: &Runtime<'_>) -> Result<i64, Error> {
    let now = runtime
        .clock
        .now_unix_ms()
        .map_err(|_| error("invalid_auth_clock"))?;
    store::check_now(now)?;
    if runtime.audit_key_version > 9_007_199_254_740_991 {
        return Err(error("invalid_auth_config"));
    }
    Ok(now)
}
pub fn begin(
    control: &mut Connection,
    runtime: &Runtime<'_>,
    email: &str,
    password: &str,
) -> Result<BeginOutcome, Failure> {
    crate::password_login::execute(control, email, password, |tx, factor| {
        let now = now(runtime)?;
        if factor.next == NextFactor::Enrollment {
            return Ok(BeginOutcome::EnrollmentRequired);
        }
        let token = new_challenge_token().map_err(|e| error(e.code()))?;
        let a = new_token().map_err(|e| error(e.code()))?;
        let b = new_token().map_err(|e| error(e.code()))?;
        let c = new_token().map_err(|e| error(e.code()))?;
        let cookie = runtime
            .cookies
            .issue_challenge(token.as_str())
            .map_err(|e| error(e.code()))?;
        store::create(
            tx,
            &factor.subject,
            runtime.cookies.workspace(),
            token.as_str(),
            store::PendingIds {
                challenge: a.as_str(),
                attempts: b.as_str(),
                context: c.as_str(),
            },
            now,
        )?;
        store::audit_begin(tx, &factor.subject, runtime.audit_key_version)?;
        Ok(BeginOutcome::Challenge(cookie))
    })
}
pub fn verify(
    control: &mut Connection,
    runtime: &Runtime<'_>,
    challenge: &VerifiedChallenge,
    code: &str,
) -> Result<VerifyOutcome, Failure> {
    verify_code(control, runtime, challenge, code, store::Method::Totp)
}
pub fn verify_backup(
    control: &mut Connection,
    runtime: &Runtime<'_>,
    challenge: &VerifiedChallenge,
    code: &str,
) -> Result<VerifyOutcome, Failure> {
    verify_code(control, runtime, challenge, code, store::Method::Backup)
}
#[derive(Clone, Copy)]
pub(crate) enum Ceremony {
    Login(store::Method),
    Enrollment,
}
pub(crate) enum CeremonyOutcome {
    Session {
        cookie: IssuedCookie,
        codes: Option<BackupCodeList>,
    },
    Rejected(Rejection),
}
fn verify_code(
    control: &mut Connection,
    runtime: &Runtime<'_>,
    challenge: &VerifiedChallenge,
    code: &str,
    method: store::Method,
) -> Result<VerifyOutcome, Failure> {
    verify_ceremony(control, runtime, challenge, code, Ceremony::Login(method)).map(|o| match o {
        CeremonyOutcome::Session { cookie, .. } => VerifyOutcome::Session(cookie),
        CeremonyOutcome::Rejected(r) => VerifyOutcome::Rejected(r),
    })
}
pub(crate) fn verify_ceremony(
    control: &mut Connection,
    runtime: &Runtime<'_>,
    challenge: &VerifiedChallenge,
    code: &str,
    ceremony: Ceremony,
) -> Result<CeremonyOutcome, Failure> {
    crate::control_fence::connection_ready(control)
        .map_err(|_| failure(error("connection_refused"), false))?;
    if !runtime.cookies.accepts_challenge(challenge) {
        return Err(failure(error("invalid_credentials"), true));
    }
    let tx = control
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| failure(error("control_busy_or_unavailable"), false))?;
    let outcome = (|| {
        let now = now(runtime)?;
        let (attempt, method) = match ceremony {
            Ceremony::Login(method) => (
                store::resolve(&tx, runtime.cookies.workspace(), challenge.token(), now)?,
                method,
            ),
            Ceremony::Enrollment => (
                store::resolve_enrollment(
                    &tx,
                    runtime.cookies.workspace(),
                    challenge.token(),
                    now,
                )?,
                store::Method::Totp,
            ),
        };
        #[cfg(test)]
        checkpoint("locked");
        if attempt.locked(now) {
            attempt.audit_locked(&tx, runtime.audit_key_version, method)?;
            return Ok(CeremonyOutcome::Rejected(Rejection::AccountLocked));
        }
        let replacement;
        let completion = match method {
            store::Method::Backup => {
                match consume_backup_code(runtime.keys, attempt.encrypted_backup_codes(), code) {
                    Ok(Some(value)) => replacement = value,
                    Ok(None) => {
                        attempt.reject(&tx, now, runtime.audit_key_version, method)?;
                        return Ok(CeremonyOutcome::Rejected(Rejection::InvalidCode));
                    }
                    Err(MfaError::CryptoUnavailable) => {
                        return Err(error("auth_crypto_unavailable"));
                    }
                    Err(_) => return Err(error("invalid_control")),
                }
                store::Completion::Backup(replacement.as_str())
            }
            store::Method::Totp => {
                let secret = runtime
                    .keys
                    .decrypt(attempt.encrypted_secret())
                    .map_err(|_| error("invalid_control"))?;
                if secret.as_str().is_empty() || secret.as_str().len() > 1024 {
                    return Err(error("invalid_control"));
                }
                // Freeze the one sampled admission clock for all OTP window calculations.
                struct Sample(i64);
                impl Clock for Sample {
                    fn now_unix_ms(&self) -> Result<i64, rentemester_runtime::auth::AuthError> {
                        Ok(self.0)
                    }
                }
                let counter = match verify_totp(secret.as_str(), code, &Sample(now)) {
                    Ok(counter) => counter,
                    Err(MfaError::InvalidCredentials) => {
                        attempt.reject(&tx, now, runtime.audit_key_version, method)?;
                        return Ok(CeremonyOutcome::Rejected(Rejection::InvalidCode));
                    }
                    Err(e) => return Err(error(e.code())),
                };
                if attempt.replayed(&tx, counter, now)? {
                    attempt.reject(&tx, now, runtime.audit_key_version, method)?;
                    return Ok(CeremonyOutcome::Rejected(Rejection::ReplayedCode));
                }
                store::Completion::Totp(counter)
            }
        };
        let id = new_token().map_err(|e| error(e.code()))?;
        let token = new_token().map_err(|e| error(e.code()))?;
        let replay = new_token().map_err(|e| error(e.code()))?;
        let cookie = runtime
            .cookies
            .issue_session(token.as_str())
            .map_err(|e| error(e.code()))?;
        let ids = store::SessionIds {
            id: id.as_str(),
            token: token.as_str(),
            replay: replay.as_str(),
        };
        let codes = match ceremony {
            Ceremony::Login(_) => {
                attempt.accept(&tx, completion, now, ids, runtime.audit_key_version)?;
                None
            }
            Ceremony::Enrollment => {
                let codes = reveal_backup_codes(runtime.keys, attempt.encrypted_backup_codes())
                    .map_err(|_| error("invalid_control"))?;
                if codes.len() != 10 {
                    return Err(error("invalid_control"));
                }
                let store::Completion::Totp(counter) = completion else {
                    return Err(error("invalid_control"));
                };
                attempt.accept_enrollment(&tx, counter, now, ids, runtime.audit_key_version)?;
                Some(codes)
            }
        };
        #[cfg(test)]
        if std::env::var("RM_MFA_FAIL_SQL").ok().as_deref() == Some("1") {
            tx.execute("INSERT INTO main.session(id) VALUES(NULL)", [])
                .map_err(|_| error("invalid_control"))?;
        }
        #[cfg(test)]
        checkpoint("before-commit");
        Ok(CeremonyOutcome::Session { cookie, codes })
    })();
    let value = match outcome {
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
#[cfg(test)]
pub(crate) fn checkpoint(point: &str) {
    if std::env::var("RM_MFA_POINT").ok().as_deref() == Some(point) {
        std::fs::write(std::env::var("RM_MFA_MARKER").unwrap(), point).unwrap();
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).unwrap();
    }
}
#[cfg(test)]
mod tests;
