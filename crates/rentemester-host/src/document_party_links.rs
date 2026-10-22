//! Fixed selected-company party-link operations, continuously owned through original verification.
use super::*;
impl Owner {
    pub fn document_party_links(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        action: rentemester_application::document_party_links::Action<'_>,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, g, context| {
            rentemester_application::document_party_links::execute(c, l, g, context, action)
        })
    }
    pub fn document_party_links_service(
        &mut self,
        key: &ServiceKey,
        clock: &dyn Clock,
        action: rentemester_application::document_party_links::Action<'_>,
    ) -> Result<
        Attempt<
            Result<
                service_company::ServiceResult<serde_json::Value>,
                service_company::ServiceFailure,
            >,
        >,
        OwnerError,
    > {
        self.service_document_action(key, clock, |c, l, g, context| {
            rentemester_application::document_party_links::execute_service(c, l, g, context, action)
        })
    }
}
