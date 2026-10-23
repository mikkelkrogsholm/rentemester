//! Service uploads retain the ordinary scanner split and immutable registration.
use super::*;
use crate::service_company::{self, ServiceFailure, ServiceResult};
pub fn prepare_service(
    control: &mut Connection,
    company: &mut Connection,
    context: service_company::Context<'_>,
    file: Prepared,
    metadata: Metadata,
    policy: ScanPolicy,
) -> Result<ServiceResult<Admission>, ServiceFailure> {
    prepare_original_service(
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
pub fn prepare_original_service(
    control: &mut Connection,
    company: &mut Connection,
    context: service_company::Context<'_>,
    file: Prepared,
    mut metadata: Metadata,
    policy: ScanPolicy,
    options: OriginalOptions,
) -> Result<ServiceResult<Admission>, ServiceFailure> {
    let workspace = context.selection.workspace.to_owned();
    let slug = context.selection.slug.to_owned();
    let mut result = service_company::precharged::prepare(
        control,
        company,
        context,
        Permission::CompanyDocumentsUpload,
        |tx, subject| {
            let vendor_observation = rentemester_storage::documents_original::vendor_fill(
                tx,
                &mut metadata,
                options.vendor_id,
            )?;
            metadata.validate()?;
            Ok(Admission {
                file,
                metadata,
                workspace,
                slug,
                subject: subject.into(),
                ledger: ledger_identity(tx)?,
                policy,
                service_permit: None,
                original_options: options,
                vendor_observation,
            })
        },
    )?;
    let (mut admission, permit) = result.result;
    admission.service_permit = Some(permit);
    Ok(ServiceResult {
        result: admission,
        usage: result.usage,
        business: result.business,
        receipt: result.receipt.take(),
        control_reusable: result.control_reusable,
    })
}
pub fn finish_service(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: service_company::Context<'_>,
    mut admission: Admission,
    scan: Option<ScanEvidence>,
) -> Result<ServiceResult<Value>, ServiceFailure> {
    let fail = |code, reusable| service_company::ServiceFailure {
        usage: service_company::UsageStage::Committed,
        business: service_company::BusinessStage::NotAttempted,
        error: Error::new(code, "service document completion refused"),
        control_reusable: reusable,
        company_reusable: reusable,
        rejection: None,
    };
    guard
        .ensure_connections(control, company)
        .map_err(|_| fail("document_namespace_changed", false))?;
    let permit = admission
        .service_permit
        .take()
        .ok_or_else(|| fail("document_admission_changed", true))?;
    let workspace = context.selection.workspace.to_owned();
    let slug = context.selection.slug.to_owned();
    let clock = context.clock;
    service_company::precharged::finish(control, company, context, permit, |tx, subject| {
        admission.validate_binding(&workspace, &slug, subject, &ledger_identity(tx)?)?;
        admission.validate_scan(scan.as_ref())?;
        admission.validate_vendor(tx)?;
        admission.metadata.validate()?;
        let today = rentemester_runtime::posting_time::latest_date(
            clock.now_unix_ms().map_err(|_| unavailable())?,
            0,
        )
        .map_err(|_| unavailable())?;
        if admission.original_options.original_api && let Some(message)=rentemester_storage::documents_original::intake_duplicate(tx,admission.file.hash(),&admission.metadata,admission.original_options.force)? {
            return rentemester_storage::documents_original::blocked(tx,admission.file.filename(),admission.file.hash(),&admission.metadata,&message);
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
            rentemester_storage::idempotency::PrincipalKind::ServiceAccount,
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
        Ok(receipt)
    })
}
pub fn list_service(
    control: &mut Connection,
    company: &mut Connection,
    context: service_company::Context<'_>,
) -> Result<ServiceResult<Value>, ServiceFailure> {
    let slug = context.selection.slug.to_owned();
    service_company::execute_operation(
        control,
        company,
        context,
        Permission::CompanyDocumentsRead,
        false,
        |tx, _, _| {
            Ok(service_company::OperationResult {
                result: rentemester_storage::documents::list(tx, &slug)?,
                receipt: None,
            })
        },
    )
}
pub fn read_service(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: service_company::Context<'_>,
    id: i64,
) -> Result<ServiceResult<Original>, ServiceFailure> {
    let key = context.key;
    let clock = context.clock;
    let slug = context.selection.slug;
    let registered = context.selection.registered;
    let archived = context.selection.archived;
    let result = service_company::execute_operation(
        control,
        company,
        context,
        Permission::CompanyDocumentsRead,
        false,
        |tx, _, _| {
            let evidence = rentemester_storage::documents::evidence(tx, id)?;
            let bytes = guard
                .read_document(&evidence.path, &evidence.hash)
                .map_err(|_| unavailable())?;
            Ok(service_company::OperationResult {
                result: Original {
                    bytes,
                    filename: evidence.filename,
                    mime: evidence.mime,
                },
                receipt: None,
            })
        },
    );
    crate::document_access::service(
        control,
        key,
        clock,
        slug,
        registered,
        archived,
        "document_file",
        id,
        result,
    )
}
