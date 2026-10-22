//! Source accrual operations inside the existing current company fences.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
    service_company,
};
use rentemester_engine::{Error, fiscal_year::Date};
use rentemester_runtime::{auth::Clock, namespace::NamespaceGuard};
use rentemester_storage::{accrual_original as store, idempotency::PrincipalKind};
use rusqlite::{Connection, Transaction};
use serde_json::Value;
pub use store::{Recognize, Register};

pub enum Action<'a> {
    Report,
    Register(&'a Register),
    Recognize(i64, &'a Recognize),
    /// Transport validation is evaluated only after current authority admission.
    Invalid,
}
fn permission(action: &Action<'_>) -> Permission {
    if matches!(action, Action::Report) {
        Permission::CompanyRead
    } else {
        Permission::CompanyLedgerPost
    }
}
fn apply(
    tx: &Transaction<'_>,
    guard: &mut NamespaceGuard,
    clock: &dyn Clock,
    subject: &str,
    kind: PrincipalKind,
    action: Action<'_>,
    expected: Option<&str>,
) -> Result<Value, Error> {
    if !matches!(action, Action::Report) {
        let actor = format!(
            "{}:{subject}",
            if matches!(kind, PrincipalKind::ServiceAccount) {
                "service_account"
            } else {
                "user"
            }
        );
        if expected.is_some_and(|v| {
            v.trim().is_empty()
                || v.trim() != v
                || v.len() > 160
                || v.chars().any(char::is_control)
                || v != actor
        }) {
            return Err(Error::new(
                "access_denied",
                "Current actor assertion did not match",
            ));
        }
    }
    if matches!(action, Action::Invalid) {
        return Err(Error::new(
            "invalid_input",
            "Invalid original accrual request",
        ));
    }
    rentemester_storage::bank_original::verify_journals(tx)?;
    let document = match action {
        Action::Report => return store::report(tx),
        Action::Register(input) => input.document_id,
        Action::Recognize(id, _) => store::document_id(tx, id)?,
        Action::Invalid => unreachable!(),
    };
    if let Some(id) = document {
        if !(1..=9_007_199_254_740_991).contains(&id) {
            return Err(Error::new(
                "invalid_input",
                "Positive safe document ID required",
            ));
        }
        rentemester_storage::invoice_extraction::validate_history(tx)?;
        let original = rentemester_storage::documents::evidence(tx, id)?;
        let bytes = guard
            .read_document(&original.path, &original.hash)
            .map_err(|_| {
                Error::new(
                    "document_evidence_unavailable",
                    "Retained accrual original unavailable or changed",
                )
            })?;
        for source in rentemester_storage::invoice_extraction::sources(tx)? {
            if source.document_id == id
                && (source.sha256 != original.hash || source.pdf_bytes != bytes)
            {
                return Err(Error::new(
                    "document_evidence_unavailable",
                    "Retained accrual source changed",
                ));
            }
        }
    }
    let now = clock
        .now_unix_ms()
        .map_err(|_| Error::new("clock_unavailable", "Accrual clock unavailable"))?;
    let days = if matches!(action, Action::Register(_)) {
        0
    } else {
        45
    };
    let latest = rentemester_runtime::posting_time::latest_date(now, days)
        .map_err(|_| Error::new("clock_unavailable", "Accrual posting horizon unavailable"))?;
    let latest = Date::parse(&latest)?;
    match action {
        Action::Register(input) => store::register(tx, input, subject, kind, latest),
        Action::Recognize(id, input) => store::recognize(tx, id, input, subject, kind, latest),
        _ => unreachable!(),
    }
}
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    action: Action<'_>,
    expected: Option<&str>,
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
        permission(&action),
        |tx, subject| {
            apply(
                tx,
                guard,
                context.clock,
                subject,
                PrincipalKind::User,
                action,
                expected,
            )
        },
    )
}
pub fn execute_service(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: service_company::Context<'_>,
    action: Action<'_>,
    expected: Option<&str>,
) -> Result<service_company::ServiceResult<Value>, service_company::ServiceFailure> {
    let clock = context.clock;
    let mutation = !matches!(action, Action::Report);
    // execute_operation commits exactly one usage charge, then rechecks the live
    // typed service key/scope/permission under its company/backup/schema fence.
    service_company::execute_operation(
        control,
        company,
        context,
        permission(&action),
        mutation,
        |tx, subject, _| {
            Ok(service_company::OperationResult {
                result: apply(
                    tx,
                    guard,
                    clock,
                    subject,
                    PrincipalKind::ServiceAccount,
                    action,
                    expected,
                )?,
                receipt: None,
            })
        },
    )
}
