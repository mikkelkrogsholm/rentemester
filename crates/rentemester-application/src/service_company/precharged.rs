//! Single-use in-memory permission to finish one admitted external-I/O request.
//! Preparation charges the service once; completion always rechecks live authority.
use super::*;
pub(crate) struct Permit {
    key_hash: String,
    workspace: String,
    slug: String,
    registered: bool,
    archived: bool,
    permission: Permission,
}
pub(crate) fn prepare<T>(
    control: &mut Connection,
    company: &mut Connection,
    context: Context<'_>,
    permission: Permission,
    apply: impl FnOnce(&Transaction<'_>, &str) -> Result<T, Error>,
) -> Result<ServiceResult<(T, Permit)>, ServiceFailure> {
    let permit = Permit {
        key_hash: context.key.lookup_hash().into(),
        workspace: context.selection.workspace.into(),
        slug: context.selection.slug.into(),
        registered: context.selection.registered,
        archived: context.selection.archived,
        permission,
    };
    execute_operation(
        control,
        company,
        context,
        permission,
        true,
        |tx, subject, _| {
            Ok(OperationResult {
                result: (apply(tx, subject)?, permit),
                receipt: None,
            })
        },
    )
}
pub(crate) fn finish<T>(
    control: &mut Connection,
    company: &mut Connection,
    context: Context<'_>,
    permit: Permit,
    apply: impl FnOnce(&Transaction<'_>, &str) -> Result<T, Error>,
) -> Result<ServiceResult<T>, ServiceFailure> {
    continue_operation(control, company, context, &permit, apply)
}
/// Continue one synchronous admitted operation without charging again. The caller
/// retains the opaque permit until its final stage; live authority is checked each time.
pub(crate) fn continue_operation<T>(
    control: &mut Connection,
    company: &mut Connection,
    context: Context<'_>,
    permit: &Permit,
    apply: impl FnOnce(&Transaction<'_>, &str) -> Result<T, Error>,
) -> Result<ServiceResult<T>, ServiceFailure> {
    let failure = |error: Error, reusable| ServiceFailure {
        usage: UsageStage::Committed,
        business: BusinessStage::NotAttempted,
        control_reusable: reusable,
        company_reusable: reusable,
        ..initial(error)
    };
    control_fence::connection_ready(control).map_err(|e| failure(e, false))?;
    control_fence::connection_ready(company).map_err(|e| failure(e, false))?;
    let s = &context.selection;
    if permit.key_hash != context.key.lookup_hash()
        || permit.workspace != s.workspace
        || permit.slug != s.slug
        || permit.registered != s.registered
        || permit.archived != s.archived
        || context.key.workspace() != s.workspace
    {
        return Err(failure(error("service_admission_changed"), true));
    }
    let catalog = Catalog {
        workspace: s.workspace.into(),
        companies: if s.registered {
            vec![RegisteredCompany {
                slug: s.slug.into(),
                name: s.slug.into(),
                archived: s.archived,
            }]
        } else {
            vec![]
        },
    };
    let mut attempted = false;
    let done = control_fence::execute_authorized(
        control,
        company,
        |tx| {
            rentemester_storage::workspace_transfer::require_business_write(tx)?;
            let observed = service_context::observe(
                tx,
                context.key.lookup_hash(),
                now(context.clock)?,
                &catalog,
            )?;
            authorize(tx, &observed, s, permit.permission)?;
            Ok(observed.subject().to_owned())
        },
        |tx, subject| {
            attempted = true;
            apply(tx, subject)
        },
    );
    match done {
        Ok(done) => Ok(ServiceResult {
            usage: UsageStage::Committed,
            business: BusinessStage::Committed,
            result: done.value,
            receipt: None,
            control_reusable: done.control_reusable,
        }),
        Err(f) => Err(ServiceFailure {
            usage: UsageStage::Committed,
            business: if f.stage == FailureStage::OutcomeUnknown {
                BusinessStage::OutcomeUnknown
            } else if attempted {
                BusinessStage::NotCommitted
            } else {
                BusinessStage::NotAttempted
            },
            error: f.error,
            control_reusable: f.control_reusable,
            company_reusable: f.company_reusable,
            rejection: None,
        }),
    }
}
