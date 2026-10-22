//! Original accruals reuse the admitted Owner and physical namespace.
use super::*;
use rentemester_application::accrual_original::{self as app, Action};
impl Owner {
    pub fn original_accrual(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        action: Action<'_>,
        expected: Option<&str>,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |control, company, guard, context| {
            app::execute(control, company, guard, context, action, expected)
        })
    }
    pub fn original_accrual_service(
        &mut self,
        key: &ServiceKey,
        clock: &dyn Clock,
        action: Action<'_>,
        expected: Option<&str>,
    ) -> Result<
        Attempt<
            Result<
                service_company::ServiceResult<serde_json::Value>,
                service_company::ServiceFailure,
            >,
        >,
        OwnerError,
    > {
        self.service_document_action(key, clock, |control, company, guard, context| {
            app::execute_service(control, company, guard, context, action, expected)
        })
    }
}
