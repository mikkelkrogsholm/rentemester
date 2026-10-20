//! Interactive CVR actions share the admitted company owner and current fence.
use super::*;
impl Owner {
    pub fn cvr(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        action: rentemester_application::cvr::Action<'_>,
        lookup: &mut dyn rentemester_runtime::contact_import::Lookup,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, g, context| {
            rentemester_application::cvr::execute(c, l, g, context, action, lookup)
        })
    }
}
