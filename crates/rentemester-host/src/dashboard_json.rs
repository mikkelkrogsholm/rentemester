//! Dashboard JSON remains inside the admitted owner and current-human read fence.
use super::*;
impl Owner {
    pub fn dashboard_json(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        as_of: &str,
        today: &str,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |control, company, guard, context| {
            rentemester_application::dashboard_json::read(
                control, company, guard, context, as_of, today,
            )
        })
    }
}
