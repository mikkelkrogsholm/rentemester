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

impl Owner {
    pub(super) fn service_document_action<T>(
        &mut self,
        key: &ServiceKey,
        clock: &dyn Clock,
        apply: impl FnOnce(
            &mut rusqlite::Connection,
            &mut rusqlite::Connection,
            &mut rentemester_runtime::namespace::NamespaceGuard,
            service_company::Context<'_>,
        )
            -> Result<service_company::ServiceResult<T>, service_company::ServiceFailure>,
    ) -> Result<
        Attempt<Result<service_company::ServiceResult<T>, service_company::ServiceFailure>>,
        OwnerError,
    > {
        self.admit()?;
        let slots = self.held.0.as_mut().ok_or(OwnerError::Retired)?;
        let outcome = apply(
            slots.control.as_mut().ok_or(OwnerError::Retired)?,
            slots.company.as_mut().ok_or(OwnerError::Retired)?,
            &mut slots.guard,
            service_company::Context {
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
        );
        let reusable = outcome.as_ref().map_or_else(
            |f| f.control_reusable && f.company_reusable,
            |v| v.control_reusable,
        );
        let retired = self.finish(reusable);
        Ok(Attempt { outcome, retired })
    }
    pub fn prepare_document_ingest_service(
        &mut self,
        key: &ServiceKey,
        clock: &dyn Clock,
        file: Prepared,
        metadata: Metadata,
        policy: ScanPolicy,
    ) -> Result<
        Attempt<Result<service_company::ServiceResult<Admission>, service_company::ServiceFailure>>,
        OwnerError,
    > {
        self.service_document_action(key, clock, |control, company, _, context| {
            rentemester_application::documents::prepare_service(
                control, company, context, file, metadata, policy,
            )
        })
    }
    pub fn finish_document_ingest_service(
        &mut self,
        key: &ServiceKey,
        clock: &dyn Clock,
        admission: Admission,
        scan: Option<ScanEvidence>,
    ) -> Result<
        Attempt<
            Result<
                service_company::ServiceResult<serde_json::Value>,
                service_company::ServiceFailure,
            >,
        >,
        OwnerError,
    > {
        self.service_document_action(key, clock, |control, company, guard, context| {
            rentemester_application::documents::finish_service(
                control, company, guard, context, admission, scan,
            )
        })
    }
    pub fn documents_service(
        &mut self,
        key: &ServiceKey,
        clock: &dyn Clock,
    ) -> Result<
        Attempt<
            Result<
                service_company::ServiceResult<serde_json::Value>,
                service_company::ServiceFailure,
            >,
        >,
        OwnerError,
    > {
        self.service_document_action(key, clock, |control, company, _, context| {
            rentemester_application::documents::list_service(control, company, context)
        })
    }
    pub fn document_original_service(
        &mut self,
        key: &ServiceKey,
        clock: &dyn Clock,
        id: i64,
    ) -> Result<
        Attempt<
            Result<
                service_company::ServiceResult<rentemester_application::documents::Original>,
                service_company::ServiceFailure,
            >,
        >,
        OwnerError,
    > {
        self.service_document_action(key, clock, |control, company, guard, context| {
            rentemester_application::documents::read_service(control, company, guard, context, id)
        })
    }
}
