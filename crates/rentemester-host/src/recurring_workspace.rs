//! Workspace-runner phases retain normal admitted owner, cookie and transaction fences.
use super::*;
use serde_json::{Value, json};
impl Owner {
    pub fn recurring_workspace(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        action: rentemester_application::recurring_workspace::Action<'_>,
        today: &str,
        now: &str,
    ) -> Result<
        Attempt<Result<Committed<Value>, rentemester_application::control_fence::Failure>>,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, g, context| {
            rentemester_application::recurring_workspace::execute(
                c, l, g, context, action, today, now,
            )
        })
    }
    pub fn recurring_workspace_catalog(
        &mut self,
        cookies: &rentemester_runtime::auth::CookieVerifier,
        proof: &VerifiedCookie,
        clock: &dyn Clock,
        assertion: &str,
    ) -> AuthAttempt<Value> {
        use rentemester_application::password_login::{Failure, FailureStage};
        let catalog = self.catalog()?;
        let context = self.workspace_context(cookies, clock, proof, &catalog)?;
        let failure = || Failure {
            stage: FailureStage::NotCommitted,
            error: rentemester_engine::Error::new(
                "access_denied",
                "current workspace owner and exact configured workspace assertion required",
            ),
            connection_reusable: true,
        };
        let value = match context.outcome {
            Ok(c) => serde_json::from_str::<Value>(c.as_json()).map_err(|_| OwnerError::Context)?,
            Err(f) => {
                return Ok(Attempt {
                    outcome: Err(f),
                    retired: context.retired,
                });
            }
        };
        if context.retired {
            return Ok(Attempt {
                outcome: Err(failure()),
                retired: true,
            });
        }
        if value["workspaceRole"] != "workspace_owner"
            || (assertion != self.workspace && Some(assertion) != self.root.to_str())
        {
            return Ok(Attempt {
                outcome: Err(failure()),
                retired: false,
            });
        }
        self.admit()?;
        let result = (|| {
            let mut companies = Vec::new();
            for company in catalog.companies.into_iter().filter(|c| c.purpose == rentemester_engine::workspace_manifest::Purpose::Live) {
                let initialized = if company.archived {
                    false
                } else {
                    self.recurring_company_initialized(&company.slug)?
                };
                companies.push(json!({"slug":company.slug,"name":company.name,"archived":company.archived,"initialized":initialized}));
            }
            Ok(json!({"workspace":catalog.workspace,"companies":companies}))
        })();
        let retired = self.finish(result.is_ok());
        result.map(|v| Attempt {
            outcome: Ok(v),
            retired,
        })
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests;
