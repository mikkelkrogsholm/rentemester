//! Document use cases share the same current-role fence as journal posting.
use crate::{
    access::Permission,
    control_fence::{
        self, Committed, CompanySelection, CookieAuthorization, Failure, FailureStage,
        SessionRequirement,
    },
};
use rentemester_engine::{Error, document::Metadata, document_scan::ScanEvidence};
use rentemester_runtime::{
    auth::{Clock, VerifiedCookie},
    document::Prepared,
    namespace::NamespaceGuard,
};
use rusqlite::Connection;
use serde_json::Value;
fn unavailable() -> Error {
    Error::new(
        "document_evidence_unavailable",
        "document evidence unavailable or integrity mismatch",
    )
}
pub struct Ingest<'a> {
    pub file: &'a Prepared,
    pub metadata: &'a Metadata,
    pub today: &'a str,
}
pub struct Context<'a> {
    pub cookie: &'a VerifiedCookie,
    pub clock: &'a dyn Clock,
    pub selection: CompanySelection<'a>,
}
#[derive(Default)]
pub struct OriginalOptions {
    pub vendor_id: Option<i64>,
    pub force: bool,
    pub original_api: bool,
}
/// Immutable operator policy, never decoded from upload metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanPolicy {
    Off,
    Required,
}
/// One authorized in-memory request. It cannot be reconstructed from HTTP or
/// cloned to change the selected source between scan and publication.
pub struct Admission {
    file: Prepared,
    metadata: Metadata,
    workspace: String,
    slug: String,
    subject: String,
    ledger: String,
    policy: ScanPolicy,
    service_permit: Option<crate::service_company::precharged::Permit>,
    original_options: OriginalOptions,
    vendor_observation: Option<String>,
}
impl Admission {
    pub fn file(&self) -> &Prepared {
        &self.file
    }
    fn validate_binding(
        &self,
        workspace: &str,
        slug: &str,
        subject: &str,
        ledger: &str,
    ) -> Result<(), Error> {
        if workspace != self.workspace
            || slug != self.slug
            || subject != self.subject
            || ledger != self.ledger
        {
            return Err(Error::new(
                "document_admission_changed",
                "document subject, selection or ledger changed during scanning",
            ));
        }
        Ok(())
    }
    fn validate_vendor(&self, tx: &rusqlite::Transaction<'_>) -> Result<(), Error> {
        let mut metadata: Metadata = serde_json::from_value(
            serde_json::to_value(&self.metadata).map_err(|_| unavailable())?,
        )
        .map_err(|_| unavailable())?;
        let current = rentemester_storage::documents_original::vendor_fill(
            tx,
            &mut metadata,
            self.original_options.vendor_id,
        )?;
        if current != self.vendor_observation {
            return Err(Error::new(
                "document_admission_changed",
                "vendor changed during scanning",
            ));
        }
        Ok(())
    }
    fn validate_scan(&self, scan: Option<&ScanEvidence>) -> Result<(), Error> {
        match (self.policy, scan) {
            (ScanPolicy::Off, None) => Ok(()),
            (ScanPolicy::Required, Some(evidence)) => {
                evidence.validate()?;
                if evidence.sha256 != self.file.hash() {
                    return Err(unavailable());
                }
                Ok(())
            }
            _ => Err(Error::new(
                "document_scan_policy_mismatch",
                "document scan evidence does not satisfy the admitted operator policy",
            )),
        }
    }
}
/// Proof of one newly registered upload. Only the known committed registration
/// path can return it; HTTP callers cannot construct or clone this capability.
pub struct RegisteredUpload {
    workspace: String,
    slug: String,
    subject: String,
    ledger: String,
    document_id: i64,
    hash: String,
    eligible: bool,
}
impl RegisteredUpload {
    pub fn eligible_for_extraction(&self) -> bool {
        self.eligible
    }
    pub(crate) fn document_id(&self) -> i64 {
        self.document_id
    }
    pub(crate) fn validate_binding(
        &self,
        workspace: &str,
        slug: &str,
        subject: &str,
        ledger: &str,
        hash: &str,
    ) -> Result<(), Error> {
        if workspace != self.workspace
            || slug != self.slug
            || subject != self.subject
            || ledger != self.ledger
            || hash != self.hash
        {
            return Err(Error::new(
                "EXTRACTION_ADMISSION_CHANGED",
                "Registered upload binding changed",
            ));
        }
        if !self.eligible {
            return Err(Error::new(
                "EXTRACTION_SNAPSHOT_INVALID",
                "Registered upload is not a supported invoice PDF",
            ));
        }
        Ok(())
    }
}
fn extraction_eligible(file: &Prepared) -> bool {
    file.mime() == "application/pdf" && file.bytes().len() <= 10 * 1024 * 1024
}
pub struct RegisteredIngest {
    pub receipt: Value,
    pub upload: RegisteredUpload,
}
fn failed(code: &'static str, reusable: bool) -> Failure {
    Failure {
        stage: FailureStage::NotCommitted,
        error: Error::new(code, "document admission refused before publication"),
        control_reusable: reusable,
        company_reusable: reusable,
    }
}
fn ledger_identity(company: &Connection) -> Result<String, Error> {
    company
        .query_row(
            "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
            [],
            |row| row.get(0),
        )
        .map_err(|_| unavailable())
}
/// Authorize before provider I/O; return with no open SQL transaction or effects.
pub fn prepare_ingest(
    control: &mut Connection,
    company: &mut Connection,
    context: Context<'_>,
    file: Prepared,
    metadata: Metadata,
    policy: ScanPolicy,
) -> Result<Committed<Admission>, Failure> {
    prepare_original_ingest(
        control,
        company,
        context,
        file,
        metadata,
        policy,
        OriginalOptions::default(),
    )
}
#[allow(clippy::too_many_arguments)]
pub fn prepare_original_ingest(
    control: &mut Connection,
    company: &mut Connection,
    context: Context<'_>,
    file: Prepared,
    mut metadata: Metadata,
    policy: ScanPolicy,
    options: OriginalOptions,
) -> Result<Committed<Admission>, Failure> {
    let workspace = context.selection.workspace.to_owned();
    let slug = context.selection.slug.to_owned();
    let prepared = control_fence::execute_cookie_subject(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Current,
        },
        context.selection,
        Permission::CompanyDocumentsUpload,
        |tx, subject| {
            let vendor_observation = rentemester_storage::documents_original::vendor_fill(
                tx,
                &mut metadata,
                options.vendor_id,
            )?;
            metadata.validate()?;
            let ledger = ledger_identity(tx)?;
            Ok(Admission {
                file,
                metadata,
                workspace,
                slug,
                subject: subject.into(),
                ledger,
                policy,
                service_permit: None,
                original_options: options,
                vendor_observation,
            })
        },
    )?;
    if !prepared.control_reusable || !control.is_autocommit() || !company.is_autocommit() {
        return Err(failed("document_admission_unconfirmed", false));
    }
    Ok(prepared)
}
/// Consume exactly the admitted bytes after a fresh authorization and physical
/// observation. The trusted HTTP adapter performs all scanner I/O between calls.
pub fn finish_ingest(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    admission: Admission,
    scan: Option<ScanEvidence>,
) -> Result<Committed<Value>, Failure> {
    finish_ingest_registered(control, company, guard, context, admission, scan).map(|committed| {
        Committed {
            value: committed.value.receipt,
            control_reusable: committed.control_reusable,
        }
    })
}
/// Alternate trusted adapter entry: retain the exact upload receipt and a
/// single-use capability only when registration has a known committed result.
pub fn finish_ingest_registered(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    admission: Admission,
    scan: Option<ScanEvidence>,
) -> Result<Committed<RegisteredIngest>, Failure> {
    if admission.service_permit.is_some() {
        return Err(failed("document_admission_changed", true));
    }
    guard
        .ensure_connections(control, company)
        .map_err(|_| failed("document_namespace_changed", false))?;
    let workspace = context.selection.workspace;
    let slug = context.selection.slug;
    control_fence::execute_cookie_subject(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Current,
        },
        context.selection,
        Permission::CompanyDocumentsUpload,
        |tx, subject| {
            admission.validate_binding(workspace, slug, subject, &ledger_identity(tx)?)?;
            admission.validate_scan(scan.as_ref())?;
            admission.validate_vendor(tx)?;
            admission.metadata.validate()?;
            let today = rentemester_runtime::posting_time::latest_date(
                context.clock.now_unix_ms().map_err(|_| {
                    Error::new("invalid_auth_clock", "trusted document clock unavailable")
                })?,
                0,
            )
            .map_err(|error| Error::new(error.code(), "trusted document date unavailable"))?;
            if admission.original_options.original_api && let Some(message)=rentemester_storage::documents_original::intake_duplicate(tx,admission.file.hash(),&admission.metadata,admission.original_options.force)? {
                let receipt=rentemester_storage::documents_original::blocked(tx,admission.file.filename(),admission.file.hash(),&admission.metadata,&message)?;
                return Ok(RegisteredIngest { receipt, upload:RegisteredUpload {workspace:workspace.into(),slug:slug.into(),subject:subject.into(),ledger:ledger_identity(tx)?,document_id:0,hash:admission.file.hash().into(),eligible:false} });
            }
            let path = guard
                .publish_document(&admission.file)
                .map_err(|_| unavailable())?;
            let receipt = rentemester_storage::documents::register_original(
                tx,
                rentemester_storage::documents::Original {
                    filename: admission.file.filename(),
                    hash: admission.file.hash(),
                    path: &path,
                    mime: admission.file.mime(),
                },
                &admission.metadata,
                subject,
                &today,
                scan.as_ref(),
                rentemester_storage::idempotency::PrincipalKind::User,
                admission.original_options.force,
            )
            .or_else(|error| {
                if admission.original_options.original_api
                    && error.code == "document_refused"
                    && (error.message.starts_with("duplicate document content")
                        || error.message.starts_with("supplier invoice identity"))
                {
                    rentemester_storage::documents_original::blocked(
                        tx,
                        admission.file.filename(),
                        admission.file.hash(),
                        &admission.metadata,
                        &error.message,
                    )
                } else {
                    Err(error)
                }
            })?;
            rentemester_storage::unmatched_bank::sync(tx)?;
            if receipt["ok"] == false {
                return Ok(RegisteredIngest {
                    receipt,
                    upload: RegisteredUpload {
                        workspace: workspace.into(),
                        slug: slug.into(),
                        subject: subject.into(),
                        ledger: ledger_identity(tx)?,
                        document_id: 0,
                        hash: admission.file.hash().into(),
                        eligible: false,
                    },
                });
            }
            let document_id = receipt["document"]["id"]
                .as_i64()
                .filter(|id| *id > 0)
                .ok_or_else(unavailable)?;
            Ok(RegisteredIngest {
                receipt,
                upload: RegisteredUpload {
                    workspace: workspace.into(),
                    slug: slug.into(),
                    subject: subject.into(),
                    ledger: ledger_identity(tx)?,
                    document_id,
                    hash: admission.file.hash().into(),
                    eligible: extraction_eligible(&admission.file),
                },
            })
        },
    )
}
pub fn ingest(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    input: Ingest<'_>,
) -> Result<Committed<Value>, Failure> {
    control_fence::execute_cookie_subject(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Current,
        },
        context.selection,
        Permission::CompanyDocumentsUpload,
        |tx, subject| {
            input.metadata.validate()?;
            // Bytes are durable before registration. Rollback/unknown commit never
            // deletes a possible winner; an unregistered private blob can be retried.
            let path = guard
                .publish_document(input.file)
                .map_err(|_| unavailable())?;
            let receipt = rentemester_storage::documents::register(
                tx,
                rentemester_storage::documents::Original {
                    filename: input.file.filename(),
                    hash: input.file.hash(),
                    path: &path,
                    mime: input.file.mime(),
                },
                input.metadata,
                subject,
                input.today,
            )?;
            rentemester_storage::unmatched_bank::sync(tx)?;
            Ok(receipt)
        },
    )
}
pub struct Original {
    pub bytes: Vec<u8>,
    pub filename: String,
    pub mime: String,
}
pub fn read(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    id: i64,
) -> Result<Committed<Original>, Failure> {
    control_fence::execute_cookie_subject(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Current,
        },
        context.selection,
        Permission::CompanyDocumentsRead,
        |tx, _| {
            let evidence = rentemester_storage::documents::evidence(tx, id)?;
            let bytes = guard
                .read_document(&evidence.path, &evidence.hash)
                .map_err(|_| unavailable())?;
            Ok(Original {
                bytes,
                filename: evidence.filename,
                mime: evidence.mime,
            })
        },
    )
}
pub fn list(
    control: &mut Connection,
    company: &mut Connection,
    context: Context<'_>,
) -> Result<Committed<Value>, Failure> {
    let slug = context.selection.slug;
    control_fence::execute_cookie_subject(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Current,
        },
        context.selection,
        Permission::CompanyDocumentsRead,
        |tx, _| rentemester_storage::documents::list(tx, slug),
    )
}

mod service;
pub use service::{
    finish_service, list_service, prepare_original_service, prepare_service, read_service,
};

#[cfg(test)]
mod tests;
