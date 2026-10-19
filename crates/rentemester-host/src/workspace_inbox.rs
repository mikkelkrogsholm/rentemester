use super::*;
impl Owner {
    pub fn workspace_inbox(
        &mut self,
        cookies: &rentemester_runtime::auth::CookieVerifier,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        anchor: &str,
        action: rentemester_application::workspace_inbox::Action<'_>,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        let catalog = self.catalog()?;
        self.document_action(cookie, clock, |c, l, g, context| {
            rentemester_application::workspace_inbox::execute(
                c,
                l,
                g,
                context,
                rentemester_application::workspace_inbox::Runtime {
                    cookies,
                    catalog: &catalog,
                    anchor,
                },
                action,
            )
        })
    }
}
#[cfg(all(test, target_os = "macos"))]
mod tests;
