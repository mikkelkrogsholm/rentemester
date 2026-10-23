//! Shared owner entrypoint, with normal physical admission and current authority.
use super::*;
impl Owner {
    pub fn compliance(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        as_of: &str,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |control, company, _, context| {
            rentemester_application::compliance::execute(control, company, context, as_of)
        })
    }
}
