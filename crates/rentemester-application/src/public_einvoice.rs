//! Current-authorized public invoice phases. Network calls are made by the
//! adapter between phases, with neither an Owner mutex nor SQL transaction held.
use crate::{
    access::Permission,
    control_fence::{
        self, Committed, CookieAuthorization, Failure, FailureStage, SessionRequirement,
    },
    documents::Context,
};
use rentemester_engine::Error;
use rentemester_runtime::namespace::NamespaceGuard;
use rentemester_storage::public_einvoice::{self as store, Identity, Source};
use rusqlite::{Connection, Transaction};
use serde_json::{Value, json};

pub enum Intent {
    Setup,
    Send(i64),
    OriginalSend(Value),
    Status(i64),
}
pub enum Next {
    Complete(Value),
    Pending(Box<Admission>),
}
/// No request decoder, Clone or public constructor: only admitted operations
/// can advance to a provider call or commit the matching result.
pub struct Admission {
    workspace: String,
    slug: String,
    subject: String,
    ledger: String,
    environment: String,
    original: bool,
    step: Step,
}
enum Step {
    FindCompany(Identity),
    FindOutbound(Identity, String),
    Company(Identity),
    Outbound(Identity, String),
    Validate(Source),
    Deliver(Source, i64),
    Observe(Source, i64, String),
}
pub enum Request<'a> {
    FindCompany {
        cvr: &'a str,
        name: &'a str,
    },
    FindOutbound {
        cvr: &'a str,
        name: &'a str,
        company_key: &'a str,
    },
    Company {
        cvr: &'a str,
        name: &'a str,
    },
    Outbound {
        cvr: &'a str,
        name: &'a str,
        company_key: &'a str,
    },
    Validate {
        xml: &'a str,
    },
    Deliver {
        xml: &'a str,
        company_key: &'a str,
    },
    Observe {
        company_key: &'a str,
        document_id: &'a str,
    },
}
pub struct Observation {
    pub document_id: Option<String>,
    pub status: String,
}
pub enum Discovery<T> {
    Found(T),
    Missing,
    Failed,
}
pub enum Outcome {
    CompanyFound(Discovery<String>),
    OutboundFound(Discovery<()>),
    Company(Option<String>),
    Outbound(bool),
    Validated(bool),
    Delivered(Option<Observation>),
    Observed(Option<Observation>),
}
impl Admission {
    pub fn request(&self) -> Request<'_> {
        match &self.step {
            Step::FindCompany(i) => Request::FindCompany {
                cvr: &i.cvr,
                name: &i.company_name,
            },
            Step::FindOutbound(i, k) => Request::FindOutbound {
                cvr: &i.cvr,
                name: &i.company_name,
                company_key: k,
            },
            Step::Company(i) => Request::Company {
                cvr: &i.cvr,
                name: &i.company_name,
            },
            Step::Outbound(i, k) => Request::Outbound {
                cvr: &i.cvr,
                name: &i.company_name,
                company_key: k,
            },
            Step::Validate(s) => Request::Validate { xml: &s.oioubl_xml },
            Step::Deliver(s, _) => Request::Deliver {
                xml: &s.oioubl_xml,
                company_key: &s.company_key,
            },
            Step::Observe(s, _, id) => Request::Observe {
                company_key: &s.company_key,
                document_id: id,
            },
        }
    }
}
fn refused() -> Error {
    Error::new(
        "public_einvoice_refused",
        "Current public invoice identity, source or provider evidence unavailable",
    )
}
fn fail(error: Error) -> Failure {
    Failure {
        stage: FailureStage::NotCommitted,
        error,
        control_reusable: true,
        company_reusable: true,
    }
}
fn ledger(db: &Connection) -> Result<String, Error> {
    db.query_row(
        "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
        [],
        |r| r.get(0),
    )
    .map_err(|_| refused())
}
fn selection<'a>(context: &Context<'a>) -> control_fence::CompanySelection<'a> {
    control_fence::CompanySelection {
        workspace: context.selection.workspace,
        slug: context.selection.slug,
        registered: context.selection.registered,
        archived: context.selection.archived,
    }
}
fn at(context: &Context<'_>) -> Result<String, Error> {
    rentemester_engine::company_knowledge::timestamp(
        context.clock.now_unix_ms().map_err(|_| refused())?,
    )
}
fn physical(tx: &Transaction<'_>, guard: &mut NamespaceGuard, s: &Source) -> Result<(), Error> {
    crate::sales::verify_invoice(tx, guard, s.invoice_document_id)?;
    let (path,hash,payload):(String,String,String)=tx.query_row("SELECT stored_path,sha256_hash,payload_json FROM documents WHERE id=?1 AND document_type='issued_invoice'",[s.invoice_document_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(|_|refused())?;
    if hash != s.json_sha256 {
        return Err(refused());
    }
    let bytes = guard.read_document(&path, &hash).map_err(|_| refused())?;
    let original: Value = serde_json::from_slice(&bytes).map_err(|_| refused())?;
    let current: Value = serde_json::from_str(&payload).map_err(|_| refused())?;
    if original != current {
        return Err(refused());
    }
    let rebuilt = store::load_source(tx, s.invoice_document_id, &s.company_key, &s.environment)?;
    if rebuilt != *s {
        return Err(refused());
    }
    Ok(())
}
fn admitted(
    context: &Context<'_>,
    actor: &str,
    ledger: String,
    environment: &str,
    original: bool,
    step: Step,
) -> Admission {
    Admission {
        workspace: context.selection.workspace.into(),
        slug: context.selection.slug.into(),
        subject: actor.into(),
        ledger,
        environment: environment.into(),
        original,
        step,
    }
}
fn pending(a: Admission, step: Step) -> Next {
    Next::Pending(Box::new(Admission { step, ..a }))
}
/// Configuration lives only in the server operator boundary; environment is
/// read from that immutable configuration, never from a browser body.
pub fn start(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    intent: Intent,
    environment: &str,
) -> Result<Committed<Next>, Failure> {
    if !matches!(environment, "test" | "production") {
        return Err(fail(refused()));
    }
    let original = matches!(&intent, Intent::OriginalSend(_));
    control_fence::execute_cookie_subject(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Fresh,
        },
        selection(&context),
        Permission::CompanyExternalSend,
        |tx, actor| {
            let ledger = ledger(tx)?;
            let step = match intent {
                Intent::Setup => {
                    let identity = store::identity(tx)?;
                    match store::registration(tx, environment)? {
                        store::Registration::Bound { company_key: key }
                            if store::outbound_registered(tx, environment, &key)? =>
                        {
                            return Ok(Next::Complete(
                                json!({"ok":true,"registration":{"registered":true,"outbound":true,"environment":environment}}),
                            ));
                        }
                        store::Registration::Bound { company_key: key } => {
                            Step::FindOutbound(identity, key)
                        }
                        store::Registration::Missing | store::Registration::Unverified { .. } => {
                            Step::FindCompany(identity)
                        }
                    }
                }
                Intent::Send(id) => match send_step(tx, guard, id, environment, false)? {
                    Ok(step) => step,
                    Err(value) => return Ok(Next::Complete(value)),
                },
                Intent::OriginalSend(body) => {
                    let id = original_id(tx, &body)?;
                    if let Some(assertion) = body.get("companyKey") {
                        let key = store::load_bound(tx, environment)?.ok_or_else(refused)?;
                        if assertion.as_str() != Some(key.as_str()) {
                            return Err(refused());
                        }
                    }
                    crate::sales::verify_invoice(tx, guard, id)?;
                    match send_step(tx, guard, id, environment, true)? {
                        Ok(step) => step,
                        Err(value) => return Ok(Next::Complete(value)),
                    }
                }
                Intent::Status(id) => {
                    let (row_id, source, document_id) =
                        store::accepted(tx, id)?.ok_or_else(refused)?;
                    if source.environment != environment {
                        return Err(refused());
                    }
                    physical(tx, guard, &source)?;
                    Step::Observe(source, row_id, document_id)
                }
            };
            Ok(Next::Pending(Box::new(admitted(
                &context,
                actor,
                ledger,
                environment,
                original,
                step,
            ))))
        },
    )
}
pub fn finish(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    admission: Admission,
    outcome: Outcome,
) -> Result<Committed<Next>, Failure> {
    let delivery = matches!(admission.step, Step::Deliver(..));
    let result = control_fence::execute_cookie_subject(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Fresh,
        },
        selection(&context),
        Permission::CompanyExternalSend,
        |tx, actor| {
            if admission.workspace != context.selection.workspace
                || admission.slug != context.selection.slug
                || admission.subject != actor
                || admission.ledger != ledger(tx)?
            {
                return Err(refused());
            }
            let now = at(&context)?;
            match (&admission.step, outcome) {
                (Step::FindCompany(identity), Outcome::CompanyFound(Discovery::Found(key))) => {
                    store::save_key(tx, identity, &admission.environment, &key, actor, &now)?;
                    let identity = store::identity(tx)?;
                    Ok(pending(admission, Step::FindOutbound(identity, key)))
                }
                (Step::FindCompany(identity), Outcome::CompanyFound(Discovery::Missing)) => {
                    if store::identity(tx)? != *identity {
                        return Err(refused());
                    }
                    let identity = identity.clone();
                    Ok(pending(admission, Step::Company(identity)))
                }
                (Step::FindOutbound(identity, key), Outcome::OutboundFound(Discovery::Missing)) => {
                    if store::identity(tx)? != *identity
                        || store::load_bound(tx, &admission.environment)?.as_deref()
                            != Some(key.as_str())
                    {
                        return Err(refused());
                    }
                    let (identity, key) = (identity.clone(), key.clone());
                    Ok(pending(admission, Step::Outbound(identity, key)))
                }
                (
                    Step::FindOutbound(identity, key),
                    Outcome::OutboundFound(Discovery::Found(())),
                ) => {
                    store::save_outbound(
                        tx,
                        identity,
                        &admission.environment,
                        key,
                        true,
                        false,
                        actor,
                        &now,
                    )?;
                    Ok(Next::Complete(
                        json!({"ok":true,"registration":{"registered":true,"outbound":true,"environment":admission.environment}}),
                    ))
                }
                (Step::Company(identity), Outcome::Company(Some(key))) => {
                    store::save_key(tx, identity, &admission.environment, &key, actor, &now)?;
                    let identity = store::identity(tx)?;
                    Ok(pending(admission, Step::FindOutbound(identity, key)))
                }
                (Step::Outbound(identity, key), Outcome::Outbound(true)) => {
                    store::save_outbound(
                        tx,
                        identity,
                        &admission.environment,
                        key,
                        true,
                        false,
                        actor,
                        &now,
                    )?;
                    Ok(Next::Complete(
                        json!({"ok":true,"registration":{"registered":true,"outbound":true,"environment":admission.environment}}),
                    ))
                }
                (Step::Validate(source), Outcome::Validated(true)) => {
                    physical(tx, guard, source)?;
                    if !store::outbound_registered(tx, &admission.environment, &source.company_key)?
                    {
                        return Err(refused());
                    }
                    let reserved = store::reserve(tx, source, actor, &now)?;
                    if !reserved.should_deliver {
                        if admission.original && !reserved.result["providerStatus"].is_string() {
                            return Err(blocked());
                        }
                        return Ok(Next::Complete(
                            json!({"ok":true,"submission":reserved.result}),
                        ));
                    }
                    let source = source.clone();
                    Ok(pending(admission, Step::Deliver(source, reserved.id)))
                }
                (Step::Deliver(source, id), Outcome::Delivered(observation)) => {
                    physical(tx, guard, source)?;
                    let observation = observation.map(|o| store::Observed {
                        document_id: o.document_id,
                        status: o.status,
                    });
                    let mut row = match observation {
                        Some(o) => store::record_outcome(tx, *id, o, actor, &now)?,
                        None => store::get(tx, source.invoice_document_id)?.ok_or_else(refused)?,
                    };
                    row["duplicate"] = json!(false);
                    Ok(Next::Complete(json!({"ok":true,"submission":row})))
                }
                (Step::Observe(source, id, expected), Outcome::Observed(Some(observation))) => {
                    physical(tx, guard, source)?;
                    if observation.document_id.as_deref() != Some(expected.as_str()) {
                        return Err(refused());
                    }
                    let row = store::record_outcome(
                        tx,
                        *id,
                        store::Observed {
                            document_id: observation.document_id,
                            status: observation.status,
                        },
                        actor,
                        &now,
                    )?;
                    Ok(Next::Complete(json!({"ok":true,"submission":row})))
                }
                _ => Err(Error::new(
                    "public_einvoice_provider_refused",
                    "Provider operation unsuccessful; delivery is never retried automatically",
                )),
            }
        },
    );
    result.map_err(|mut f| {
        if delivery {
            f.stage = FailureStage::OutcomeUnknown;
        }
        f
    })
}

