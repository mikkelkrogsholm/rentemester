//! No worker callback or process handle enters the Owner. Drop its external
//! mutex between preparation and final publication; the physical lease remains.
use super::*;
use rentemester_application::document_pdf::{self as app, Admission, Completion, Read};
use rentemester_engine::document_pdf::ParserIdentity;
impl Owner {
    pub fn prepare_document_pdf(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        document_id: i64,
        identity: ParserIdentity,
    ) -> Result<
        Attempt<Result<Committed<Admission>, rentemester_application::control_fence::Failure>>,
        OwnerError,
    > {
        self.document_action(cookie, clock, |control, company, guard, context| {
            app::prepare(control, company, guard, context, document_id, identity)
        })
    }
    pub fn finish_document_pdf(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        admission: Admission,
        completion: Option<Completion>,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |control, company, guard, context| {
            app::finish(control, company, guard, context, admission, completion)
        })
    }
    pub fn document_pdf(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        action: Read,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |control, company, guard, context| {
            app::read(control, company, guard, context, action)
        })
    }
}
#[cfg(all(test, target_os = "macos"))]
mod tests;
impl Owner {
    pub fn prepare_document_pdf_service(
        &mut self,
        key: &ServiceKey,
        clock: &dyn Clock,
        id: i64,
        identity: ParserIdentity,
    ) -> Result<
        Attempt<Result<service_company::ServiceResult<Admission>, service_company::ServiceFailure>>,
        OwnerError,
    > {
        self.service_document_action(key, clock, |control, company, guard, context| {
            app::prepare_service(control, company, guard, context, id, identity)
        })
    }
    pub fn finish_document_pdf_service(
        &mut self,
        key: &ServiceKey,
        clock: &dyn Clock,
        admission: Admission,
        completion: Option<Completion>,
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
            app::finish_service(control, company, guard, context, admission, completion)
        })
    }
    pub fn document_pdf_service(
        &mut self,
        key: &ServiceKey,
        clock: &dyn Clock,
        action: Read,
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
            app::read_service(control, company, guard, context, action)
        })
    }
}
