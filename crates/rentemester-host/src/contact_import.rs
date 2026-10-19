//! Fixed current-company import. Trusted narrow CVR port; no public SQL/path callback.
use super::*;
use rentemester_runtime::contact_import::Lookup;
impl Owner {
    pub fn import_contacts(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        input: &rentemester_engine::contact_import::Input,
        lookup: &mut dyn Lookup,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, g, context| {
            rentemester_application::contact_import::execute(c, l, g, context, input, lookup)
        })
    }
}
#[cfg(all(test, target_os = "macos"))]
mod tests;
