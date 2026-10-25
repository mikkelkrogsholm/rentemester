//! Fixed original diagnostic operations on the current admitted Owner.
use super::*;
use rentemester_application::diagnostics_original118 as app;
use serde_json::Value;
impl Owner {
    pub fn retention_status_original118(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        as_of: Option<&str>,
    ) -> Result<
        Attempt<Result<Committed<Value>, rentemester_application::control_fence::Failure>>,
        OwnerError,
    > {
        let today = diagnostic_date(clock, as_of)?;
        self.document_action(cookie, clock, |c, l, _, ctx| {
            app::retention_human(c, l, ctx, &today)
        })
    }
    pub fn retention_status_original118_service(
        &mut self,
        key: &ServiceKey,
        clock: &dyn Clock,
        as_of: Option<&str>,
    ) -> Result<
        Attempt<Result<service_company::ServiceResult<Value>, service_company::ServiceFailure>>,
        OwnerError,
    > {
        let today = diagnostic_date(clock, as_of)?;
        self.service_document_action(key, clock, |c, l, _, ctx| {
            app::retention_service(c, l, ctx, &today)
        })
    }
    pub fn system_healthcheck_original118(
        &mut self,
        cookies: &rentemester_runtime::auth::CookieVerifier,
        proof: &VerifiedCookie,
        clock: &dyn Clock,
        slug: &str,
    ) -> AuthAttempt<Value> {
        let catalog = self.catalog()?;
        let root = self.root.clone();
        self.control_auth(cookies.workspace(), |c| {
            app::health(
                c,
                clock,
                app::Credential::Human(cookies, proof),
                &catalog,
                slug,
                |registered| {
                    rentemester_storage::healthcheck::inspect_company(&root.join(registered))
                },
            )
        })
    }
    pub fn system_healthcheck_original118_service(
        &mut self,
        key: &ServiceKey,
        clock: &dyn Clock,
        slug: &str,
    ) -> AuthAttempt<Value> {
        let catalog = self.catalog()?;
        let root = self.root.clone();
        self.control_auth(key.workspace(), |c| {
            app::health(
                c,
                clock,
                app::Credential::Service(key),
                &catalog,
                slug,
                |registered| {
                    rentemester_storage::healthcheck::inspect_company(&root.join(registered))
                },
            )
        })
    }
}
fn diagnostic_date(clock: &dyn Clock, explicit: Option<&str>) -> Result<String, OwnerError> {
    match explicit {
        Some(date) => Ok(date.to_owned()),
        None => rentemester_runtime::posting_time::latest_date(
            clock.now_unix_ms().map_err(|_| OwnerError::Context)?,
            0,
        )
        .map_err(|_| OwnerError::Context),
    }
}
#[cfg(test)]
#[path = "diagnostics_original118_tests.rs"]
mod tests;
