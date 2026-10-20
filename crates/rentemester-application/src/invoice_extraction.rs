//! Source-bound extraction admission. Provider I/O occurs only between these
//! fixed operations, with neither SQL transactions nor the Owner mutex held.
use crate::{
    access::Permission,
    control_fence::{
        self, Committed, CookieAuthorization, Failure, FailureStage, SessionRequirement,
    },
    documents::Context,
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
pub struct Admission {
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
        Permission::CompanyMasterData,
        |tx, subject| {
            let identity = identity.ok_or_else(|| {
                Error::new(
                    "EXTRACTION_PROVIDER_UNAVAILABLE",
                    "Invoice extraction provider is not configured",
                )
            })?;
            let source = source(tx, document_id, true)?;
            let bytes = physical(guard, &source, true)?;
            let ledger = ledger(tx)?;
            let prepared =
                store::prepare(tx, document_id, &bytes, &identity, &actor(subject), &stamp)?;
            Ok(Admission {
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
        Permission::CompanyMasterData,
        |tx, subject| {
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
                store::finish(tx, &admission.prepared, &outcome, &actor(subject), &stamp)?
            };
            Ok(json!({"ok":true,"extraction":extraction}))
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
        |tx, _| {
            store::validate_history(tx)?;
            let extraction = store::inspect(tx, document_id)?;
            let original = source(tx, document_id, extraction.is_some())?;
            let bytes = physical(guard, &original, extraction.is_some())?;
            for retained in store::sources(tx)? {
                if retained.document_id == document_id
                    && (retained.sha256 != original.hash || retained.pdf_bytes != bytes)
                {
                    return Err(changed());
                }
            }
            Ok(json!({"ok":true,"extraction":extraction}))
        },
    )
}
