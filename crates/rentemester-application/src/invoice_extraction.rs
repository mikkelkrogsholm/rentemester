//! Source-bound extraction admission. Provider I/O occurs only between these
//! fixed operations, with neither SQL transactions nor the Owner mutex held.
use crate::{
    access::Permission,
    control_fence::{
        self, Committed, CookieAuthorization, Failure, FailureStage, SessionRequirement,
    },
    documents::{Context, RegisteredUpload},
};
use rentemester_engine::{
    Error,
    invoice_extraction::{ProviderFailure, ProviderIdentity, ProviderOutput},
};
use rentemester_runtime::{invoice_extractor::Input, namespace::NamespaceGuard};
use rentemester_storage::invoice_extraction as store;
use rusqlite::{Connection, Transaction};
use serde_json::{Value, json};

pub struct Completion {
    pub identity: ProviderIdentity,
    pub outcome: Result<ProviderOutput, ProviderFailure>,
}
#[derive(PartialEq, Eq)]
struct Source {
    path: String,
    hash: String,
    filename: String,
    mime: String,
}
/// An owned observation of the authenticated company/source/context. It cannot
/// be decoded from caller JSON or reconstructed for a different principal.
#[derive(Clone, Copy)]
enum Origin {
    Explicit,
    RegisteredUpload,
}
impl Origin {
    fn permission(self) -> Permission {
        match self {
            Self::Explicit => Permission::CompanyMasterData,
            Self::RegisteredUpload => Permission::CompanyDocumentsUpload,
        }
    }
}
pub struct Admission {
    origin: Origin,
    service_permit: Option<crate::service_company::precharged::Permit>,
    workspace: String,
    slug: String,
    subject: String,
    ledger: String,
    source: Source,
    bytes: Vec<u8>,
    prepared: store::Prepared,
}
impl Admission {
    pub fn provider_input(&self) -> Option<Input<'_>> {
        self.prepared.cached().is_none().then_some(Input {
            sha256: &self.source.hash,
            pdf_bytes: &self.bytes,
        })
    }
    pub fn identity(&self) -> &ProviderIdentity {
        self.prepared.identity()
    }
}
fn unavailable() -> Error {
    Error::new(
        "EXTRACTION_SNAPSHOT_UNAVAILABLE",
        "Registered invoice source unavailable",
    )
}
fn changed() -> Error {
    Error::new(
        "EXTRACTION_ADMISSION_CHANGED",
        "Extraction principal, company, ledger or source changed",
    )
}
fn failed(error: Error, reusable: bool) -> Failure {
    Failure {
        stage: FailureStage::NotCommitted,
        error,
        control_reusable: reusable,
        company_reusable: reusable,
    }
}
fn ledger(db: &Connection) -> Result<String, Error> {
    db.query_row(
        "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
        [],
        |r| r.get(0),
    )
    .map_err(|_| unavailable())
}
fn source(tx: &Transaction<'_>, id: i64, require_pdf: bool) -> Result<Source, Error> {
    if !(1..=9_007_199_254_740_991).contains(&id) {
        return Err(Error::new(
            "invalid_input",
            "Positive safe document ID required",
        ));
    }
    let exists: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM documents WHERE id=?1)",
            [id],
            |r| r.get(0),
        )
        .map_err(|_| unavailable())?;
    if !exists {
        return Err(Error::new(
            "document_not_found",
            "Registered document unavailable",
        ));
    }
    let value = rentemester_storage::documents::evidence(tx, id)?;
    if (require_pdf && value.mime != "application/pdf")
        || value.hash.len() != 64
        || !value
            .hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        || value.path.is_empty()
    {
        return Err(Error::new(
            "EXTRACTION_SNAPSHOT_INVALID",
            "Extraction requires a registered PDF original",
        ));
    }
    Ok(Source {
        path: value.path,
        hash: value.hash,
        filename: value.filename,
        mime: value.mime,
    })
}
fn physical(
    guard: &mut NamespaceGuard,
    source: &Source,
    require_pdf: bool,
) -> Result<Vec<u8>, Error> {
    let bytes = guard
        .read_document(&source.path, &source.hash)
        .map_err(|_| unavailable())?;
    if bytes.is_empty()
        || bytes.len()
            > if require_pdf {
                10 * 1024 * 1024
            } else {
                rentemester_runtime::document::MAX_BYTES
            }
    {
        return Err(Error::new(
            "EXTRACTION_SNAPSHOT_INVALID",
            "Registered source exceeds its admitted size bound",
        ));
    }
    Ok(bytes)
}
fn at(context: &Context<'_>) -> Result<String, Error> {
    rentemester_engine::company_knowledge::timestamp(
        context.clock.now_unix_ms().map_err(|_| unavailable())?,
    )
}
fn actor(subject: &str) -> String {
    format!("user:{subject}")
}

