//! Fixed admitted extraction operations. The caller releases its Owner mutex
//! between prepare and finish; no provider callback enters the Owner.
use super::*;
use rentemester_application::invoice_extraction::{self as app, Admission, Completion};
use rentemester_engine::invoice_extraction::ProviderIdentity;
impl Owner {
    pub fn prepare_invoice_extraction(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        id: i64,
        identity: Option<ProviderIdentity>,
    ) -> Result<
        Attempt<Result<Committed<Admission>, rentemester_application::control_fence::Failure>>,
        OwnerError,
    > {
        self.document_action(cookie, clock, |control, company, guard, context| {
            app::prepare(control, company, guard, context, id, identity)
        })
    }
    pub fn finish_invoice_extraction(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        admission: Admission,
        completion: Option<Completion>,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |control, company, guard, context| {
            app::finish(control, company, guard, context, admission, completion)
        })
    }
    pub fn invoice_extraction(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        id: i64,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |control, company, guard, context| {
            app::read(control, company, guard, context, id)
        })
    }
}
#[cfg(all(test, target_os = "macos"))]
mod tests;
