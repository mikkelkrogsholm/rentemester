//! Fixed original-document operations over the common Owner.
use super::*;
use rentemester_application::{
    documents::{Admission, OriginalOptions, ScanPolicy},
    documents_original::{self as app, Action},
};
use rentemester_engine::document::Metadata;
use rentemester_runtime::document::Prepared;
impl Owner {
    pub fn documents_original(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        action: Action,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |control, company, guard, context| {
            app::execute(control, company, guard, context, action)
        })
    }
    pub fn documents_original_service(
        &mut self,
        key: &ServiceKey,
        clock: &dyn Clock,
        action: Action,
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
            app::execute_service(control, company, guard, context, action)
        })
    }
    pub fn prepare_documents_original(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        file: Prepared,
        metadata: Metadata,
        policy: ScanPolicy,
        options: OriginalOptions,
    ) -> Result<
        Attempt<Result<Committed<Admission>, rentemester_application::control_fence::Failure>>,
        OwnerError,
    > {
        self.document_action(cookie, clock, |control, company, _guard, context| {
            rentemester_application::documents::prepare_original_ingest(
                control, company, context, file, metadata, policy, options,
            )
        })
    }
    pub fn prepare_documents_original_service(
        &mut self,
        key: &ServiceKey,
        clock: &dyn Clock,
        file: Prepared,
        metadata: Metadata,
        policy: ScanPolicy,
        options: OriginalOptions,
    ) -> Result<
        Attempt<Result<service_company::ServiceResult<Admission>, service_company::ServiceFailure>>,
        OwnerError,
    > {
        self.service_document_action(key, clock, |control, company, _guard, context| {
            rentemester_application::documents::prepare_original_service(
                control, company, context, file, metadata, policy, options,
            )
        })
    }
}
