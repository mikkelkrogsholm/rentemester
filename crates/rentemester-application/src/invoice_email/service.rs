//! Service mail keeps one charged intent and fresh authority around the ordinary SMTP split.
use super::*;
use crate::service_company::{
    self, BusinessStage, Context, OperationResult, ServiceFailure, ServiceResult, UsageStage,
    precharged,
};
use rentemester_storage::idempotency::PrincipalKind;
fn invalid(error: Error) -> ServiceFailure {
    let mut failure = service_company::initial(error);
    failure.control_reusable = true;
    failure.company_reusable = true;
    failure
}
fn ambiguous(mut failure: ServiceFailure) -> ServiceFailure {
    failure.business = BusinessStage::OutcomeUnknown;
    failure
}
fn context<'a>(source: &Context<'a>) -> Context<'a> {
    Context {
        key: source.key,
        clock: source.clock,
        max_future_days: source.max_future_days,
        selection: control_fence::CompanySelection {
            workspace: source.selection.workspace,
            slug: source.selection.slug,
            registered: source.selection.registered,
            archived: source.selection.archived,
        },
    }
}
pub fn status_service(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
) -> Result<ServiceResult<Value>, ServiceFailure> {
    service_company::execute_operation(
        control,
        company,
        context,
        Permission::CompanyRead,
        false,
        |tx, _, _| {
            for source in store::sources(tx)? {
                verify(tx, guard, &source)?;
            }
            Ok(OperationResult {
                result: json!({"rows":store::rows(tx)?}),
                receipt: None,
            })
        },
    )
}
pub fn reconcile_service(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    input: &Reconcile,
) -> Result<ServiceResult<Value>, ServiceFailure> {
    input.validate().map_err(invalid)?;
    let clock = context.clock;
    service_company::execute_operation(
        control,
        company,
        context,
        Permission::CompanyExternalSend,
        true,
        |tx, subject, _| {
            let delivery = store::get(tx, &input.message_id)?.ok_or_else(store::refused)?;
            verify(tx, guard, &delivery.source)?;
            Ok(OperationResult {
                result: json!({"delivery":store::reconcile_with_principal(tx, input, subject, &at(clock)?, PrincipalKind::ServiceAccount)?.row(false)}),
                receipt: None,
            })
        },
    )
}
fn execute_service(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    ctx: Context<'_>,
    intent: Intent<'_>,
    transport: &mut dyn Transport,
) -> Result<ServiceResult<Value>, ServiceFailure> {
    match intent {
        Intent::Send(input) => {
            input.validate().map_err(invalid)?;
        }
        Intent::Retry(input) => {
            input.validate().map_err(invalid)?;
        }
    }
    let sender = transport.identity().clone();
    let reserved = precharged::prepare(
        control,
        company,
        context(&ctx),
        Permission::CompanyExternalSend,
        |tx, subject| {
            let (source, message, retry) = match intent {
                Intent::Send(input) => {
                    let (source, message) = material(tx, guard, input, &sender)?;
                    (source, message, None)
                }
                Intent::Retry(input) => {
                    let delivery = store::get(tx, &input.message_id)?.ok_or_else(store::refused)?;
                    if source_sender(&sender) != delivery.source.sender {
                        return Err(store::refused());
                    }
                    let message = verify(tx, guard, &delivery.source)?;
                    (delivery.source, message, Some(input))
                }
            };
            let attempt = new_token().map_err(|_| store::refused())?;
            match store::reserve_with_principal(
                tx,
                source.clone(),
                attempt.as_str(),
                subject,
                &at(ctx.clock)?,
                retry,
                PrincipalKind::ServiceAccount,
            )? {
                Reservation::Existing(row) => Ok(Pending {
                    row,
                    dispatch: None,
                }),
                Reservation::Reserved(delivery) => Ok(Pending {
                    row: delivery.row(false),
                    dispatch: Some((source, message, delivery.state_hash)),
                }),
            }
        },
    )?;
    let (pending, permit) = reserved.result;
    let Some((source, message, expected_hash)) = pending.dispatch else {
        return Ok(ServiceResult {
            usage: reserved.usage,
            business: reserved.business,
            result: json!({"delivery":pending.row}),
            receipt: None,
            control_reusable: reserved.control_reusable,
        });
    };
    let uncertain = |error, reusable| ServiceFailure {
        usage: UsageStage::Committed,
        business: BusinessStage::OutcomeUnknown,
        error,
        control_reusable: reusable,
        company_reusable: reusable,
        rejection: None,
    };
    if !reserved.control_reusable {
        return Err(uncertain(store::refused(), false));
    }
    #[cfg(test)]
    BEFORE_READY.with(|hook| {
        if let Some(action) = hook.borrow_mut().take() {
            action();
        }
    });
    let ready =
        precharged::continue_operation(control, company, context(&ctx), &permit, |tx, _| {
            let delivery = store::get(tx, &source.message_id)?.ok_or_else(store::refused)?;
            if delivery.state_hash != expected_hash
                || delivery.source != source
                || source_sender(transport.identity()) != source.sender
            {
                return Err(store::refused());
            }
            verify(tx, guard, &source)?;
            Ok(())
        })
        .map_err(ambiguous)?;
    if !ready.control_reusable {
        return Err(uncertain(store::refused(), false));
    }
    control_fence::connection_ready(control)
        .and_then(|_| control_fence::connection_ready(company))
        .map_err(|error| uncertain(error, false))?;
    let outcome = match transport.send(&message) {
        Handoff::Accepted => "accepted",
        Handoff::Refused => "refused",
        Handoff::Unknown => "unknown",
    };
    precharged::continue_operation(control, company, context(&ctx), &permit, |tx, subject| {
        verify(tx, guard, &source)?;
        Ok(json!({"delivery":store::terminal_with_principal(tx, &source.message_id, &expected_hash, outcome, subject, &at(ctx.clock)?, PrincipalKind::ServiceAccount)?.row(false)}))
    }).map_err(ambiguous)
}
pub fn send_service(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    input: &Send,
    transport: &mut dyn Transport,
) -> Result<ServiceResult<Value>, ServiceFailure> {
    execute_service(
        control,
        company,
        guard,
        context,
        Intent::Send(input),
        transport,
    )
}
pub fn retry_service(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    input: &Retry,
    transport: &mut dyn Transport,
) -> Result<ServiceResult<Value>, ServiceFailure> {
    execute_service(
        control,
        company,
        guard,
        context,
        Intent::Retry(input),
        transport,
    )
}
#[cfg(test)]
thread_local! {
    static BEFORE_READY: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const { std::cell::RefCell::new(None) };
}
#[cfg(test)]
mod tests;
