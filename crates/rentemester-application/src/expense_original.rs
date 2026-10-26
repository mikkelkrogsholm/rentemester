//! Original purchase operations. Network observations enter only through an opaque admission.
use crate::{
    access::Permission,
    control_fence::{
        self, Committed, CookieAuthorization, Failure, FailureStage, SessionRequirement,
    },
    documents::Context,
    service_company,
};
use rentemester_engine::{Error, fiscal_year::Date};
use rentemester_runtime::{auth::Clock, namespace::NamespaceGuard};
pub use rentemester_storage::expense_original::Input;
use rentemester_storage::{idempotency::PrincipalKind, purchase_preflight as store};
use rusqlite::{Connection, Transaction};
use serde_json::Value;
pub use store::ProviderResult;

pub struct Admission {
    permit: Option<service_company::precharged::Permit>,
    workspace: String,
    slug: String,
    subject: String,
    ledger: String,
    document: i64,
    source: (String, String, Vec<u8>),
    prepared: store::Prepared,
    facts: SupplierFacts,
}
impl Admission {
    pub fn provider_vat(&self) -> Option<&str> {
        self.prepared.provider_vat()
    }
}
fn unavailable() -> Error {
    Error::new(
        "document_evidence_unavailable",
        "Retained purchase source unavailable or changed",
    )
}
fn changed() -> Error {
    Error::new(
        "purchase_admission_changed",
        "Current purchase admission changed",
    )
}
fn now(clock: &dyn Clock) -> Result<i64, Error> {
    clock.now_unix_ms().map_err(|_| unavailable())
}
fn stamp(clock: &dyn Clock) -> Result<String, Error> {
    rentemester_engine::company_knowledge::timestamp(now(clock)?)
}
fn failed(error: Error, reusable: bool) -> Failure {
    Failure {
        stage: FailureStage::NotCommitted,
        error,
        control_reusable: reusable,
        company_reusable: reusable,
    }
}
fn service_failed() -> service_company::ServiceFailure {
    service_company::ServiceFailure {
        usage: service_company::UsageStage::Committed,
        business: service_company::BusinessStage::NotAttempted,
        error: changed(),
        control_reusable: false,
        company_reusable: false,
        rejection: None,
    }
}
fn actor(subject: &str, service: bool) -> String {
    format!(
        "{}:{subject}",
        if service { "service_account" } else { "user" }
    )
}
fn assert_actor(expected: Option<&str>, actual: &str) -> Result<(), Error> {
    if expected
        .is_some_and(|v| v.trim().is_empty() || v.trim() != v || v.len() > 160 || v != actual)
    {
        return Err(Error::new(
            "access_denied",
            "Current actor assertion did not match",
        ));
    }
    Ok(())
}
fn ledger(db: &Connection) -> Result<String, Error> {
    db.query_row(
        "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
        [],
        |r| r.get(0),
    )
    .map_err(|_| unavailable())
}
fn source(
    tx: &Transaction<'_>,
    guard: &mut NamespaceGuard,
    id: i64,
) -> Result<(String, String, Vec<u8>), Error> {
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
        .map_err(|_| unavailable())?;
    for retained in rentemester_storage::invoice_extraction::sources(tx)? {
        if retained.document_id == id
            && (retained.sha256 != original.hash || retained.pdf_bytes != bytes)
        {
            return Err(changed());
        }
    }
    Ok((original.path, original.hash, bytes))
}
type SupplierFacts = (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<Value>,
);
fn facts(tx: &Transaction<'_>, id: i64) -> Result<SupplierFacts, Error> {
    let (vat,country,kind,status) = tx.query_row("SELECT sender_vat_cvr,supplier_country_code,supplier_identifier_kind,supplier_identity_status FROM documents WHERE id=?1",[id],|r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).map_err(|_| unavailable())?;
    Ok((
        vat,
        country,
        kind,
        status,
        rentemester_storage::invoice_extraction::inspect(tx, id)?,
    ))
}
#[allow(clippy::too_many_arguments)]
fn prepare_tx(
    tx: &Transaction<'_>,
    guard: &mut NamespaceGuard,
    context: (&str, &str),
    subject: &str,
    id: i64,
    expected: Option<&str>,
    clock: &dyn Clock,
    service: bool,
) -> Result<Admission, Error> {
    let actor = actor(subject, service);
    assert_actor(expected, &actor)?;
    let source = source(tx, guard, id)?;
    let ledger = ledger(tx)?;
    let facts = facts(tx, id)?;
    let prepared = store::prepare_apply(tx, id, &actor, &stamp(clock)?)?;
    Ok(Admission {
        permit: None,
        workspace: context.0.into(),
        slug: context.1.into(),
        subject: subject.into(),
        ledger,
        document: id,
        facts,
        source,
        prepared,
    })
}
#[allow(clippy::too_many_arguments)]
fn finish_tx(
    tx: &Transaction<'_>,
    guard: &mut NamespaceGuard,
    context: (&str, &str),
    subject: &str,
    admission: Admission,
    observation: Option<ProviderResult>,
    clock: &dyn Clock,
    service: bool,
) -> Result<Value, Error> {
    if admission.workspace != context.0
        || admission.slug != context.1
        || admission.subject != subject
        || ledger(tx)? != admission.ledger
        || source(tx, guard, admission.document)? != admission.source
        || facts(tx, admission.document)? != admission.facts
    {
        return Err(changed());
    }
    store::finish_apply(
        tx,
        admission.prepared,
        observation,
        &actor(subject, service),
        &stamp(clock)?,
    )
}
pub fn prepare(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    id: i64,
    expected: Option<&str>,
) -> Result<Committed<Admission>, Failure> {
    let selection = (context.selection.workspace, context.selection.slug);
    control_fence::execute_cookie_subject(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Fresh,
        },
        context.selection,
        Permission::CompanyLedgerPost,
        |tx, subject| {
            prepare_tx(
                tx,
                guard,
                selection,
                subject,
                id,
                expected,
                context.clock,
                false,
            )
        },
    )
}
pub fn finish(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    admission: Admission,
    observation: Option<ProviderResult>,
) -> Result<Committed<Value>, Failure> {
    let selection = (context.selection.workspace, context.selection.slug);
    if admission.permit.is_some() {
        return Err(failed(changed(), true));
    }
    guard
        .ensure_connections(control, company)
        .map_err(|_| failed(unavailable(), false))?;
    control_fence::execute_cookie_subject(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Fresh,
        },
        context.selection,
        Permission::CompanyLedgerPost,
        |tx, subject| {
            finish_tx(
                tx,
                guard,
                selection,
                subject,
                admission,
                observation,
                context.clock,
                false,
            )
        },
    )
}
pub fn read(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    id: i64,
) -> Result<Committed<Value>, Failure> {
    control_fence::execute_cookie_subject_mode(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Current,
        },
        context.selection,
        Permission::CompanyDocumentsRead,
        true,
        |tx, _| {
            source(tx, guard, id)?;
            store::inspect(tx, id, &stamp(context.clock)?)
        },
    )
}
// Equality-only observation before usage charging. This does not confer authority;
// the standard operation fence still resolves and authorizes the current key.
fn check_service_actor(
    control: &mut Connection,
    company: &Connection,
    context: &service_company::Context<'_>,
    expected: Option<&str>,
) -> Result<(), service_company::ServiceFailure> {
    let Some(expected) = expected else {
        return Ok(());
    };
    let check = (|| {
        let catalog = rentemester_storage::workspace_context::Catalog {
            workspace: context.selection.workspace.into(),
            companies: if context.selection.registered {
                vec![rentemester_storage::workspace_context::RegisteredCompany {
                    slug: context.selection.slug.into(),
                    name: context.selection.slug.into(),
                    purpose: Default::default(), archived: context.selection.archived,
                }]
            } else {
                vec![]
            },
        };
        let tx = control
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|_| unavailable())?;
        let observed = rentemester_storage::service_context::observe(
            &tx,
            context.key.lookup_hash(),
            now(context.clock)?,
            &catalog,
        )?;
        assert_actor(Some(expected), &actor(observed.subject(), true))?;
        tx.rollback().map_err(|_| unavailable())
    })();
    check.map_err(|error| service_company::ServiceFailure {
        usage: service_company::UsageStage::NotCommitted,
        business: service_company::BusinessStage::NotAttempted,
        error,
        control_reusable: control.is_autocommit(),
        company_reusable: company.is_autocommit(),
        rejection: None,
    })
}
pub fn prepare_service(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: service_company::Context<'_>,
    id: i64,
    expected: Option<&str>,
) -> Result<service_company::ServiceResult<Admission>, service_company::ServiceFailure> {
    check_service_actor(control, company, &context, expected)?;
    let selection = (context.selection.workspace, context.selection.slug);
    let clock = context.clock;
    let result = service_company::precharged::prepare(
        control,
        company,
        context,
        Permission::CompanyLedgerPost,
        |tx, subject| prepare_tx(tx, guard, selection, subject, id, expected, clock, true),
    )?;
    if !result.control_reusable || !control.is_autocommit() || !company.is_autocommit() {
        return Err(service_failed());
    }
    let (mut admission, permit) = result.result;
    admission.permit = Some(permit);
    Ok(service_company::ServiceResult {
        result: admission,
        usage: result.usage,
        business: result.business,
        receipt: result.receipt,
        control_reusable: result.control_reusable,
    })
}
pub fn finish_service(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: service_company::Context<'_>,
    mut admission: Admission,
    observation: Option<ProviderResult>,
) -> Result<service_company::ServiceResult<Value>, service_company::ServiceFailure> {
    guard
        .ensure_connections(control, company)
        .map_err(|_| service_failed())?;
    let permit = admission.permit.take().ok_or_else(service_failed)?;
    let selection = (context.selection.workspace, context.selection.slug);
    let clock = context.clock;
    service_company::precharged::finish(control, company, context, permit, |tx, subject| {
        finish_tx(
            tx,
            guard,
            selection,
            subject,
            admission,
            observation,
            clock,
            true,
        )
    })
}
pub fn read_service(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: service_company::Context<'_>,
    id: i64,
) -> Result<service_company::ServiceResult<Value>, service_company::ServiceFailure> {
    let clock = context.clock;
    service_company::execute_operation(
        control,
        company,
        context,
        Permission::CompanyDocumentsRead,
        false,
        |tx, _, _| {
            source(tx, guard, id)?;
            Ok(service_company::OperationResult {
                result: store::inspect(tx, id, &stamp(clock)?)?,
                receipt: None,
            })
        },
    )
}
fn book_tx(
    tx: &Transaction<'_>,
    guard: &mut NamespaceGuard,
    subject: &str,
    input: &Input,
    expected: Option<&str>,
    clock: &dyn Clock,
    service: bool,
) -> Result<Value, rentemester_storage::idempotency::Failure> {
    book_tx_program(tx, guard, subject, input, expected, clock, service, false)
}
pub(crate) fn book_tx_agent(
    tx: &Transaction<'_>,
    guard: &mut NamespaceGuard,
    subject: &str,
    input: &Input,
    expected: Option<&str>,
    clock: &dyn Clock,
    service: bool,
) -> Result<Value, rentemester_storage::idempotency::Failure> {
    book_tx_program(tx, guard, subject, input, expected, clock, service, true)
}
fn book_tx_program(
    tx: &Transaction<'_>,
    guard: &mut NamespaceGuard,
    subject: &str,
    input: &Input,
    expected: Option<&str>,
    clock: &dyn Clock,
    service: bool,
    agent: bool,
) -> Result<Value, rentemester_storage::idempotency::Failure> {
    assert_actor(expected, &actor(subject, service))?;
    source(tx, guard, input.document_id)?;
    rentemester_storage::bank_original::verify_journals(tx)?;
    let hash =
        rentemester_storage::bank_reconciliation::source_hash(tx, input.bank_transaction_id)?;
    let bytes = guard
        .read_bank_statement(&hash)
        .map_err(|_| unavailable())?;
    rentemester_storage::bank_reconciliation::verify_source(tx, input.bank_transaction_id, &bytes)?;
    let now = now(clock)?;
    let latest =
        rentemester_runtime::posting_time::latest_date(now, 45).map_err(|_| unavailable())?;
    (if agent {
        rentemester_storage::expense_original::book_agent
    } else {
        rentemester_storage::expense_original::book
    })(
        tx,
        input,
        subject,
        if service {
            PrincipalKind::ServiceAccount
        } else {
            PrincipalKind::User
        },
        Date::parse(&latest)?,
        now,
    )
}
pub fn book(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    input: &Input,
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
        Permission::CompanyLedgerPost,
        |tx, subject| {
            book_tx(tx, guard, subject, input, expected, context.clock, false).map_err(|f| f.error)
        },
    )
}
pub fn book_service(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: service_company::Context<'_>,
    input: &Input,
    expected: Option<&str>,
) -> Result<service_company::ServiceResult<Value>, service_company::ServiceFailure> {
    check_service_actor(control, company, &context, expected)?;
    let clock = context.clock;
    let mut rejection = None;
    let result = service_company::execute_operation(
        control,
        company,
        context,
        Permission::CompanyLedgerPost,
        true,
        |tx, subject, _| {
            let result =
                book_tx(tx, guard, subject, input, expected, clock, true).map_err(|f| {
                    rejection = f.rejection;
                    f.error
                })?;
            Ok(service_company::OperationResult {
                result,
                receipt: None,
            })
        },
    );
    result.map_err(|mut f| {
        f.rejection = rejection;
        f
    })
}
/// Convert only the already bounded VIES observation; provider bodies remain internal.
pub fn observation(
    result: Result<Value, rentemester_runtime::vies::Error>,
) -> Result<ProviderResult, Error> {
    use rentemester_runtime::vies::Error as E;
    let empty = |status: &str| ProviderResult {
        status: status.into(),
        name: None,
        address: None,
        raw_response: None,
    };
    match result {
        Ok(v) => {
            let Some(valid) = v["valid"].as_bool() else {
                return Ok(empty("inconclusive"));
            };
            Ok(ProviderResult {
                status: if valid { "valid" } else { "invalid" }.into(),
                name: v["name"].as_str().map(str::to_owned),
                address: v["address"].as_str().map(str::to_owned),
                raw_response: v["rawResponse"].as_str().map(str::to_owned),
            })
        }
        Err(E::Configuration) => Err(Error::new(
            "vies_invalid_config",
            "VAT provider configuration unavailable",
        )),
        Err(E::InvalidInput | E::Protocol) => Ok(empty("inconclusive")),
        Err(E::Unavailable | E::Refused { .. }) => Ok(empty("unavailable")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rentemester_runtime::vies::Error as ViesError;
    use serde_json::json;
    #[test]
    fn provider_outcomes_are_not_invented_successes() {
        for (input, status) in [
            (Ok(json!({"valid":true})), "valid"),
            (Ok(json!({"valid":false})), "invalid"),
            (Ok(json!({"valid":"true"})), "inconclusive"),
            (Err(ViesError::Protocol), "inconclusive"),
            (Err(ViesError::InvalidInput), "inconclusive"),
            (Err(ViesError::Unavailable), "unavailable"),
            (Err(ViesError::Refused { status: 503 }), "unavailable"),
        ] {
            assert_eq!(observation(input).unwrap().status, status);
        }
        assert!(observation(Err(ViesError::Configuration)).is_err());
    }
    #[test]
    fn actor_is_only_current_identity_assertion() {
        assert!(assert_actor(None, "user:current").is_ok());
        assert!(assert_actor(Some("user:current"), "user:current").is_ok());
        for expected in ["user:other", " user:current", "", "service_account:current"] {
            assert!(assert_actor(Some(expected), "user:current").is_err());
        }
    }
}
