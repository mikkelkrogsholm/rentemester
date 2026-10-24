//! Current-authorized inbound phases. Provider I/O occurs strictly between calls.
use crate::{
    access::Permission,
    control_fence::{
        self, Committed, CookieAuthorization, Failure, FailureStage, SessionRequirement,
    },
    documents::Context,
};
use rentemester_engine::{Error, digisense_received as convert};
use rentemester_runtime::{document::Prepared, namespace::NamespaceGuard};
use rentemester_storage::{digisense_received as received, public_einvoice as store};
use rusqlite::{Connection, Transaction};
use serde_json::{Value, json};

/// Consumed capability; never deserialize, clone, persist or log this value.
pub struct Admission {
    workspace: String,
    slug: String,
    subject: String,
    ledger: String,
    identity: store::Identity,
    company_key: String,
    environment: String,
    generation: [u8; 32],
    changes: u64,
    data_version: i64,
    record: Option<Value>,
}
impl Admission {
    pub fn company_key(&self) -> &str {
        &self.company_key
    }
    pub fn environment(&self) -> &str {
        &self.environment
    }
}
pub struct Checked {
    pub admission: Admission,
    pub known: bool,
}
pub struct Finished {
    pub admission: Admission,
    pub document: Value,
}
fn refused() -> Error {
    Error::new(
        "digisense_received_admission_changed",
        "Current receive authority, binding or evidence unavailable",
    )
}
fn failure(error: Error) -> Failure {
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
fn witness(db: &Connection) -> Result<(u64, i64), Error> {
    Ok((
        db.total_changes(),
        db.pragma_query_value(None, "data_version", |r| r.get(0))
            .map_err(|_| refused())?,
    ))
}
fn refresh(
    mut result: Committed<Admission>,
    db: &Connection,
) -> Result<Committed<Admission>, Failure> {
    if !result.control_reusable || !db.is_autocommit() {
        return Err(failure(refused()));
    }
    let (changes, data_version) = witness(db).map_err(failure)?;
    result.value.changes = changes;
    result.value.data_version = data_version;
    Ok(result)
}
fn binding(
    a: &Admission,
    tx: &Transaction<'_>,
    workspace: &str,
    slug: &str,
    subject: &str,
    environment: &str,
    generation: [u8; 32],
) -> Result<(), Error> {
    if a.workspace != workspace
        || a.slug != slug
        || a.subject != subject
        || a.environment != environment
        || a.generation != generation
        || a.ledger != ledger(tx)?
        || a.identity != store::identity(tx)?
        || store::registration(tx, environment)?
            != (store::Registration::Bound {
                company_key: a.company_key.clone(),
            })
        || witness(tx)? != (a.changes, a.data_version)
    {
        return Err(refused());
    }
    Ok(())
}
fn physical(
    guard: &mut NamespaceGuard,
    control: &Connection,
    company: &Connection,
) -> Result<(), Failure> {
    guard
        .ensure_connections(control, company)
        .map_err(|_| Failure {
            stage: FailureStage::NotCommitted,
            error: refused(),
            control_reusable: false,
            company_reusable: false,
        })
}
/// `environment` and `generation` come only from the operator-owned live client.
pub fn prepare(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    environment: &str,
    generation: [u8; 32],
    company_key_assertion: Option<&str>,
) -> Result<Committed<Admission>, Failure> {
    physical(guard, control, company)?;
    let workspace = context.selection.workspace.to_owned();
    let slug = context.selection.slug.to_owned();
    let result = control_fence::execute_cookie_subject(
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
            let identity = store::identity(tx)?;
            let store::Registration::Bound { company_key } = store::registration(tx, environment)?
            else {
                return Err(refused());
            };
            if company_key_assertion.is_some_and(|k| k != company_key) {
                return Err(refused());
            }
            Ok(Admission {
                workspace,
                slug,
                subject: subject.into(),
                ledger: ledger(tx)?,
                identity,
                company_key,
                environment: environment.into(),
                generation,
                changes: 0,
                data_version: 0,
                record: None,
            })
        },
    )?;
    refresh(result, company)
}
/// Check the stable ID before downloading. Only unknown records yield a record-bound capability.
pub fn check(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    mut admission: Admission,
    environment: &str,
    generation: [u8; 32],
    record: Value,
) -> Result<Committed<Checked>, Failure> {
    physical(guard, control, company)?;
    let workspace = context.selection.workspace;
    let slug = context.selection.slug;
    let result = control_fence::execute_cookie_subject(
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
            binding(
                &admission,
                tx,
                workspace,
                slug,
                subject,
                environment,
                generation,
            )?;
            if admission.record.is_some() {
                return Err(refused());
            }
            let known = received::known(tx, received::internal_id(&record)?)?;
            if !known {
                admission.record = Some(record);
            }
            Ok((admission, known))
        },
    )?;
    let known = result.value.1;
    let refreshed = refresh(
        Committed {
            value: result.value.0,
            control_reusable: result.control_reusable,
        },
        company,
    )?;
    Ok(Committed {
        value: Checked {
            admission: refreshed.value,
            known,
        },
        control_reusable: refreshed.control_reusable,
    })
}
/// Submit only completed downloads. Transient download failure simply drops the capability.
#[allow(clippy::too_many_arguments)]
pub fn finish(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    mut admission: Admission,
    environment: &str,
    generation: [u8; 32],
    xml: Vec<u8>,
    overrides: Value,
    force_duplicate_logical_identity: bool,
) -> Result<Committed<Finished>, Failure> {
    physical(guard, control, company)?;
    let workspace = context.selection.workspace;
    let slug = context.selection.slug;
    let result = control_fence::execute_cookie_subject(
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
            binding(
                &admission,
                tx,
                workspace,
                slug,
                subject,
                environment,
                generation,
            )?;
            let record = admission.record.take().ok_or_else(refused)?;
            let internal_id = received::internal_id(&record)?.to_owned();
            if received::known(tx, &internal_id)? {
                return Ok((
                    admission,
                    json!({"internalId":internal_id,"status":"skipped-duplicate"}),
                ));
            }
            // All physical/source gates precede content classification: access and namespace
            // failures must never poison stable-ID dedup with a terminal content result.
            rentemester_storage::documents::validate_intake(tx)?;
            let destination = convert::validate_destination(&record, &xml, &admission.identity.cvr);
            if let Err(error) = &destination {
                if error.code == "digisense_received_wrong_company" {
                    return Err(refused());
                }
            }
            let metadata = convert::build_received_metadata(&record, &xml, &overrides);
            let mut reason = if destination.is_err() {
                Some(received::TerminalReason::InvalidXml)
            } else {
                None
            };
            let mut document_no = None;
            let mut document_id = None;
            if reason.is_none() {
                match metadata {
                    Err(_) => reason = Some(received::TerminalReason::InvalidMetadata),
                    Ok(metadata) => {
                        if metadata.validate().is_err() {
                            reason = Some(received::TerminalReason::InvalidMetadata);
                        } else {
                            // Filename is fixed and never contains listing IDs, URLs or paths.
                            let file = Prepared::new("digisense-received.xml".into(), xml)
                                .map_err(|_| refused())?;
                            if let Some(message) =
                                rentemester_storage::documents_original::intake_duplicate(
                                    tx,
                                    file.hash(),
                                    &metadata,
                                    force_duplicate_logical_identity,
                                )?
                            {
                                reason =
                                    Some(if message.starts_with("duplicate document content") {
                                        received::TerminalReason::DuplicateContent
                                    } else {
                                        received::TerminalReason::DuplicateLogicalIdentity
                                    });
                            } else {
                                let today = rentemester_runtime::posting_time::latest_date(
                                    context.clock.now_unix_ms().map_err(|_| refused())?,
                                    0,
                                )
                                .map_err(|_| refused())?;
                                let path = guard.publish_document(&file).map_err(|_| refused())?;
                                let receipt = rentemester_storage::documents::register_original(
                                    tx,
                                    rentemester_storage::documents::Original {
                                        filename: file.filename(),
                                        hash: file.hash(),
                                        path: &path,
                                        mime: file.mime(),
                                    },
                                    &metadata,
                                    subject,
                                    &today,
                                    None,
                                    rentemester_storage::idempotency::PrincipalKind::User,
                                    force_duplicate_logical_identity,
                                )?;
                                document_id = receipt["document"]["id"].as_i64();
                                if document_id.is_none() {
                                    return Err(refused());
                                }
                                document_no = receipt["document"]["documentNo"]
                                    .as_str()
                                    .map(str::to_owned);
                                rentemester_storage::unmatched_bank::sync(tx)?;
                            }
                        }
                    }
                }
            }
            received::append(
                tx,
                &admission.company_key,
                &record,
                document_id,
                reason,
                subject,
            )?;
            let document = if let Some(reason) = reason {
                json!({"internalId":internal_id,"status":"quarantined","errors":[reason.code()]})
            } else {
                json!({"internalId":internal_id,"status":"ingested","documentNo":document_no})
            };
            Ok((admission, document))
        },
    )?;
    let document = result.value.1;
    let refreshed = refresh(
        Committed {
            value: result.value.0,
            control_reusable: result.control_reusable,
        },
        company,
    )?;
    Ok(Committed {
        value: Finished {
            admission: refreshed.value,
            document,
        },
        control_reusable: refreshed.control_reusable,
    })
}
