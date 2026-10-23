//! Fixed original supplier commitments on the admitted current Owner namespace.
use super::*;
use rentemester_application::supplier_commitment_original::{self as app, Operation, Request};
impl Owner {
    pub fn original_supplier_commitment(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        request: Request<'_>,
        operation: Operation,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |control, company, guard, context| {
            app::execute(control, company, guard, context, request, operation)
        })
    }
    pub fn original_supplier_commitment_service(
        &mut self,
        key: &ServiceKey,
        clock: &dyn Clock,
        request: Request<'_>,
        operation: Operation,
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
            app::execute_service(control, company, guard, context, request, operation)
        })
    }
}