pub fn prepare(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    document_id: i64,
    identity: Option<ProviderIdentity>,
) -> Result<Committed<Admission>, Failure> {
    prepare_inner(
        control,
        company,
        guard,
        context,
        document_id,
        identity,
        None,
        None,
    )
}
/// Internal upload continuation, never an alternate HTTP privilege selector.
pub fn prepare_registered_upload(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    upload: RegisteredUpload,
    identity: Option<ProviderIdentity>,
) -> Result<Committed<Admission>, Failure> {
    let document_id = upload.document_id();
    prepare_inner(
        control,
        company,
        guard,
        context,
        document_id,
        identity,
        Some(upload),
        None,
    )
}
#[allow(clippy::too_many_arguments)]
fn prepare_inner(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    document_id: i64,
    identity: Option<ProviderIdentity>,
    upload: Option<RegisteredUpload>,
    expected_actor: Option<&str>,
) -> Result<Committed<Admission>, Failure> {
    let origin = if upload.is_some() {
        Origin::RegisteredUpload
    } else {
        Origin::Explicit
    };
    let workspace = context.selection.workspace.to_owned();
    let slug = context.selection.slug.to_owned();
    let stamp = at(&context).map_err(|e| failed(e, true))?;
    let value = control_fence::execute_cookie_subject(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Current,
        },
        context.selection,
        origin.permission(),
        |tx, subject| {
            assert_actor(expected_actor, &actor(subject))?;
            let identity = identity.ok_or_else(|| {
                Error::new(
                    "EXTRACTION_PROVIDER_UNAVAILABLE",
                    "Invoice extraction provider is not configured",
                )
            })?;
            let source = source(tx, document_id, true)?;
            let bytes = physical(guard, &source, true)?;
            let ledger = ledger(tx)?;
            if let Some(upload) = upload {
                upload.validate_binding(&workspace, &slug, subject, &ledger, &source.hash)?;
            }
            let prepared =
                store::prepare(tx, document_id, &bytes, &identity, &actor(subject), &stamp)?;
            Ok(Admission {
                origin,
                service_permit: None,
                workspace,
                slug,
                subject: subject.into(),
                ledger,
                source,
                bytes,
                prepared,
            })
        },
    )?;
    if !value.control_reusable || !control.is_autocommit() || !company.is_autocommit() {
        return Err(failed(unavailable(), false));
    }
    Ok(value)
}
pub fn finish(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    admission: Admission,
    completion: Option<Completion>,
) -> Result<Committed<Value>, Failure> {
    if admission.service_permit.is_some() {
        return Err(failed(changed(), true));
    }
    guard
        .ensure_connections(control, company)
        .map_err(|_| failed(unavailable(), false))?;
    let workspace = context.selection.workspace;
    let slug = context.selection.slug;
    let stamp = at(&context).map_err(|e| failed(e, true))?;
    control_fence::execute_cookie_subject(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Current,
        },
        context.selection,
        admission.origin.permission(),
        |tx, subject| {
            finish_tx(
                tx, guard, workspace, slug, subject, admission, completion, &stamp, "user",
            )
        },
    )
}
/// Status never executes a provider, adopts cached state or appends an attempt.
pub fn read(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    document_id: i64,
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
        |tx, _| read_tx(tx, guard, document_id, false),
    )
}


