//! Fixed admitted archive installation and source-backed reads. No legal-document registration.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
};
use rentemester_engine::Error;
use rentemester_runtime::{document::Prepared, namespace::NamespaceGuard};
use rentemester_storage::{
    archive_import as store,
    idempotency::{self, Operation, Outcome, PrincipalKind},
};
use rusqlite::Connection;
use serde_json::{Value, json};
#[derive(Clone, Copy)]
pub enum Action<'a> {
    Plan(&'a Prepared),
    Apply(&'a Prepared, &'a store::Apply),
    List,
    Year(u16),
    Original(i64),
    FiscalYears { today: &'a str },
}
pub enum Output {
    Json(Value),
    Original(Vec<u8>),
}
fn unavailable() -> Error {
    Error::new(
        "archive_source_unavailable",
        "Archive original unavailable or source assurance differs",
    )
}
/// Immutable historical assurance only: current profile, eligibility and old authority are not proof.
pub fn verify_physical(
    db: &Connection,
    guard: &mut NamespaceGuard,
) -> Result<Vec<store::VerifiedArchive>, Error> {
    store::validate_history(db)?;
    let mut verified = vec![];
    for reference in store::originals(db)? {
        let bytes = guard
            .read_document(&reference.path, &reference.sha256)
            .map_err(|_| unavailable())?;
        verified.push(store::verify_original(db, &reference, &bytes)?);
    }
    Ok(verified)
}
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    action: Action<'_>,
) -> Result<Committed<Output>, Failure> {
    let readonly = !matches!(action, Action::Apply(..));
    let permission = match action {
        Action::Apply(..) => Permission::CompanyAdmin,
        Action::Original(_) => Permission::CompanyDocumentsRead,
        _ => Permission::CompanyRead,
    };
    let clock = context.clock;
    let slug = context.selection.slug;
    control_fence::execute_cookie_subject_mode(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock,
            requirement: SessionRequirement::Current,
        },
        context.selection,
        permission,
        readonly,
        |tx, subject| {
            let verified = verify_physical(tx, guard)?;
            match action {
                Action::List => store::read(tx, &verified, None).map(Output::Json),
                Action::FiscalYears { today } => {
                    let mut live = rentemester_storage::cockpit::read(tx, slug, today, None, None)?
                        .fiscal_years;
                    let archived = store::read(tx, &verified, None)?;
                    let years = live["years"].as_array_mut().ok_or_else(unavailable)?;
                    for year in archived["years"].as_array().ok_or_else(unavailable)? {
                        let label = year["fiscalYear"]
                            .as_u64()
                            .ok_or_else(unavailable)?
                            .to_string();
                        years.push(json!({"label":label,"start":null,"end":null,"source":"archive","selectable":false}));
                    }
                    Ok(Output::Json(json!({"ok":true,"fiscalYears":live})))
                }
                Action::Year(year) => store::read(tx, &verified, Some(year)).map(Output::Json),
                Action::Original(id) => {
                    if id <= 0 || id > 9_007_199_254_740_991 {
                        return Err(Error::new(
                            "archive_import_invalid",
                            "Invalid archive import ID",
                        ));
                    }
                    let reference = store::originals(tx)?
                        .into_iter()
                        .find(|r| r.import_id == id)
                        .ok_or_else(|| {
                            Error::new("archive_not_found", "Archive original not found")
                        })?;
                    let bytes = guard
                        .read_document(&reference.path, &reference.sha256)
                        .map_err(|_| unavailable())?;
                    store::verify_original(tx, &reference, &bytes)?;
                    // Native CompanyDocumentsRead roles also require CompanyRead; no workspace-owner bypass.
                    Ok(Output::Original(bytes))
                }
                Action::Plan(file) | Action::Apply(file, _) => {
                    if file.mime() != "application/json" {
                        return Err(Error::new(
                            "archive_import_invalid",
                            "Archive source must be an original JSON package",
                        ));
                    }
                    let (source, parsed) = store::decode_source(file.bytes())?;
                    if source.path != file.relative_path()
                        || source.sha256 != file.hash()
                        || source.size_bytes != file.bytes().len() as u64
                    {
                        return Err(unavailable());
                    }
                    match action {
                        Action::Plan(_) => Ok(Output::Json(
                            json!({"ok":true,"plan":store::prepare(tx,&source,&parsed)?.review()}),
                        )),
                        Action::Apply(_, input) => {
                            if !input.confirm {
                                return Err(Error::new(
                                    "archive_import_invalid",
                                    "Explicit confirmation required",
                                ));
                            }
                            idempotency::validate_key(&input.idempotency_key)?;
                            let payload = json!({"sourceSha256":source.sha256,"sizeBytes":source.size_bytes,"planHash":input.plan_hash,"confirm":true});
                            let now = clock.now_unix_ms().map_err(|_| {
                                Error::new(
                                    "invalid_auth_clock",
                                    "Trusted archive clock unavailable",
                                )
                            })?;
                            let actor = format!("user:{subject}");
                            match idempotency::execute(
                                tx,
                                idempotency::Input {
                                    key: &input.idempotency_key,
                                    operation: Operation::ClosedHistoryArchiveImport,
                                    principal_kind: PrincipalKind::User,
                                    subject,
                                    payload: &payload,
                                    created_by: &actor,
                                    created_by_program: "closed-history-archive-import",
                                    now_ms: now,
                                },
                                |tx| {
                                    let prepared = store::prepare(tx, &source, &parsed)?;
                                    if prepared.review()["planHash"] != input.plan_hash {
                                        return Err(Error::new(
                                            "archive_plan_changed",
                                            "Archive source or current review facts changed",
                                        ));
                                    }
                                    let path =
                                        guard.publish_document(file).map_err(|_| unavailable())?;
                                    if path != source.path {
                                        return Err(unavailable());
                                    }
                                    store::apply(tx, &prepared, input, subject, now)
                                },
                            )
                            .map_err(|f| f.error)?
                            {
                                Outcome::Accepted {
                                    mut result,
                                    receipt,
                                } => {
                                    result["idempotency"] =
                                        serde_json::to_value(receipt).map_err(|_| unavailable())?;
                                    Ok(Output::Json(result))
                                }
                                Outcome::Refused { code } => Err(Error::new(
                                    code,
                                    "Archive request key refused; read the canonical retained import before retrying",
                                )),
                            }
                        }
                        _ => unreachable!(),
                    }
                }
            }
        },
    )
}
