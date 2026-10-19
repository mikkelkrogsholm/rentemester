//! Exact owned context, source namespace and continuous mutex admission.
use super::*;
impl Owner {
    pub fn scheduled_recurring(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        action: rentemester_application::scheduled_recurring::Action<'_>,
        today: &str,
        now: &str,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, g, context| {
            rentemester_application::scheduled_recurring::execute(
                c, l, g, context, action, today, now,
            )
        })
    }
}
