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
    let workspace = context.selection.workspace.to_owned();
    let slug = context.selection.slug.to_owned();
    let mut result = service_company::precharged::prepare(
        control,
        company,
        context,
        Permission::CompanyDocumentsUpload,
        |tx, subject| {
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
        admission.metadata.validate()?;
        let today = rentemester_runtime::posting_time::latest_date(
            clock.now_unix_ms().map_err(|_| unavailable())?,
            0,
        )
        .map_err(|_| unavailable())?;
        let path = guard
            .publish_document(&admission.file)
            .map_err(|_| unavailable())?;
        let receipt = rentemester_storage::documents::register_scanned_with_principal(
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
        )?;
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
    service_company::execute_operation(
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
    )
}
