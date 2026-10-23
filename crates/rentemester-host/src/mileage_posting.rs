//! Mileage accounting uses the existing company owner and guarded namespace.
use super::*;
impl Owner {
    pub fn mileage_posting(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        action: rentemester_application::mileage_posting::Action<'_>,
        today: &str,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |control, company, guard, context| {
            rentemester_application::mileage_posting::execute(
                control, company, guard, context, action, today,
            )
        })
    }
}
