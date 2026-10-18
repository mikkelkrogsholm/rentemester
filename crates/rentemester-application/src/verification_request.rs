//! Committed native issuance followed by synchronous verification-mail handoff outside SQLite.
use crate::{
    email_verification,
    mfa_login::error,
    reset_request::{Failure, Stage},
};
use rentemester_runtime::{
    auth::{CookieVerifier, VerifiedCookie},
    recovery::{RecoveryLinks, SensitiveUrl, VerificationResult, VerificationSender},
};
use rusqlite::Connection;
pub struct Runtime<'a> {
    pub verification: &'a email_verification::Runtime<'a>,
    pub cookies: &'a CookieVerifier,
    pub links: &'a RecoveryLinks,
}
fn failure(code: &'static str, reusable: bool) -> Failure {
    Failure {
        stage: Stage::NotCommitted,
        error: error(code),
        connection_reusable: reusable,
    }
}
/// Unit acknowledgement contains no account existence, token or delivery assertion. An HTTP
/// adapter must conceal all admitted outcomes (including panics), and enforce public timing/rate.
pub fn execute(
    control: &mut Connection,
    runtime: &Runtime<'_>,
    sender: &mut dyn VerificationSender,
    email: &str,
    redirect: Option<&str>,
    caller: Option<&VerifiedCookie>,
) -> Result<(), Failure> {
    crate::control_fence::connection_ready(control)
        .map_err(|_| failure("connection_refused", false))?;
    if !sender.enabled() {
        return Err(failure("auth_delivery_disabled", true));
    }
    runtime
        .links
        .validate_redirect(redirect)
        .map_err(|_| failure("invalid_redirect", true))?;
    // Prepare the complete bounded message inside the issuance transaction, so malformed or
    // oversize destinations cannot leave a committed intent that could never be delivered.
    let message = email_verification::issue_prepared(
        control,
        runtime.verification,
        email,
        caller.map(|p| (runtime.cookies, p)),
        |token, email| {
            runtime
                .links
                .verification_message(email, token.as_str(), redirect)
                .map_err(|_| error("invalid_redirect"))
        },
    )
    .map_err(|f| Failure {
        stage: match f.stage {
            email_verification::FailureStage::NotCommitted => Stage::NotCommitted,
            email_verification::FailureStage::OutcomeUnknown => Stage::OutcomeUnknown,
        },
        error: f.error,
        connection_reusable: f.connection_reusable,
    })?;
    #[cfg(test)]
    crate::mfa_login::checkpoint("verification-request-committed");
    if let Some(message) = message
        && sender.send(&message).is_err()
    {
        return Err(Failure {
            stage: Stage::Committed,
            error: error("auth_delivery_unconfirmed"),
            connection_reusable: crate::control_fence::connection_ready(control).is_ok(),
        });
    }
    Ok(())
}
/// Validate and construct the successful destination before mutation. Unknown commit never
/// returns it; no verification bearer is injected into it. Omitted callback means a future JSON response.
pub fn complete(
    control: &mut Connection,
    runtime: &Runtime<'_>,
    token: &str,
    redirect: Option<&str>,
) -> Result<Option<SensitiveUrl>, email_verification::Failure> {
    crate::control_fence::connection_ready(control)
        .map_err(|_| crate::mfa_login::failure(error("connection_refused"), false))?;
    let destination = runtime
        .links
        .verification_callback(redirect, VerificationResult::Accepted)
        .map_err(|_| crate::mfa_login::failure(error("invalid_redirect"), true))?;
    email_verification::complete(control, runtime.verification, token)?;
    Ok(destination)
}
