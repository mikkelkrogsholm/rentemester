//! Fixed source-bound correction on the admitted Owner. No public callbacks or path reopeners.
use crate::*;
impl Owner {
    pub fn direct_bank_payable_correction(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        action: rentemester_application::direct_bank_payable_correction::Action<'_>,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, g, context| {
            rentemester_application::direct_bank_payable_correction::execute(
                c, l, g, context, action,
            )
        })
    }
}
#[cfg(all(test, target_os = "macos"))]
mod tests;
