//! Private authorized-operator first-user saga. Not an HTTP registration endpoint.
use crate::{
    control_fence::{self, CompanySelection},
    verification_request,
};
use rentemester_engine::Error;
use rentemester_runtime::{
    auth::new_token,
    bootstrap::{Operator, ReservationKey},
    password::hash_password,
    recovery::VerificationSender,
};
use rentemester_storage::{bootstrap as store, password_credential};
use rusqlite::{Connection, Transaction, TransactionBehavior};
pub use store::{Phase, Reservation};

pub struct Runtime<'a> {
    pub reservation_key: &'a ReservationKey,
    pub verification: &'a verification_request::Runtime<'a>,
}
/// Sensitive borrowed input. No Debug/Serialize and no password retained in result.
pub struct Input<'a> {
    pub email: &'a str,
    pub name: &'a str,
    pub password: &'a str,
}
#[derive(Debug)]
pub struct Failure {
    /// Earlier saga commits can survive this failure. None does not imply that an
    /// unknown first commit had no effects; inspect commit_outcome_unknown too.
    pub last_committed_phase: Option<Phase>,
    pub commit_outcome_unknown: bool,
    pub connection_reusable: bool,
    pub error: Error,
}
fn failure(code: &'static str, phase: Option<Phase>, reusable: bool) -> Failure {
    Failure {
        last_committed_phase: phase,
        commit_outcome_unknown: false,
        connection_reusable: reusable,
        error: Error::new(
            code,
            "private workspace bootstrap refused; inspect committed phase",
        ),
    }
}
// ECMAScript trim, matching the private source bootstrap (U+FEFF included,
// U+0085 excluded). Existing native email/password policies still apply.
fn trim(s: &str) -> &str {
    s.trim_matches(|c: char| c != '\u{0085}' && (c.is_whitespace() || c == '\u{feff}'))
}
fn step(
    control: &mut Connection,
    phase: Option<Phase>,
    work: impl FnOnce(&Transaction<'_>) -> Result<Reservation, Error>,
) -> Result<Reservation, Failure> {
    control_fence::connection_ready(control)
        .map_err(|_| failure("connection_refused", phase, false))?;
    let tx = control
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| failure("control_busy_or_unavailable", phase, false))?;
    let result = match work(&tx) {
        Ok(result) => result,
        Err(error) => {
            let reusable =
                tx.rollback().is_ok() && control_fence::connection_ready(control).is_ok();
            return Err(failure(error.code, phase, reusable));
        }
    };
    tx.commit().map_err(|_| Failure {
        last_committed_phase: phase,
        commit_outcome_unknown: true,
        connection_reusable: false,
        error: Error::new(
            "commit_outcome_unknown",
            "private bootstrap commit outcome unknown",
        ),
    })?;
    Ok(result)
}
/// Adapter must resolve operator from trusted local policy, hold workspace lifetime
/// and bound KDF/mail work. All user creation is credential-only; address verification
/// and MFA remain separate. Retrying the same canonical identity preserves its hash.
pub fn execute(
    control: &mut Connection,
    operator: &Operator,
    runtime: &Runtime<'_>,
    sender: &mut dyn VerificationSender,
    selection: CompanySelection<'_>,
    input: Input<'_>,
) -> Result<Reservation, Failure> {
    control_fence::connection_ready(control)
        .map_err(|_| failure("connection_refused", None, false))?;
    let email = password_credential::lookup_email(trim(input.email))
        .map_err(|_| failure("invalid_input", None, true))?;
    if !selection.registered
        || selection.archived
        || selection.workspace != operator.workspace()
        || selection.workspace != runtime.reservation_key.workspace()
        || selection.workspace != runtime.verification.cookies.workspace()
        || selection.workspace != runtime.verification.verification.signer.workspace()
    {
        return Err(failure("bootstrap_operator_refused", None, true));
    }
    runtime
        .verification
        .links
        .validate_redirect(None)
        .map_err(|_| failure("invalid_auth_config", None, true))?;
    let reservation_hash = runtime
        .reservation_key
        .hash_email(&email)
        .map_err(|_| failure("invalid_auth_config", None, true))?;
    let mut current = step(control, None, |tx| {
        store::reserve(tx, &reservation_hash, selection.slug, operator.actor())
    })?;
    if current.phase == Phase::AccessCompleted {
        return Ok(current);
    }
    if current.phase == Phase::Reserved {
        let name = trim(input.name);
        let password = trim(input.password);
        if name.is_empty() || name.len() > 4096 || name.chars().any(char::is_control) {
            return Err(failure("invalid_input", Some(current.phase), true));
        }
        crate::password_reset::validate_password(password)
            .map_err(|_| failure("invalid_input", Some(current.phase), true))?;
        // No SQLite transaction or statement survives the expensive KDF.
        let hash = hash_password(password)
            .map_err(|_| failure("auth_crypto_unavailable", Some(current.phase), true))?;
        let user = new_token()
            .map_err(|_| failure("auth_crypto_unavailable", Some(current.phase), true))?;
        let account = new_token()
            .map_err(|_| failure("auth_crypto_unavailable", Some(current.phase), true))?;
        let now = runtime
            .verification
            .verification
            .clock
            .now_unix_ms()
            .map_err(|_| failure("invalid_auth_clock", Some(current.phase), true))?;
        current = step(control, Some(current.phase), |tx| {
            store::identity(
                tx,
                &reservation_hash,
                selection.slug,
                operator.actor(),
                store::Identity {
                    email: &email,
                    name,
                    password_hash: hash.as_str(),
                    user_id: user.as_str(),
                    account_id: account.as_str(),
                    now,
                },
            )
        })?;
    }
    if current.phase == Phase::IdentityCreated {
        // The existing issuance commits its bound one-time intent before handoff.
        // It returns Ok without sending for an already verified address; that
        // stronger current database observation is accepted, never a caller flag.
        verification_request::execute(control, runtime.verification, sender, &email, None, None)
            .map_err(|f| Failure {
                last_committed_phase: Some(current.phase),
                commit_outcome_unknown: f.stage == crate::reset_request::Stage::OutcomeUnknown,
                connection_reusable: f.connection_reusable,
                error: Error::new(
                    f.error.code,
                    "bootstrap verification handoff did not complete",
                ),
            })?;
        current = step(control, Some(current.phase), |tx| {
            store::mail_confirmed(
                tx,
                &reservation_hash,
                selection.slug,
                &email,
                operator.actor(),
            )
        })?;
    }
    step(control, Some(current.phase), |tx| {
        store::complete(
            tx,
            &reservation_hash,
            selection.slug,
            &email,
            operator.actor(),
        )
    })
}
#[cfg(test)]
mod tests;
