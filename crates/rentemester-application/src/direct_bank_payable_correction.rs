//! Current principal authorization and physical source proof across one composite transaction.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
};
use rentemester_engine::{Error, fiscal_year::Date};
use rentemester_runtime::namespace::NamespaceGuard;
use rentemester_storage::direct_bank_payable_correction::{self as storage, ApplyInput, PlanInput};
use rusqlite::Connection;
use serde_json::{Value, json};
pub enum Action<'a> {
    Plan(&'a PlanInput),
    Apply(&'a ApplyInput),
    OriginalPlan(&'a PlanInput),
    OriginalApply {
        input: &'a ApplyInput,
        expected: Option<&'a rentemester_storage::bank_original::ExpectedPrincipal>,
    },
}
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    action: Action<'_>,
) -> Result<Committed<Value>, Failure> {
    let permission = match action {
        Action::Plan(_) | Action::OriginalPlan(_) => Permission::CompanyRead,
        Action::Apply(_) | Action::OriginalApply { .. } => Permission::CompanyLedgerPost,
    };
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
        |tx, subject| {
            authorized(
                tx,
                guard,
                action,
                subject,
                rentemester_storage::idempotency::PrincipalKind::User,
                clock,
            )
        },
    )
}
fn authorized(
    tx: &rusqlite::Transaction<'_>,
    guard: &mut NamespaceGuard,
    action: Action<'_>,
    subject: &str,
    kind: rentemester_storage::idempotency::PrincipalKind,
    clock: &dyn rentemester_runtime::auth::Clock,
) -> Result<Value, Error> {
    if let Action::OriginalApply {
        expected: Some(expected),
        ..
    } = &action
    {
        expected.assert_current(kind, subject)?;
    }
    let input = match action {
        Action::Plan(i) | Action::OriginalPlan(i) => i.clone(),
        Action::Apply(i) | Action::OriginalApply { input: i, .. } => i.source(),
    };
    input.validate()?;
    let original = rentemester_storage::documents::evidence(tx, input.document_id)?;
    guard
        .read_document(&original.path, &original.hash)
        .map_err(|_| {
            Error::new(
                "document_evidence_unavailable",
                "Correction purchase original missing or changed",
            )
        })?;
    let hash =
        rentemester_storage::bank_reconciliation::source_hash(tx, input.bank_transaction_id)?;
    let bytes = guard.read_bank_statement(&hash).map_err(|_| {
        Error::new(
            "bank_statement_original_unavailable",
            "Correction bank archive missing or changed",
        )
    })?;
    rentemester_storage::bank_reconciliation::verify_source(tx, input.bank_transaction_id, &bytes)?;
    let now = clock
        .now_unix_ms()
        .map_err(|e| Error::new(e.code(), "trusted correction clock unavailable"))?;
    let latest = Date::parse(
        &rentemester_runtime::posting_time::latest_date(now, 45)
            .map_err(|e| Error::new(e.code(), "trusted posting horizon unavailable"))?,
    )?;
    match action {
        Action::Plan(i) => Ok(json!({"ok":true,"plan":storage::plan(tx,i,latest)?})),
        Action::Apply(i) => storage::apply(tx, i, subject, latest, now),
        Action::OriginalPlan(i) => Ok(rentemester_storage::bank_original::envelope(
            json!({"ok":true,"plan":storage::plan(tx,i,latest)?}),
            false,
        )),
        Action::OriginalApply { input: i, .. } => {
            let value = storage::apply_original(tx, i, subject, latest, now, kind)?;
            let mut correction = value["correction"].clone();
            if let Some(receipt) = value.get("idempotency") {
                correction["idempotency"] = receipt.clone();
            }
            Ok(rentemester_storage::bank_original::envelope(
                correction, false,
            ))
        }
    }
}
/// Only original compatibility actions deliberately admit authenticated service accounts.
pub fn execute_service(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: crate::service_company::Context<'_>,
    action: Action<'_>,
) -> Result<crate::service_company::ServiceResult<Value>, crate::service_company::ServiceFailure> {
    let (permission, mutation) = match action {
        Action::OriginalPlan(_) => (Permission::CompanyRead, false),
        Action::OriginalApply { .. } => (Permission::CompanyLedgerPost, true),
        _ => {
            return Err(crate::service_company::initial(Error::new(
                "access_denied",
                "native direct correction remains human-only",
            )));
        }
    };
    let clock = context.clock;
    crate::service_company::execute_operation(
        control,
        company,
        context,
        permission,
        mutation,
        |tx, subject, _| {
            Ok(crate::service_company::OperationResult {
                result: authorized(
                    tx,
                    guard,
                    action,
                    subject,
                    rentemester_storage::idempotency::PrincipalKind::ServiceAccount,
                    clock,
                )?,
                receipt: None,
            })
        },
    )
}
#[cfg(test)]
mod tests;
