//! Explicit registered-PDF evidence. Worker execution belongs between prepare and
//! finish, outside SQL transactions and the adapter's global Owner mutex.
use crate::{
    access::Permission,
    control_fence::{
        self, Committed, CookieAuthorization, Failure, FailureStage, SessionRequirement,
    },
    documents::Context,
};
use rentemester_engine::{
    Error,
    document_pdf::{self as pdf, OperationalError, ParseResult, ParserIdentity},
};
use rentemester_runtime::namespace::NamespaceGuard;
use rentemester_storage::document_pdf as store;
use rusqlite::{Connection, Transaction};
use serde_json::{Value, json};
const PROGRAM: &str = "rentemester-document-pdf";
const MAX_SOURCE: usize = 10 * 1024 * 1024;

/// Runtime-only provenance and output, never request JSON or an HTTP credential.
pub struct Completion {
    pub identity: ParserIdentity,
    pub outcome: Result<ParseResult, OperationalError>,
}
#[derive(PartialEq, Eq)]
struct Source {
    path: String,
    hash: String,
    filename: String,
    mime: String,
}
enum State {
    Ready {
        prepared: Box<store::Prepared>,
        bytes: Vec<u8>,
    },
    Terminal(OperationalError),
}
/// A single owned source/authority observation, not a serializable scan ticket.
pub struct Admission {
    workspace: String,
    slug: String,
    subject: String,
    ledger: String,
    document_id: i64,
    identity: ParserIdentity,
    source: Option<Source>,
    state: State,
    service_permit: Option<crate::service_company::precharged::Permit>,
}
impl Admission {
    pub fn worker_input(&self) -> Option<&[u8]> {
        match &self.state {
            State::Ready { prepared, bytes } if prepared.cached().is_none() => Some(bytes),
            _ => None,
        }
    }
    pub fn identity(&self) -> &ParserIdentity {
        &self.identity
    }
}
pub enum Read {
    Status(i64),
    Text {
        document_id: i64,
        offset: usize,
        limit: usize,
    },
    Pending {
        identity: ParserIdentity,
        cursor: i64,
        limit: usize,
    },
}
fn unavailable() -> Error {
    Error::new(
        "document_pdf_unavailable",
        "Registered PDF evidence unavailable",
    )
}
fn failure(error: Error, reusable: bool) -> Failure {
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
        |row| row.get(0),
    )
    .map_err(|_| unavailable())
}
fn registered(db: &Connection, id: i64) -> Result<(), Error> {
    if id <= 0 || id > 9_007_199_254_740_991 {
        return Err(Error::new(
            "invalid_input",
            "Positive safe document ID required",
        ));
    }
    let exists: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM documents WHERE id=?1)",
            [id],
            |row| row.get(0),
        )
        .map_err(|_| unavailable())?;
    if !exists {
        return Err(Error::new(
            "document_not_found",
            "Registered document unavailable",
        ));
    }
    Ok(())
}
fn source(tx: &Transaction<'_>, id: i64) -> Option<Source> {
    rentemester_storage::documents::evidence(tx, id)
        .ok()
        .map(|value| Source {
            path: value.path,
            hash: value.hash,
            filename: value.filename,
            mime: value.mime,
        })
}
fn physical(
    guard: &mut NamespaceGuard,
    source: Option<&Source>,
) -> Result<Vec<u8>, OperationalError> {
    let source = source.ok_or(OperationalError::SourceInvalid)?;
    if source.mime != "application/pdf" || !pdf::hex(&source.hash) || source.path.is_empty() {
        return Err(OperationalError::SourceInvalid);
    }
    guard
        .read_document(&source.path, &source.hash)
        .map_err(|_| OperationalError::SourceUnavailable)
}
fn stamp(context: &Context<'_>) -> Result<String, Error> {
    let now = context
        .clock
        .now_unix_ms()
        .map_err(|error| Error::new(error.code(), "Trusted PDF clock unavailable"))?;
    rentemester_engine::company_knowledge::timestamp(now)
}
fn failed(code: OperationalError) -> Value {
    json!({"ok":false,"errors":["PDF_PARSE_FAILED"],"errorCode":code.as_str()})
}
fn source_hash(source: Option<&Source>) -> Option<&str> {
    source
        .map(|value| value.hash.as_str())
        .filter(|hash| pdf::hex(hash))
}

