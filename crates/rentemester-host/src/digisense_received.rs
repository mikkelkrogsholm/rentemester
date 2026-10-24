//! Fixed forwarding commands. No HTTP, provider calls or user-decoded admissions.
use super::*;
use rentemester_application::{control_fence::Failure, digisense_received as intake};
use serde_json::Value;
/// Bound to this exact live Owner, including across selection/reopen attempts.
pub struct Admission {
    epoch: std::sync::Arc<()>,
    inner: intake::Admission,
}
impl Admission {
    pub fn company_key(&self) -> &str {
        self.inner.company_key()
    }
    pub fn environment(&self) -> &str {
        self.inner.environment()
    }
}
pub struct Checked {
    pub admission: Admission,
    pub known: bool,
}
pub struct Finished {
    pub admission: Admission,
    pub document: Value,
}
pub type ReceiveAttempt<T> = Result<Attempt<Result<Committed<T>, Failure>>, OwnerError>;
fn wrap<T, U>(
    attempt: Attempt<Result<Committed<T>, Failure>>,
    f: impl FnOnce(T) -> U,
) -> Attempt<Result<Committed<U>, Failure>> {
    Attempt {
        retired: attempt.retired,
        outcome: attempt.outcome.map(|r| Committed {
            value: f(r.value),
            control_reusable: r.control_reusable,
        }),
    }
}
impl Owner {
    pub fn preflight_digisense_received(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
    ) -> ReceiveAttempt<()> {
        self.document_action(cookie, clock, intake::preflight)
    }
    /// Current catalog, without a caller-supplied filesystem/workspace selector.
    pub fn digisense_received_catalog(
        &mut self,
        cookies: &rentemester_runtime::auth::CookieVerifier,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
    ) -> AuthAttempt<Value> {
        let assertion = self.workspace.clone();
        self.recurring_workspace_catalog(cookies, cookie, clock, &assertion)
    }

    /// Environment and generation are trusted live operator-client values, not request fields.
    pub fn prepare_digisense_received(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        environment: &str,
        generation: [u8; 32],
        company_key_assertion: Option<&str>,
    ) -> ReceiveAttempt<Admission> {
        let epoch = self.received_epoch.clone();
        self.document_action(cookie, clock, |c, l, g, context| {
            intake::prepare(
                c,
                l,
                g,
                context,
                environment,
                generation,
                company_key_assertion,
            )
        })
        .map(|a| wrap(a, |inner| Admission { epoch, inner }))
    }
    pub fn check_digisense_received(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        admission: Admission,
        environment: &str,
        generation: [u8; 32],
        record: Value,
    ) -> ReceiveAttempt<Checked> {
        if !std::sync::Arc::ptr_eq(&self.received_epoch, &admission.epoch) {
            return Err(OwnerError::Context);
        }
        let epoch = admission.epoch;
        self.document_action(cookie, clock, |c, l, g, context| {
            intake::check(
                c,
                l,
                g,
                context,
                admission.inner,
                environment,
                generation,
                record,
            )
        })
        .map(|a| {
            wrap(a, |v| Checked {
                admission: Admission {
                    epoch,
                    inner: v.admission,
                },
                known: v.known,
            })
        })
    }
    #[allow(clippy::too_many_arguments)]
    pub fn finish_digisense_received(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        admission: Admission,
        environment: &str,
        generation: [u8; 32],
        xml: Vec<u8>,
        overrides: Value,
        force_duplicate_logical_identity: bool,
    ) -> ReceiveAttempt<Finished> {
        if !std::sync::Arc::ptr_eq(&self.received_epoch, &admission.epoch) {
            return Err(OwnerError::Context);
        }
        let epoch = admission.epoch;
        self.document_action(cookie, clock, |c, l, g, context| {
            intake::finish(
                c,
                l,
                g,
                context,
                admission.inner,
                environment,
                generation,
                xml,
                overrides,
                force_duplicate_logical_identity,
            )
        })
        .map(|a| {
            wrap(a, |v| Finished {
                admission: Admission {
                    epoch,
                    inner: v.admission,
                },
                document: v.document,
            })
        })
    }
}
#[cfg(all(test, target_os = "macos"))]
mod tests;
