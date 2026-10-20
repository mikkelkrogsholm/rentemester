//! Fixed lookup phases release the Owner and SQL before any VIES network call.
use super::*;
use rentemester_application::eu_sales::{self as app, Admission};
use rentemester_engine::fiscal_year::Date;
use serde_json::Value;
type ResultAttempt<T> = Result<
    Attempt<Result<Committed<T>, rentemester_application::control_fence::Failure>>,
    OwnerError,
>;
impl Owner {
    pub fn start_vies(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        vat: &str,
    ) -> ResultAttempt<Admission> {
        self.document_action(cookie, clock, |control, company, _, context| {
            app::start(control, company, context, vat)
        })
    }
    pub fn finish_vies(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        admission: Admission,
        observation: &Value,
    ) -> ResultAttempt<Value> {
        self.document_action(cookie, clock, |control, company, _, context| {
            app::finish(control, company, context, admission, observation)
        })
    }
    pub fn eu_sales(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        from: Date,
        to: Date,
    ) -> ResultAttempt<Value> {
        self.document_action(cookie, clock, |control, company, guard, context| {
            app::report(control, company, guard, context, from, to)
        })
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests;