fn assert_actor(expected: Option<&str>, actual: &str) -> Result<(), Error> {
    if expected.is_some_and(|value| value != actual) {
        return Err(Error::new(
            "access_denied",
            "Current actor assertion did not match",
        ));
    }
    Ok(())
}
pub fn prepare_original(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    document_id: i64,
    identity: Option<ProviderIdentity>,
    expected_actor: Option<&str>,
) -> Result<Committed<Admission>, Failure> {
    prepare_inner(
        control,
        company,
        guard,
        context,
        document_id,
        identity,
        None,
        expected_actor,
    )
}
#[allow(clippy::too_many_arguments)]
fn finish_tx(
    tx: &Transaction<'_>,
    guard: &mut NamespaceGuard,
    workspace: &str,
    slug: &str,
    subject: &str,
    admission: Admission,
    completion: Option<Completion>,
    stamp: &str,
    principal: &str,
) -> Result<Value, Error> {
    if workspace != admission.workspace
        || slug != admission.slug
        || subject != admission.subject
        || ledger(tx)? != admission.ledger
    {
        return Err(changed());
    }
    let current = source(tx, admission.prepared.document_id(), true)?;
    if current != admission.source || physical(guard, &current, true)? != admission.bytes {
        return Err(changed());
    }
    store::recheck(tx, &admission.prepared)?;
    let extraction = if let Some(cached) = admission.prepared.cached() {
        if completion.is_some() {
            return Err(Error::new(
                "invalid_input",
                "Cached extraction cannot accept provider output",
            ));
        }
        cached.clone()
    } else {
        let completion = completion
            .ok_or_else(|| Error::new("invalid_input", "Provider completion required"))?;
        let outcome = if &completion.identity == admission.prepared.identity() {
            completion.outcome
        } else {
            Err(ProviderFailure::Unavailable)
        };
        store::finish(
            tx,
            &admission.prepared,
            &outcome,
            &format!("{principal}:{subject}"),
            stamp,
        )?
    };
    Ok(json!({"ok":true,"extraction":extraction}))
}
fn read_tx(
    tx: &Transaction<'_>,
    guard: &mut NamespaceGuard,
    document_id: i64,
    missing_null: bool,
) -> Result<Value, Error> {
    store::validate_history(tx)?;
    let extraction = store::inspect(tx, document_id)?;
    let original = match source(tx, document_id, extraction.is_some()) {
        Err(e) if missing_null && e.code == "document_not_found" && extraction.is_none() => {
            return Ok(json!({"ok":true,"extraction":null}));
        }
        value => value?,
    };
    let bytes = physical(guard, &original, extraction.is_some())?;
    for retained in store::sources(tx)? {
        if retained.document_id == document_id
            && (retained.sha256 != original.hash || retained.pdf_bytes != bytes)
        {
            return Err(changed());
        }
    }
    Ok(json!({"ok":true,"extraction":extraction}))
}
pub fn read_original(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    document_id: i64,
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
        |tx, _| read_tx(tx, guard, document_id, true),
    )
}