pub fn configuration(
    control: &mut Connection,
    company: &mut Connection,
    context: Context<'_>,
    environment: Option<&str>,
) -> Result<Committed<Value>, Failure> {
    control_fence::execute_cookie_subject(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Current,
        },
        selection(&context),
        Permission::CompanyRead,
        |tx, _| {
            let key = match environment {
                Some(e) => match store::registration(tx, e)? {
                    store::Registration::Bound { company_key } => Some(company_key),
                    _ => None,
                },
                None => None,
            };
            let outbound = match (environment, &key) {
                (Some(e), Some(k)) => store::outbound_registered(tx, e, k)?,
                _ => false,
            };
            Ok(
                json!({"ok":true,"publicEInvoice":{"configured":environment.is_some(),"environment":environment,"registered":key.is_some(),"outbound":outbound}}),
            )
        },
    )
}

pub fn submissions(
    control: &mut Connection,
    company: &mut Connection,
    context: Context<'_>,
) -> Result<Committed<Value>, Failure> {
    control_fence::execute_cookie_subject(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Current,
        },
        selection(&context),
        Permission::CompanyRead,
        |tx, _| {
            store::validate_backup(tx)?;
            Ok(json!({"ok":true,"rows":store::rows(tx)?["submissions"]}))
        },
    )
}

