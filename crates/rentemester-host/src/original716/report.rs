//! Source report uses the ordinary physical owner and document admission.
use crate::*;
impl Owner {
    pub fn original716_compliance(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        input: &serde_json::Value,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |control, company, _, context| {
            rentemester_application::original716::report::execute(control, company, context, input)
        })
    }
}
impl Owner {
    pub fn original716_remaining(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        operation: &str,
        input: &serde_json::Value,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |control, company, _, context| {
            rentemester_application::original716::report::remaining(
                control, company, context, operation, input,
            )
        })
    }
}
