//! Fixed ordinary report use cases. Source selection never escapes the admitted company read.
use super::*;
use rentemester_application::archive_reports::{self as app, Action, Request};
impl Owner {
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