#[derive(Clone, Copy)]
pub enum OriginalAction {
    Preview,
    OioUbl,
    Prepare,
}
/// Original artifacts use physical issued JSON/PDF, independently of provider configuration.
pub fn original(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    action: OriginalAction,
    body: &Value,
) -> Result<Committed<Value>, Failure> {
    let preparing = matches!(action, OriginalAction::Prepare);
    control_fence::execute_cookie_subject_mode(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: if preparing {
                SessionRequirement::Fresh
            } else {
                SessionRequirement::Current
            },
        },
        selection(&context),
        if preparing {
            Permission::CompanyExternalSend
        } else {
            Permission::CompanyDocumentsRead
        },
        matches!(action, OriginalAction::Preview),
        |tx, actor| {
            let id = original_id(tx, body)?;
            crate::sales::verify_invoice(tx, guard, id)?;
            let (no,raw):(String,String)=tx.query_row("SELECT invoice_no,payload_json FROM documents WHERE id=? AND document_type='issued_invoice'",[id],|r|Ok((r.get(0)?,r.get(1)?))).map_err(|_|refused())?;
            let payload: Value = serde_json::from_str(&raw).map_err(|_| refused())?;
            if preparing {
                if body["confirm"] != true {
                    return Err(refused());
                }
                let access =
                    serde_json::from_value(body["accessPoint"].clone()).map_err(|_| refused())?;
                let ack = body
                    .get("acknowledgement")
                    .cloned()
                    .map(serde_json::from_value)
                    .transpose()
                    .map_err(|_| refused())?;
                return store::prepare_original(
                    tx,
                    id,
                    &access,
                    ack.as_ref(),
                    actor,
                    &at(&context)?,
                );
            }
            let preview = matches!(action, OriginalAction::Preview);
            let export = if preview {
                rentemester_engine::public_einvoice_preview::export(&no, &payload)?
            } else {
                rentemester_engine::public_einvoice::export(&no, &payload)?
            };
            if !preview {
                tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor,created_at)VALUES('public_einvoice_oioubl_export','document',?,?,?,?)",rusqlite::params![id.to_string(),json!({"invoiceNumber":no,"sha256":export.sha256}).to_string(),actor,at(&context)?]).map_err(|_|refused())?;
            }
            Ok(
                json!({"ok":true,"invoiceNumber":no,"sha256":export.sha256,"xml":export.xml,"appliedRules":[if preview{"DK-INVOICE-PUBLIC-EXPORT-001"}else{"DK-INVOICE-PUBLIC-OIOUBL-001"}],"errors":[]}),
            )
        },
    )
}
fn original_id(tx: &Transaction<'_>, body: &Value) -> Result<i64, Error> {
    rentemester_storage::invoice_lifecycle::resolve_invoice(tx, body, "documentId", "invoiceNumber")
}
fn send_step(
    tx: &Transaction<'_>,
    guard: &mut NamespaceGuard,
    id: i64,
    environment: &str,
    original: bool,
) -> Result<Result<Step, Value>, Error> {
    if let Some(row) = store::get(tx, id)? {
        if original && !row["providerStatus"].is_string() {
            return Err(blocked());
        }
        // Repeated original transmit may observe an already accepted queued ID once.
        // Terminal/uncertain/legacy/manual states are cached and never redelivered.
        if original && row["legacy"] == false && row["providerStatus"] == "queued-for-delivery" {
            let (row_id, source, document_id) = store::accepted(tx, id)?.ok_or_else(refused)?;
            if source.environment != environment {
                return Err(refused());
            }
            physical(tx, guard, &source)?;
            return Ok(Ok(Step::Observe(source, row_id, document_id)));
        }
        return Ok(Err(json!({"ok":true,"submission":row})));
    }
    let key = store::load_bound(tx, environment)?.ok_or_else(refused)?;
    if !store::outbound_registered(tx, environment, &key)? {
        return Err(refused());
    }
    let source = store::load_source(tx, id, &key, environment)?;
    physical(tx, guard, &source)?;
    Ok(Ok(Step::Validate(source)))
}

fn blocked() -> Error {
    Error::new(
        "public_einvoice_redelivery_blocked",
        "An existing manual or historical handoff blocks delivery and is not native provider evidence",
    )
}
