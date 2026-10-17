//! Password-bound setup, isolated from enrolled login. Admitted control handle only.
use crate::mfa_login::{self, Ceremony, CeremonyOutcome, error, now};
pub use crate::mfa_login::{Failure, Rejection, Runtime};
use rentemester_runtime::{
    auth::{IssuedCookie, VerifiedChallenge, new_challenge_token, new_token},
    mfa::{BackupCodeList, EnrollmentUri, enrollment_uri, generate_backup_codes, new_totp_secret},
};
use rentemester_storage::{mfa_login as store, password_credential::NextFactor};
use rusqlite::Connection;
/// Contains the factor secret; neither Debug nor serializable. Only known-commit output.
pub struct Setup {
    pub cookie: IssuedCookie,
    pub uri: EnrollmentUri,
}
pub enum ConfirmOutcome {
    Completed {
        cookie: IssuedCookie,
        codes: BackupCodeList,
    },
    Rejected(Rejection),
}
pub fn begin(
    control: &mut Connection,
    runtime: &Runtime<'_>,
    email: &str,
    password: &str,
    issuer: &str,
) -> Result<Setup, Failure> {
    crate::password_login::execute(control, email, password, |tx, factor| {
        let now = now(runtime)?;
        if factor.next != NextFactor::Enrollment {
            return Err(error("invalid_credentials"));
        }
        let secret = new_totp_secret().map_err(|e| error(e.code()))?;
        let uri =
            enrollment_uri(issuer, &factor.email, secret.as_str()).map_err(|e| error(e.code()))?;
        let encrypted = runtime
            .keys
            .encrypt(secret.as_str())
            .map_err(|e| error(e.code()))?;
        let backup = generate_backup_codes(runtime.keys).map_err(|e| error(e.code()))?;
        let id = new_token().map_err(|e| error(e.code()))?;
        store::prepare_enrollment(
            tx,
            &factor.subject,
            id.as_str(),
            encrypted.as_str(),
            backup.as_str(),
            now,
        )?;
        let token = new_challenge_token().map_err(|e| error(e.code()))?;
        let a = new_token().map_err(|e| error(e.code()))?;
        let b = new_token().map_err(|e| error(e.code()))?;
        let c = new_token().map_err(|e| error(e.code()))?;
        let cookie = runtime
            .cookies
            .issue_challenge(token.as_str())
            .map_err(|e| error(e.code()))?;
        store::create_enrollment(
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
        store::audit_enrollment_begin(tx, &factor.subject, runtime.audit_key_version)?;
        #[cfg(test)]
        if std::env::var("RM_MFA_FAIL_SQL").ok().as_deref() == Some("1") {
            tx.execute("INSERT INTO main.session(id) VALUES(NULL)", [])
                .map_err(|_| error("invalid_control"))?;
        }
        #[cfg(test)]
        mfa_login::checkpoint("before-commit");
        Ok(Setup { cookie, uri })
    })
}
pub fn confirm(
    control: &mut Connection,
    runtime: &Runtime<'_>,
    challenge: &VerifiedChallenge,
    code: &str,
) -> Result<ConfirmOutcome, Failure> {
    mfa_login::verify_ceremony(control, runtime, challenge, code, Ceremony::Enrollment).map(|o| {
        match o {
            CeremonyOutcome::Session {
                cookie,
                codes: Some(codes),
            } => ConfirmOutcome::Completed { cookie, codes },
            CeremonyOutcome::Session { .. } => {
                unreachable!("enrollment completion always carries codes")
            }
            CeremonyOutcome::Rejected(r) => ConfirmOutcome::Rejected(r),
        }
    })
}