pub fn prepare_service(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: crate::service_company::Context<'_>,
    document_id: i64,
    identity: Option<ProviderIdentity>,
    expected_actor: Option<&str>,
) -> Result<crate::service_company::ServiceResult<Admission>, crate::service_company::ServiceFailure>
{
    let workspace = context.selection.workspace.to_owned();
    let slug = context.selection.slug.to_owned();
    let clock = context.clock;
    // Equality-only preflight before metering; the normal fences still establish
    // authority and repeat this assertion before any extraction evidence.
    if let Some(expected) = expected_actor {
        let check = (|| {
            let catalog = rentemester_storage::workspace_context::Catalog {
                workspace: workspace.clone(),
                companies: if context.selection.registered {
                    vec![rentemester_storage::workspace_context::RegisteredCompany {
                        slug: slug.clone(),
                        name: slug.clone(),
                        archived: context.selection.archived,
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
                clock.now_unix_ms().map_err(|_| unavailable())?,
                &catalog,
            )?;
            assert_actor(
                Some(expected),
                &format!("service_account:{}", observed.subject()),
            )?;
            tx.rollback().map_err(|_| unavailable())
        })();
        check.map_err(|error| crate::service_company::ServiceFailure {
            usage: crate::service_company::UsageStage::NotCommitted,
            business: crate::service_company::BusinessStage::NotAttempted,
            error,
            control_reusable: control.is_autocommit(),
            company_reusable: company.is_autocommit(),
            rejection: None,
        })?;
    }
    let result = crate::service_company::precharged::prepare(
        control,
        company,
        context,
        Permission::CompanyMasterData,
        |tx, subject| {
            assert_actor(expected_actor, &format!("service_account:{subject}"))?;
            let stamp = rentemester_engine::company_knowledge::timestamp(
                clock.now_unix_ms().map_err(|_| unavailable())?,
            )?;
            let identity = identity.ok_or_else(|| {
                Error::new(
                    "EXTRACTION_PROVIDER_UNAVAILABLE",
                    "Invoice extraction provider is not configured",
                )
            })?;
            let source = source(tx, document_id, true)?;
            let bytes = physical(guard, &source, true)?;
            let ledger = ledger(tx)?;
            let prepared = store::prepare(
                tx,
                document_id,
                &bytes,
                &identity,
                &format!("service_account:{subject}"),
                &stamp,
            )?;
            Ok(Admission {
                origin: Origin::Explicit,
                service_permit: None,
                workspace,
                slug,
                subject: subject.into(),
                ledger,
                source,
                bytes,
                prepared,
            })
        },
    )?;
    if !result.control_reusable || !control.is_autocommit() || !company.is_autocommit() {
        return Err(service_failed());
    }
    let (mut admission, permit) = result.result;
    admission.service_permit = Some(permit);
    Ok(crate::service_company::ServiceResult {
        result: admission,
        usage: result.usage,
        business: result.business,
        receipt: result.receipt,
        control_reusable: result.control_reusable,
    })
}
fn service_failed() -> crate::service_company::ServiceFailure {
    crate::service_company::ServiceFailure {
        usage: crate::service_company::UsageStage::Committed,
        business: crate::service_company::BusinessStage::NotAttempted,
        error: unavailable(),
        control_reusable: false,
        company_reusable: false,
        rejection: None,
    }
}
pub fn finish_service(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: crate::service_company::Context<'_>,
    mut admission: Admission,
    completion: Option<Completion>,
) -> Result<crate::service_company::ServiceResult<Value>, crate::service_company::ServiceFailure> {
    guard
        .ensure_connections(control, company)
        .map_err(|_| service_failed())?;
    let permit = admission.service_permit.take().ok_or_else(service_failed)?;
    let workspace = context.selection.workspace.to_owned();
    let slug = context.selection.slug.to_owned();
    let clock = context.clock;
    crate::service_company::precharged::finish(control, company, context, permit, |tx, subject| {
        let stamp = rentemester_engine::company_knowledge::timestamp(
            clock.now_unix_ms().map_err(|_| unavailable())?,
        )?;
        finish_tx(
            tx,
            guard,
            &workspace,
            &slug,
            subject,
            admission,
            completion,
            &stamp,
            "service_account",
        )
    })
}
pub fn read_service(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: crate::service_company::Context<'_>,
    document_id: i64,
) -> Result<crate::service_company::ServiceResult<Value>, crate::service_company::ServiceFailure> {
    crate::service_company::execute_operation(
        control,
        company,
        context,
        Permission::CompanyDocumentsRead,
        false,
        |tx, _, _| {
            Ok(crate::service_company::OperationResult {
                result: read_tx(tx, guard, document_id, true)?,
                receipt: None,
            })
        },
    )
}

#[cfg(test)]
mod upload_origin_tests {
    use super::*;
    #[test]
    fn extraction_origin_has_fixed_permissions_not_a_caller_selector() {
        assert_eq!(Origin::Explicit.permission(), Permission::CompanyMasterData);
        assert_eq!(
            Origin::RegisteredUpload.permission(),
            Permission::CompanyDocumentsUpload
        );
    }
}
