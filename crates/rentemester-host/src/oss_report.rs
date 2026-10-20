//! Current-authorized OSS base report through the owned namespace.
use super::*;
use rentemester_engine::fiscal_year::Date;
use serde_json::Value;

impl Owner {
    pub fn oss_report(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        from: Date,
        to: Date,
    ) -> Result<
        Attempt<Result<Committed<Value>, rentemester_application::control_fence::Failure>>,
        OwnerError,
    > {
        self.document_action(cookie, clock, |control, company, guard, context| {
            rentemester_application::oss_report::report(control, company, guard, context, from, to)
        })
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests;
