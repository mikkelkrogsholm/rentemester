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
}
