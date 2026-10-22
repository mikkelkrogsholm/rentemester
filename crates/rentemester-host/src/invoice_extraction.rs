//! Fixed admitted extraction operations. The caller releases its Owner mutex
//! between prepare and finish; no provider callback enters the Owner.
use super::*;
use rentemester_application::invoice_extraction::{self as app, Admission, Completion};
use rentemester_engine::invoice_extraction::ProviderIdentity;
impl Owner {
    pub fn prepare_uploaded_invoice_extraction(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        upload: rentemester_application::documents::RegisteredUpload,
        identity: Option<ProviderIdentity>,
    ) -> Result<
        Attempt<Result<Committed<Admission>, rentemester_application::control_fence::Failure>>,
        OwnerError,
    > {
        self.document_action(cookie, clock, |control, company, guard, context| {
            app::prepare_registered_upload(control, company, guard, context, upload, identity)
        })
    }
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
/// The retained BLOB must agree with the physical source, not just its SQL hash.
pub(crate) fn validate_live_sources(
    db: &Connection,
    guard: &mut NamespaceGuard,
) -> Result<(), rentemester_engine::Error> {
    for source in rentemester_storage::invoice_extraction::sources(db)? {
        let path: String = db
            .query_row(
                "SELECT stored_path FROM documents WHERE id=?1",
                [source.document_id],
                |r| r.get(0),
            )
            .map_err(|_| source_unavailable())?;
        let bytes = guard
            .read_document(&path, &source.sha256)
            .map_err(|_| source_unavailable())?;
        if bytes != source.pdf_bytes {
            return Err(source_unavailable());
        }
    }
    Ok(())
}
pub(crate) fn validate_archive_sources(
    db: &Connection,
    artifact: &rentemester_runtime::backup_artifact::VerifiedArtifact,
    slug: &str,
) -> Result<(), rentemester_engine::Error> {
    use rentemester_runtime::backup_artifact::EntryKind;
    for source in rentemester_storage::invoice_extraction::sources(db)? {
        let path: String = db
            .query_row(
                "SELECT stored_path FROM documents WHERE id=?1",
                [source.document_id],
                |r| r.get(0),
            )
            .map_err(|_| source_unavailable())?;
        let full = format!("{slug}/{path}");
        let entry = artifact.entries().iter().find(|entry| {
            entry.path() == full && matches!(entry.kind(), EntryKind::DocumentOriginal{company} if company == slug)
        }).ok_or_else(source_unavailable)?;
        if entry.bytes() != source.pdf_bytes {
            return Err(source_unavailable());
        }
    }
    Ok(())
}
fn source_unavailable() -> rentemester_engine::Error {
    rentemester_engine::Error::new(
        "EXTRACTION_SNAPSHOT_UNAVAILABLE",
        "Invoice extraction original differs or is unavailable",
    )
}
#[cfg(all(test, target_os = "macos"))]
mod tests;

impl Owner {
    pub fn prepare_original_invoice_extraction(&mut self, cookie: &VerifiedCookie, clock: &dyn Clock, id: i64, identity: Option<ProviderIdentity>, expected_actor: Option<&str>) -> Result<Attempt<Result<Committed<Admission>, rentemester_application::control_fence::Failure>>, OwnerError> {
        self.document_action(cookie, clock, |control, company, guard, context| { app::prepare_original(control, company, guard, context, id, identity, expected_actor) })
    }
    pub fn original_invoice_extraction(&mut self, cookie: &VerifiedCookie, clock: &dyn Clock, id: i64) -> Result<Attempt<Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>>, OwnerError> {
        self.document_action(cookie, clock, |control, company, guard, context| { app::read_original(control, company, guard, context, id) })
    }
    pub fn prepare_invoice_extraction_service(&mut self, key: &ServiceKey, clock: &dyn Clock, id: i64, identity: Option<ProviderIdentity>, expected_actor: Option<&str>) -> Result<Attempt<Result<service_company::ServiceResult<Admission>, service_company::ServiceFailure>>, OwnerError> {
        self.service_document_action(key, clock, |control, company, guard, context| { app::prepare_service(control, company, guard, context, id, identity, expected_actor) })
    }
    pub fn finish_invoice_extraction_service(&mut self, key: &ServiceKey, clock: &dyn Clock, admission: Admission, completion: Option<Completion>) -> Result<Attempt<Result<service_company::ServiceResult<serde_json::Value>, service_company::ServiceFailure>>, OwnerError> {
        self.service_document_action(key, clock, |control, company, guard, context| { app::finish_service(control, company, guard, context, admission, completion) })
    }
    pub fn invoice_extraction_service(&mut self, key: &ServiceKey, clock: &dyn Clock, id: i64) -> Result<Attempt<Result<service_company::ServiceResult<serde_json::Value>, service_company::ServiceFailure>>, OwnerError> {
        self.service_document_action(key, clock, |control, company, guard, context| { app::read_service(control, company, guard, context, id) })
    }
}
