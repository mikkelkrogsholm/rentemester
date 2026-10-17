//! Internal password-to-pending-factor transaction. No HTTP/session/MFA provider is enabled.
use rentemester_engine::Error;
use rentemester_runtime::password::verify_password;
use rentemester_storage::password_credential::{self, FirstFactor};
use rusqlite::{Connection, Transaction, TransactionBehavior};

#[derive(Debug, PartialEq, Eq)]
pub enum FailureStage {
    NotCommitted,
    OutcomeUnknown,
}
#[derive(Debug)]
pub struct Failure {
    pub stage: FailureStage,
    pub error: Error,
    pub connection_reusable: bool,
}
fn failure(error: Error, connection_reusable: bool) -> Failure {
    Failure {
        stage: FailureStage::NotCommitted,
        error,
        connection_reusable,
    }
}
fn unavailable() -> Error {
    Error::new(
        "control_busy_or_unavailable",
        "credential transaction unavailable",
    )
}
/// Same caller-owned admitted handle for both snapshots; no connection or identity claims
/// come from request data. Caller owns workspace lifetime, bounded blocking and endpoint gates.
/// Callback is trusted synchronous pending-factor Rust code, never a session issuer or SQL sandbox.
pub fn execute<T>(
    control: &mut Connection,
    email: &str,
    password: &str,
    continue_factor: impl FnOnce(&Transaction<'_>, FirstFactor) -> Result<T, Error>,
) -> Result<T, Failure> {
    // Preserve the public email operation's preflight/error precedence.
    crate::control_fence::connection_ready(control).map_err(|_| {
        failure(
            Error::new("connection_refused", "credential connection refused"),
            false,
        )
    })?;
    let email = password_credential::lookup_email(email).map_err(|e| failure(e, true))?;
    execute_captured(
        control,
        password,
        |tx| password_credential::capture(tx, &email).map(|c| c.map(|c| (c, ()))),
        |tx, factor, ()| continue_factor(tx, factor),
    )
}
/// Candidate metadata has no authority: the continuation must re-resolve current session facts.
pub(crate) fn execute_captured<M, T>(
    control: &mut Connection,
    password: &str,
    capture: impl FnOnce(
        &Transaction<'_>,
    ) -> Result<Option<(password_credential::CredentialSnapshot, M)>, Error>,
    continue_factor: impl FnOnce(&Transaction<'_>, FirstFactor, M) -> Result<T, Error>,
) -> Result<T, Failure> {
    crate::control_fence::connection_ready(control).map_err(|_| {
        failure(
            Error::new("connection_refused", "credential connection refused"),
            false,
        )
    })?;
    if password.len() > rentemester_runtime::password::INPUT_BYTE_LIMIT {
        return Err(failure(password_credential::denied(), true));
    }
    let read = control
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .map_err(|_| failure(unavailable(), false))?;
    // Acquire a stable read snapshot before schema/data admission.
    let captured = read
        .query_row("SELECT count(*) FROM main.sqlite_schema", [], |r| {
            r.get::<_, i64>(0)
        })
        .map_err(|_| unavailable())
        .and_then(|_| capture(&read));
    let ended = read.rollback();
    if ended.is_err() {
        return Err(failure(unavailable(), false));
    }
    let candidate = captured.map_err(|e| failure(e, true))?;
    #[cfg(test)]
    checkpoint("unlocked");
    verify_password(password, candidate.as_ref().map(|c| c.0.password_hash())).map_err(|e| {
        failure(
            Error::new(e.code(), "credential verification refused"),
            true,
        )
    })?;
    let (candidate, metadata) =
        candidate.ok_or_else(|| failure(password_credential::denied(), true))?;
    let tx = control
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| failure(unavailable(), false))?;
    let result = candidate.recheck(&tx).and_then(|factor| {
        #[cfg(test)]
        checkpoint("locked");
        continue_factor(&tx, factor, metadata)
    });
    let value = match result {
        Ok(value) => value,
        Err(error) => {
            // A trusted callback's raw error may include secrets: normalize it too.
            let error = if [
                "invalid_credentials",
                "invalid_control",
                "control_fence_required",
                "invalid_auth_clock",
                "invalid_auth_config",
                "auth_crypto_unavailable",
            ]
            .contains(&error.code)
            {
                Error::new(error.code, "credential continuation refused")
            } else {
                Error::new("transaction_refused", "credential continuation refused")
            };
            let reusable = tx.rollback().is_ok();
            return Err(failure(error, reusable));
        }
    };
    tx.commit().map_err(|_| Failure {
        stage: FailureStage::OutcomeUnknown,
        error: Error::new(
            "commit_outcome_unknown",
            "credential continuation commit unconfirmed",
        ),
        connection_reusable: false,
    })?;
    Ok(value)
}
#[cfg(test)]
fn checkpoint(point: &str) {
    if std::env::var("RM_PASSWORD_LOGIN_POINT").ok().as_deref() == Some(point) {
        std::fs::write(std::env::var("RM_PASSWORD_LOGIN_MARKER").unwrap(), point).unwrap();
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).unwrap();
    }
}
#[cfg(test)]
mod tests;
