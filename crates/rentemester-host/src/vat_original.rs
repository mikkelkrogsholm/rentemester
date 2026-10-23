//! Source VAT reads use the same admitted, owned physical namespace.
use super::*;
use rentemester_application::vat_original::{self as app, Operation};
use serde_json::Value;

impl Owner {
    pub fn original_vat(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        operation: Operation,
        parse: impl FnOnce() -> Result<(String, String), rentemester_engine::Error>,
    ) -> Result<
        Attempt<Result<Committed<Value>, rentemester_application::control_fence::Failure>>,
        OwnerError,
    > {
        self.document_action(cookie, clock, |control, company, guard, context| {
            app::execute(control, company, guard, context, operation, parse)
        })
    }
    pub fn original_vat_service(
        &mut self,
        key: &ServiceKey,
        clock: &dyn Clock,
        operation: Operation,
        parse: impl FnOnce() -> Result<(String, String), rentemester_engine::Error>,
    ) -> Result<
        Attempt<Result<service_company::ServiceResult<Value>, service_company::ServiceFailure>>,
        OwnerError,
    > {
        self.service_document_action(key, clock, |control, company, guard, context| {
            app::execute_service(control, company, guard, context, operation, parse)
        })
    }
}
