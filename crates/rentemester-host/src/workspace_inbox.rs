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
        let original = self.slug.clone();
        let observe = match &action {
            rentemester_application::workspace_inbox::Action::Assign(_, target) => {
                Some(target.company_slug.as_str())
            }
            rentemester_application::workspace_inbox::Action::Ingest(..) => Some(anchor),
            _ => None,
        };
        if let Some(target) = observe {
            self.select_company(target)?;
        }
        let mut attempt = self.document_action(cookie, clock, |c, l, g, context| {
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
        })?;
        if self.reusable && self.slug != original && self.select_company(&original).is_err() {
            self.reusable = false;
            attempt.retired = true;
            attempt.outcome = Err(rentemester_application::control_fence::Failure {
                stage: rentemester_application::control_fence::FailureStage::OutcomeUnknown,
                error: rentemester_engine::Error::new(
                    "company_selection_unavailable",
                    "Inbox result requires reconciliation after company restoration failure",
                ),
                control_reusable: false,
                company_reusable: false,
            });
        }
        Ok(attempt)
    }
}
#[cfg(all(test, target_os = "macos"))]
mod tests;

impl Owner {
    pub fn workspace_inbox_original(
        &mut self,
        cookies: &rentemester_runtime::auth::CookieVerifier,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        anchor: &str,
        id: &str,
    ) -> Result<
        Attempt<
            Result<
                Committed<rentemester_application::workspace_inbox::Original>,
                rentemester_application::control_fence::Failure,
            >,
        >,
        OwnerError,
    > {
        let catalog = self.catalog()?;
        self.document_action(cookie, clock, |control, _, _, context| {
            rentemester_application::workspace_inbox::original(
                control,
                context,
                rentemester_application::workspace_inbox::Runtime {
                    cookies,
                    catalog: &catalog,
                    anchor,
                },
                id,
            )
        })
    }
}
