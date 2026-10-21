//! Current human bad-debt writeoff commands through the protected company namespace.
use super::*;
impl Owner {
    pub fn invoice_bad_debt(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        input: &rentemester_storage::invoice_bad_debt::WriteOff,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |control, company, guard, context| {
            rentemester_application::invoice_bad_debt::write_off(
                control, company, guard, context, input,
            )
        })
    }
}