/// Success/cache is effect-free. A registered source failure commits one
/// authorized failed attempt and returns a terminal Admission, never worker input.
pub fn prepare(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    document_id: i64,
    identity: ParserIdentity,
) -> Result<Committed<Admission>, Failure> {
    let workspace = context.selection.workspace.to_owned();
    let slug = context.selection.slug.to_owned();
    let at = stamp(&context).map_err(|error| failure(error, true))?;
    let prepared = control_fence::execute_cookie_subject(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Current,
        },
        context.selection,
        Permission::CompanyDocumentsUpload,
        |tx, subject| {
            prepare_tx(
                tx,
                guard,
                document_id,
                identity,
                workspace,
                slug,
                subject,
                &at,
                "user",
            )
        },
    )?;
    if !prepared.control_reusable || !control.is_autocommit() || !company.is_autocommit() {
        return Err(failure(unavailable(), false));
    }
    Ok(prepared)
}
/// Reauthorize and re-prove the registered bytes before retaining either result
/// or operational failure. A stale/hidden source never creates new parse evidence.
pub fn finish(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    admission: Admission,
    completion: Option<Completion>,
) -> Result<Committed<Value>, Failure> {
    if admission.service_permit.is_some() {
        return Err(failure(unavailable(), true));
    }
    guard
        .ensure_connections(control, company)
        .map_err(|_| failure(unavailable(), false))?;
    let workspace = context.selection.workspace;
    let slug = context.selection.slug;
    let at = stamp(&context).map_err(|error| failure(error, true))?;
    control_fence::execute_cookie_subject(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Current,
        },
        context.selection,
        Permission::CompanyDocumentsUpload,
        |tx, subject| {
            finish_tx(
                tx, guard, workspace, slug, subject, admission, completion, &at, "user",
            )
        },
    )
}
/// Read retained canonical evidence only. No execution, failed attempt or cache
/// adoption is hidden in status/text; pending plans only the explicit write batch.
pub fn read(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    action: Read,
) -> Result<Committed<Value>, Failure> {
    let readonly = !matches!(action, Read::Pending { .. });
    let permission = if matches!(action, Read::Pending { .. }) {
        Permission::CompanyDocumentsUpload
    } else {
        Permission::CompanyDocumentsRead
    };
    control_fence::execute_cookie_subject_mode(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Current,
        },
        context.selection,
        permission,
        readonly,
        |tx, _| read_tx(tx, guard, action),
    )
}

#[allow(clippy::too_many_arguments)]
fn prepare_tx(
    tx: &Transaction<'_>,
    guard: &mut NamespaceGuard,
    document_id: i64,
    identity: ParserIdentity,
    workspace: String,
    slug: String,
    subject: &str,
    at: &str,
    principal: &str,
) -> Result<Admission, Error> {
    identity.validate()?;
    registered(tx, document_id)?;
    store::validate_history(tx)?;
    let ledger = ledger(tx)?;
    let source = source(tx, document_id);
    let bytes = physical(guard, source.as_ref()).and_then(|bytes| {
        if bytes.is_empty() || bytes.len() > MAX_SOURCE {
            Err(OperationalError::SourceInvalid)
        } else {
            Ok(bytes)
        }
    });
    let state = match bytes {
        Ok(bytes) => State::Ready {
            prepared: Box::new(store::prepare(
                tx,
                document_id,
                &source.as_ref().ok_or_else(unavailable)?.hash,
                &identity,
            )?),
            bytes,
        },
        Err(code) => {
            store::record_failure(
                tx,
                document_id,
                source_hash(source.as_ref()),
                &identity,
                code,
                &format!("{principal}:{subject}"),
                PROGRAM,
                at,
            )?;
            State::Terminal(code)
        }
    };
    Ok(Admission {
        workspace,
        slug,
        subject: subject.into(),
        ledger,
        document_id,
        identity,
        source,
        state,
        service_permit: None,
    })
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
    at: &str,
    principal: &str,
) -> Result<Value, Error> {
    if workspace != admission.workspace
        || slug != admission.slug
        || subject != admission.subject
        || ledger(tx)? != admission.ledger
    {
        return Err(Error::new(
            "document_pdf_admission_changed",
            "PDF company, subject or ledger changed during processing",
        ));
    }
    registered(tx, admission.document_id)?;
    store::validate_history(tx)?;
    let current = source(tx, admission.document_id);
    if current != admission.source {
        return Err(pdf::tampered());
    }
    match admission.state {
        State::Terminal(code) => {
            if completion.is_some() {
                return Err(pdf::tampered());
            }
            Ok(failed(code))
        }
        State::Ready { prepared, bytes } => {
            let actual = physical(guard, current.as_ref()).map_err(|_| pdf::tampered())?;
            if actual != bytes {
                return Err(pdf::tampered());
            }
            let actor = format!("{principal}:{subject}");
            if prepared.cached().is_some() {
                if completion.is_some() {
                    return Err(pdf::tampered());
                }
                let run = store::record_cached(tx, &prepared, &actor, PROGRAM, at)?;
                return Ok(json!({"ok":true,"parse":run.summary(admission.document_id)}));
            }
            let completion = completion.ok_or_else(|| {
                Error::new("invalid_pdf_completion", "Worker completion required")
            })?;
            if completion.identity != admission.identity {
                let code = OperationalError::ProtocolViolation;
                store::record_failure(
                    tx,
                    admission.document_id,
                    Some(prepared.source_hash()),
                    &admission.identity,
                    code,
                    &actor,
                    PROGRAM,
                    at,
                )?;
                return Ok(failed(code));
            }
            match completion.outcome {
                Ok(result) => {
                    if result
                        .validate(prepared.source_hash(), &admission.identity)
                        .is_err()
                    {
                        let code = OperationalError::ProtocolViolation;
                        store::record_failure(
                            tx,
                            admission.document_id,
                            Some(prepared.source_hash()),
                            &admission.identity,
                            code,
                            &actor,
                            PROGRAM,
                            at,
                        )?;
                        return Ok(failed(code));
                    }
                    let run = store::record_result(tx, &prepared, &result, &actor, PROGRAM, at)?;
                    Ok(json!({"ok":true,"parse":run.summary(admission.document_id)}))
                }
                Err(code) => {
                    store::record_failure(
                        tx,
                        admission.document_id,
                        Some(prepared.source_hash()),
                        &admission.identity,
                        code,
                        &actor,
                        PROGRAM,
                        at,
                    )?;
                    Ok(failed(code))
                }
            }
        }
    }
}

