//! Explicit manual recurring generation shares ordinary invoice issuance and current authority.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
};
use rentemester_runtime::namespace::NamespaceGuard;
use rentemester_storage::recurring_invoices::{self as recurring, Prepared};
use rusqlite::Connection;
use serde_json::Value;
pub enum Action<'a> {
    List,
    OriginalList(bool),
    OriginalCreate(&'a recurring::original::Create),
    OriginalGenerate(&'a recurring::original::Generate),
    Create(&'a recurring::Create),
    Generate(i64, &'a recurring::Generate),
    Retire(i64, &'a recurring::Retire),
}
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    action: Action<'_>,
    today: &str,
    now: &str,
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
        if matches!(action, Action::List | Action::OriginalList(_)) {
            Permission::CompanyRead
        } else {
            Permission::CompanyDraftWrite
        },
        |tx, actor| match action {
            Action::OriginalList(include) => recurring::original::list(tx, slug, include),
            Action::OriginalCreate(i) => recurring::original::create(
                tx,
                i,
                actor,
                rentemester_storage::idempotency::PrincipalKind::User,
            ),
            Action::OriginalGenerate(i) => original_generate(
                tx,
                guard,
                i,
                today,
                now,
                actor,
                rentemester_storage::idempotency::PrincipalKind::User,
            ),
            Action::List => recurring::list(tx, slug),
            Action::Create(i) => recurring::create(tx, i, actor, now),
            Action::Retire(id, i) => recurring::retire(tx, id, i, actor),
            Action::Generate(id, i) => match recurring::prepare(tx, id, i, today, now)? {
                Prepared::Existing(value) => {
                    crate::sales::verify_invoice(tx, guard, value["documentId"].as_i64().unwrap())?;
                    Ok(value)
                }
                Prepared::Issue(pending) => {
                    let issued = crate::sales::issue_payload(tx, guard, &pending.payload, actor)?;
                    recurring::finish(tx, &pending, &issued, actor)
                }
            },
        },
    )
}

fn original_generate(
    tx: &rusqlite::Transaction<'_>,
    guard: &mut NamespaceGuard,
    input: &recurring::original::Generate,
    today: &str,
    now: &str,
    actor: &str,
    principal: rentemester_storage::idempotency::PrincipalKind,
) -> Result<Value, rentemester_engine::Error> {
    original_prepared(
        tx,
        guard,
        recurring::original::prepare(tx, input, today, now)?,
        actor,
        principal,
    )
}

pub(crate) fn original_prepared(
    tx: &rusqlite::Transaction<'_>,
    guard: &mut NamespaceGuard,
    prepared: Prepared,
    actor: &str,
    principal: rentemester_storage::idempotency::PrincipalKind,
) -> Result<Value, rentemester_engine::Error> {
    match prepared {
        Prepared::Existing(value) => {
            crate::sales::verify_invoice(
                tx,
                guard,
                value["documentId"].as_i64().ok_or_else(|| {
                    rentemester_engine::Error::new(
                        "invoice_evidence_unavailable",
                        "generated invoice identity unavailable",
                    )
                })?,
            )?;
            Ok(recurring::original::generation_result(value, None))
        }
        Prepared::Issue(pending) => {
            let issued = crate::sales::issue_payload_with_principal(
                tx,
                guard,
                &pending.payload,
                actor,
                principal,
            )?;
            let value = recurring::finish_with_principal(tx, &pending, &issued, actor, principal)?;
            Ok(recurring::original::generation_result(
                value,
                Some(&pending.payload),
            ))
        }
    }
}
pub fn execute_service(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: crate::service_company::Context<'_>,
    action: Action<'_>,
    today: &str,
    now: &str,
) -> Result<crate::service_company::ServiceResult<Value>, crate::service_company::ServiceFailure> {
    use crate::service_company::{self, OperationResult};
    use rentemester_storage::idempotency::PrincipalKind;
    let read = matches!(action, Action::OriginalList(_));
    if !matches!(
        action,
        Action::OriginalList(_) | Action::OriginalCreate(_) | Action::OriginalGenerate(_)
    ) {
        return Err(service_company::initial(rentemester_engine::Error::new(
            "access_denied",
            "service recurring action unavailable",
        )));
    }
    let slug = context.selection.slug;
    service_company::execute_operation(
        control,
        company,
        context,
        if read {
            Permission::CompanyRead
        } else {
            Permission::CompanyDraftWrite
        },
        !read,
        |tx, actor, _| {
            let result = match action {
                Action::OriginalList(include) => recurring::original::list(tx, slug, include)?,
                Action::OriginalCreate(i) => {
                    recurring::original::create(tx, i, actor, PrincipalKind::ServiceAccount)?
                }
                Action::OriginalGenerate(i) => original_generate(
                    tx,
                    guard,
                    i,
                    today,
                    now,
                    actor,
                    PrincipalKind::ServiceAccount,
                )?,
                _ => unreachable!(),
            };
            Ok(OperationResult {
                result,
                receipt: None,
            })
        },
    )
}
