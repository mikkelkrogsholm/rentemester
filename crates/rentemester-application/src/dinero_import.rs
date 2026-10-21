//! Reviewed live Dinero source acceptance under current human authority.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
};
use rentemester_engine::{Error, fiscal_year::Date};
use rentemester_runtime::{document::Prepared, namespace::NamespaceGuard};
use rentemester_storage::{
    dinero_import as store,
    idempotency::{self, Operation, Outcome, PrincipalKind},
};
use rusqlite::Connection;
use serde_json::{Value, json};
#[derive(Clone, Copy)]
pub enum Action<'a> {
    Plan(&'a Prepared),
    Apply(&'a Prepared, &'a store::Apply),
    List,
    Original(i64),
    ImportedReceivables(&'a str),
}
pub enum Output {
    Json(Value),
    Imported(Value),
    Original(Vec<u8>),
}
fn unavailable() -> Error {
    Error::new(
        "dinero_source_unavailable",
        "Dinero source original unavailable or retained assurance differs",
    )
}
/// Checks immutable accepted source facts and exact retained bytes, including receipt replays.
pub fn verify_physical(
    db: &Connection,
    guard: &mut NamespaceGuard,
) -> Result<Vec<store::VerifiedImport>, Error> {
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
    // CompanyAdmin is a financial operation here: the normal fence performs current
    // authorization, transfer exclusion and the current financial-backup check.
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
            // Earlier closed-year archive sources are part of live acceptance assurance.
            crate::archive_import::verify_physical(tx, guard)?;
            match action {
                Action::List => store::read_imports(tx, &verified).map(Output::Json),
                Action::ImportedReceivables(as_of) => {
                    store::read_imports(tx, &verified)?;
                    rentemester_storage::imported_receivables::read(tx, as_of).map(Output::Imported)
                }
                Action::Original(id) => {
                    if id <= 0 || id > 9_007_199_254_740_991 {
                        return Err(Error::new(
                            "dinero_import_invalid",
                            "Invalid Dinero attempt ID",
                        ));
                    }
                    let reference = store::originals(tx)?
                        .into_iter()
                        .find(|r| r.attempt_id == id)
                        .ok_or_else(|| {
                            Error::new(
                                "dinero_import_not_found",
                                "Dinero source original not found",
                            )
                        })?;
                    let bytes = guard
                        .read_document(&reference.path, &reference.sha256)
                        .map_err(|_| unavailable())?;
                    store::verify_original(tx, &reference, &bytes)?;
                    Ok(Output::Original(bytes))
                }
                Action::Plan(file) | Action::Apply(file, _) => {
                    if file.mime() != "application/json" {
                        return Err(Error::new(
                            "dinero_import_invalid",
                            "Dinero source must be an original JSON package",
                        ));
                    }
                    let decoded = store::decode_source(file.bytes())?;
                    if decoded.source.path != file.relative_path()
                        || decoded.source.sha256 != file.hash()
                        || decoded.source.size_bytes != file.bytes().len() as u64
                    {
                        return Err(unavailable());
                    }
                    let now = clock.now_unix_ms().map_err(|_| {
                        Error::new("invalid_auth_clock", "Trusted Dinero clock unavailable")
                    })?;
                    let latest = Date::parse(
                        &rentemester_runtime::posting_time::latest_date(now, 0).map_err(|_| {
                            Error::new(
                                "invalid_auth_clock",
                                "Trusted Dinero posting horizon unavailable",
                            )
                        })?,
                    )?;
                    match action {
                        Action::Plan(_) => {
                            Ok(Output::Json(store::prepare(tx, &decoded, latest)?.review()))
                        }
                        Action::Apply(_, input) => {
                            if !input.confirm {
                                return Err(Error::new(
                                    "dinero_import_invalid",
                                    "Explicit confirmation required",
                                ));
                            }
                            idempotency::validate_key(&input.idempotency_key)?;
                            let payload = json!({"sourceSha256":decoded.source.sha256,"sizeBytes":decoded.source.size_bytes,"planHash":input.plan_hash,"confirm":true});
                            let actor = format!("user:{subject}");
                            match idempotency::execute(
                                tx,
                                idempotency::Input {
                                    key: &input.idempotency_key,
                                    operation: Operation::DineroImport,
                                    principal_kind: PrincipalKind::User,
                                    subject,
                                    payload: &payload,
                                    created_by: &actor,
                                    created_by_program: "native-dinero-import",
                                    now_ms: now,
                                },
                                |tx| {
                                    let prepared = store::prepare(tx, &decoded, latest)?;
                                    if prepared.review()["planHash"] != input.plan_hash {
                                        return Err(Error::new(
                                            "dinero_plan_changed",
                                            "Dinero source or reviewed current facts changed",
                                        ));
                                    }
                                    if prepared.review()["status"] != "ready" {
                                        return Err(Error::new(
                                            "dinero_import_refused",
                                            "Reviewed Dinero source has unresolved blockers",
                                        ));
                                    }
                                    // These generated paths are independently content-bound, never caller paths.
                                    let path =
                                        guard.publish_document(file).map_err(|_| unavailable())?;
                                    if path != decoded.source.path {
                                        return Err(unavailable());
                                    }
                                    if let Some(archive) = &decoded.archive {
                                        let document = Prepared::new(
                                            "dinero-closed-history.json".into(),
                                            archive.bytes.clone(),
                                        )
                                        .map_err(|_| unavailable())?;
                                        if document.hash() != archive.source.sha256
                                            || document.relative_path() != archive.source.path
                                            || document.bytes().len() as u64
                                                != archive.source.size_bytes
                                        {
                                            return Err(unavailable());
                                        }
                                        if guard
                                            .publish_document(&document)
                                            .map_err(|_| unavailable())?
                                            != archive.source.path
                                        {
                                            return Err(unavailable());
                                        }
                                    }
                                    store::apply(tx, &prepared, input, latest, subject, now)
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
                                    "Dinero request key refused; inspect canonical retained import before retrying",
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