fn read_tx(tx: &Transaction<'_>, guard: &mut NamespaceGuard, action: Read) -> Result<Value, Error> {
    match action {
        Read::Status(id) => {
            registered(tx, id)?;
            physical(guard, source(tx, id).as_ref()).map_err(|_| pdf::tampered())?;
            Ok(json!({"ok":true,"parse":store::status(tx, id)?}))
        }
        Read::Text {
            document_id,
            offset,
            limit,
        } => {
            registered(tx, document_id)?;
            physical(guard, source(tx, document_id).as_ref()).map_err(|_| pdf::tampered())?;
            let mut value = store::text(tx, document_id, offset, limit)?;
            value["ok"] = json!(true);
            Ok(value)
        }
        Read::Pending {
            identity,
            cursor,
            limit,
        } => {
            let mut value = store::pending(tx, &identity, cursor, limit)?;
            let ids = value["documentIds"].as_array().ok_or_else(unavailable)?;
            for id in ids {
                let id = id.as_i64().ok_or_else(unavailable)?;
                registered(tx, id)?;
                physical(guard, source(tx, id).as_ref()).map_err(|_| pdf::tampered())?;
            }
            value["ok"] = json!(true);
            Ok(value)
        }
    }
}

pub fn prepare_service(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: crate::service_company::Context<'_>,
    document_id: i64,
    identity: ParserIdentity,
) -> Result<crate::service_company::ServiceResult<Admission>, crate::service_company::ServiceFailure>
{
    let workspace = context.selection.workspace.to_owned();
    let slug = context.selection.slug.to_owned();
    let clock = context.clock;
    let result = crate::service_company::precharged::prepare(
        control,
        company,
        context,
        Permission::CompanyDocumentsUpload,
        |tx, subject| {
            let at = rentemester_engine::company_knowledge::timestamp(
                clock.now_unix_ms().map_err(|_| unavailable())?,
            )?;
            prepare_tx(
                tx,
                guard,
                document_id,
                identity,
                workspace,
                slug,
                subject,
                &at,
                "service_account",
            )
        },
    )?;
    if !result.control_reusable || !control.is_autocommit() || !company.is_autocommit() {
        return Err(crate::service_company::ServiceFailure {
            usage: crate::service_company::UsageStage::Committed,
            business: crate::service_company::BusinessStage::NotAttempted,
            error: unavailable(),
            control_reusable: false,
            company_reusable: false,
            rejection: None,
        });
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
pub fn finish_service(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: crate::service_company::Context<'_>,
    mut admission: Admission,
    completion: Option<Completion>,
) -> Result<crate::service_company::ServiceResult<Value>, crate::service_company::ServiceFailure> {
    let fail = || crate::service_company::ServiceFailure {
        usage: crate::service_company::UsageStage::Committed,
        business: crate::service_company::BusinessStage::NotAttempted,
        error: unavailable(),
        control_reusable: false,
        company_reusable: false,
        rejection: None,
    };
    guard
        .ensure_connections(control, company)
        .map_err(|_| fail())?;
    let permit = admission.service_permit.take().ok_or_else(fail)?;
    let workspace = context.selection.workspace.to_owned();
    let slug = context.selection.slug.to_owned();
    let clock = context.clock;
    crate::service_company::precharged::finish(control, company, context, permit, |tx, subject| {
        let at = rentemester_engine::company_knowledge::timestamp(
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
            &at,
            "service_account",
        )
    })
}
pub fn read_service(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: crate::service_company::Context<'_>,
    action: Read,
) -> Result<crate::service_company::ServiceResult<Value>, crate::service_company::ServiceFailure> {
    let permission = if matches!(action, Read::Pending { .. }) {
        Permission::CompanyDocumentsUpload
    } else {
        Permission::CompanyDocumentsRead
    };
    crate::service_company::execute_operation(
        control,
        company,
        context,
        permission,
        false,
        |tx, _, _| {
            Ok(crate::service_company::OperationResult {
                result: read_tx(tx, guard, action)?,
                receipt: None,
            })
        },
    )
}
