//! Fixed two-phase document admission. Network I/O belongs between these calls,
//! after the adapter has dropped its Owner mutex guard, not inside a callback.
use super::*;
use rentemester_application::documents::{Admission, RegisteredIngest, ScanPolicy};
use rentemester_engine::{document::Metadata, document_scan::ScanEvidence};
use rentemester_runtime::document::Prepared;
impl Owner {
    pub fn prepare_document_ingest(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        file: Prepared,
        metadata: Metadata,
        policy: ScanPolicy,
    ) -> Result<
        Attempt<Result<Committed<Admission>, rentemester_application::control_fence::Failure>>,
        OwnerError,
    > {
        self.document_action(cookie, clock, |control, company, _, context| {
            rentemester_application::documents::prepare_ingest(
                control, company, context, file, metadata, policy,
            )
        })
    }
    pub fn finish_document_ingest_registered(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        admission: Admission,
        scan: Option<ScanEvidence>,
    ) -> Result<
        Attempt<
            Result<Committed<RegisteredIngest>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |control, company, guard, context| {
            rentemester_application::documents::finish_ingest_registered(
                control, company, guard, context, admission, scan,
            )
        })
    }
    pub fn finish_document_ingest(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        admission: Admission,
        scan: Option<ScanEvidence>,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |control, company, guard, context| {
            rentemester_application::documents::finish_ingest(
                control, company, guard, context, admission, scan,
            )
        })
    }
}
#[cfg(all(test, target_os = "macos"))]
mod tests;
