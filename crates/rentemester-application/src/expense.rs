//! Verify original evidence and post the expense atomically under current roles.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
};
use rentemester_engine::{Error, fiscal_year::Date};
use rentemester_runtime::namespace::NamespaceGuard;
pub use rentemester_storage::expense::Input;
use rusqlite::Connection;
use serde_json::Value;
pub enum Action<'a> {
    Original(rentemester_storage::payable_original::Action<'a>),
    Options(i64),
    Payables { as_of: &'a str, status: &'a str },
    Register(&'a rentemester_storage::payables::Register),
    Pay(i64, &'a rentemester_storage::payables::Payment),
    Post(&'a Input),
}
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    action: Action<'_>,
) -> Result<Committed<Value>, Failure> {
    let permission = match action {
        Action::Original(command) => {
            if crate::payable_original::mutation(command) {
                Permission::CompanyLedgerPost
            } else {
                Permission::CompanyRead
            }
        }
        Action::Options(_) => Permission::CompanyDocumentsRead,
        Action::Payables { .. } => Permission::CompanyRead,
        Action::Post(_) | Action::Register(_) | Action::Pay(_, _) => Permission::CompanyLedgerPost,
    };
    let slug = context.selection.slug;
    let clock = context.clock;
    control_fence::execute_cookie_subject(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock,
            requirement: SessionRequirement::Current,
        },
        context.selection,
        permission,
        |tx, subject| match action {
            Action::Original(command) => crate::payable_original::authorized(
                tx,
                guard,
                command,
                subject,
                rentemester_storage::idempotency::PrincipalKind::User,
                clock,
            ),
            Action::Options(id) => rentemester_storage::expense::options(tx, slug, id),
            Action::Payables { as_of, status } => {
                rentemester_storage::payables::list(tx, slug, as_of, status)
            }
            Action::Register(_) | Action::Pay(_, _) => {
                let id = match action {
                    Action::Register(input) => input.document_id,
                    Action::Pay(id, _) => rentemester_storage::payables::document_id(tx, id)?,
                    _ => unreachable!(),
                };
                let original = rentemester_storage::documents::evidence(tx, id)?;
                guard
                    .read_document(&original.path, &original.hash)
                    .map_err(|_| {
                        Error::new(
                            "document_evidence_unavailable",
                            "payable original unavailable or integrity mismatch",
                        )
                    })?;
                let now = clock
                    .now_unix_ms()
                    .map_err(|e| Error::new(e.code(), "posting clock unavailable"))?;
                let latest = rentemester_runtime::posting_time::latest_date(now, 45)
                    .map_err(|e| Error::new(e.code(), "posting horizon unavailable"))?;
                let latest = Date::parse(&latest)?;
                match action {
                    Action::Register(input) => {
                        rentemester_storage::payables::register(tx, input, subject, latest)
                    }
                    Action::Pay(id, input) => {
                        rentemester_storage::payables::pay(tx, id, input, subject, latest)
                    }
                    _ => unreachable!(),
                }
            }
            Action::Post(input) => {
                let original = rentemester_storage::documents::evidence(tx, input.document_id)?;
                guard
                    .read_document(&original.path, &original.hash)
                    .map_err(|_| {
                        Error::new(
                            "document_evidence_unavailable",
                            "expense original unavailable or integrity mismatch",
                        )
                    })?;
                let now = clock
                    .now_unix_ms()
                    .map_err(|e| Error::new(e.code(), "posting clock unavailable"))?;
                let latest = rentemester_runtime::posting_time::latest_date(now, 45)
                    .map_err(|e| Error::new(e.code(), "posting horizon unavailable"))?;
                rentemester_storage::expense::post(tx, input, subject, Date::parse(&latest)?)
            }
        },
    )
}

/// Explicit service bill recognition/payment; direct expense posting remains a separate flow.
pub fn execute_service(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: crate::service_company::Context<'_>,
    action: Action<'_>,
) -> Result<crate::service_company::ServiceResult<Value>, crate::service_company::ServiceFailure> {
    use crate::service_company::{self, OperationResult};
    use rentemester_storage::{documents, idempotency::PrincipalKind, payables};
    let (permission, mutation) = match action {
        Action::Original(command) => {
            let mutation = crate::payable_original::mutation(command);
            (
                if mutation {
                    Permission::CompanyLedgerPost
                } else {
                    Permission::CompanyRead
                },
                mutation,
            )
        }
        Action::Options(_) => (Permission::CompanyDocumentsRead, false),
        Action::Payables { .. } => (Permission::CompanyRead, false),
        Action::Register(_) | Action::Pay(_, _) => (Permission::CompanyLedgerPost, true),
        _ => {
            let mut failure = service_company::initial(Error::new(
                "access_denied",
                "service expense action not supported",
            ));
            failure.control_reusable = true;
            failure.company_reusable = true;
            return Err(failure);
        }
    };
    let slug = context.selection.slug;
    let clock = context.clock;
    service_company::execute_operation(
        control,
        company,
        context,
        permission,
        mutation,
        |tx, subject, _| {
            let result = match action {
                Action::Original(command) => crate::payable_original::authorized(
                    tx,
                    guard,
                    command,
                    subject,
                    PrincipalKind::ServiceAccount,
                    clock,
                )?,
                Action::Options(id) => rentemester_storage::expense::options(tx, slug, id)?,
                Action::Payables { as_of, status } => payables::list(tx, slug, as_of, status)?,
                Action::Register(_) | Action::Pay(_, _) => {
                    let id = match action {
                        Action::Register(input) => input.document_id,
                        Action::Pay(id, _) => payables::document_id(tx, id)?,
                        _ => unreachable!(),
                    };
                    let original = documents::evidence(tx, id)?;
                    guard
                        .read_document(&original.path, &original.hash)
                        .map_err(|_| {
                            Error::new(
                                "document_evidence_unavailable",
                                "payable original unavailable or integrity mismatch",
                            )
                        })?;
                    let now = clock
                        .now_unix_ms()
                        .map_err(|e| Error::new(e.code(), "posting clock unavailable"))?;
                    let latest = rentemester_runtime::posting_time::latest_date(now, 45)
                        .map_err(|e| Error::new(e.code(), "posting horizon unavailable"))?;
                    let latest = Date::parse(&latest)?;
                    match action {
                        Action::Register(input) => payables::register_with_principal(
                            tx,
                            input,
                            subject,
                            latest,
                            PrincipalKind::ServiceAccount,
                        )?,
                        Action::Pay(id, input) => payables::pay_with_principal(
                            tx,
                            id,
                            input,
                            subject,
                            latest,
                            PrincipalKind::ServiceAccount,
                        )?,
                        _ => unreachable!(),
                    }
                }
                _ => unreachable!("service action admitted above"),
            };
            Ok(OperationResult {
                result,
                receipt: None,
            })
        },
    )
}

#[cfg(test)]
mod service_tests;
