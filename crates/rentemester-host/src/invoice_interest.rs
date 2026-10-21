//! Current human interest commands through the protected company namespace.
use super::*;
impl Owner {
    pub fn invoice_interest(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        action: rentemester_application::invoice_interest::Action<'_>,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |control, company, guard, context| {
            rentemester_application::invoice_interest::execute(
                control, company, guard, context, action,
            )
        })
    }
}
