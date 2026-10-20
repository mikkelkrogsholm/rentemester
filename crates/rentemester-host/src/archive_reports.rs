//! Fixed ordinary report use cases. Source selection never escapes the admitted company read.
use super::*;
use rentemester_application::archive_reports::{self as app, Action, Request};
impl Owner {
    pub fn archive_reports_service(
        &mut self,
        key: &ServiceKey,
        clock: &dyn Clock,
        action: Action<'_>,
    ) -> Result<
        Attempt<
            Result<
                rentemester_application::service_company::ServiceResult,
                rentemester_application::service_company::ServiceFailure,
            >,
        >,
        OwnerError,
    > {
        self.admit()?;
        let slots = self.held.0.as_mut().unwrap();
        let outcome = app::execute_service(
            slots.control.as_mut().unwrap(),
            slots.company.as_mut().unwrap(),
            &mut slots.guard,
            rentemester_application::service_company::Context {
                key,
                clock,
                max_future_days: 45,
                selection: CompanySelection {
                    workspace: &self.workspace,
                    slug: &self.slug,
                    registered: true,
                    archived: false,
                },
            },
            action,
        );
        let reusable = outcome.as_ref().map_or_else(
            |f| f.control_reusable && f.company_reusable,
            |v| v.control_reusable,
        );
        let retired = self.finish(reusable);
        Ok(Attempt { outcome, retired })
    }
    pub fn archive_reports(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        request: Request<'_>,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, g, context| {
            app::execute(c, l, g, context, Action::Read(request))
        })
    }
    pub fn archive_report_years(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        today: &str,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, g, context| {
            app::execute(c, l, g, context, Action::FiscalYears { today })
        })
    }
}
#[cfg(all(test, target_os = "macos"))]
mod tests;
