//! Reviewed live Dinero source acceptance under current human authority.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
};
use rentemester_engine::{Error, fiscal_year::Date};
use rentemester_runtime::{
    document::Prepared,
    namespace::{NamespaceGuard, PublishedDocument},
};
use rentemester_storage::{
    dinero_import as store,
    idempotency::{self, Operation, Outcome, PrincipalKind},
};
use rusqlite::Connection;
use serde_json::{Value, json};
#[derive(Clone, Copy)]
pub enum Action<'a> {
    Plan(&'a Prepared),
    OriginalRun(&'a Prepared, bool),
    Apply(&'a Prepared, &'a store::Apply),
    List,
    Original(i64),
    ImportedReceivables(&'a str),
}
pub enum Output {
    Json(Value),
    Imported(Value),
    Original {
        bytes: Vec<u8>,
        content_type: &'static str,
    },
}
fn unavailable() -> Error {
    Error::new(
        "dinero_source_unavailable",
        "Dinero source original unavailable or retained assurance differs",
    )
}
fn publish(
    guard: &mut NamespaceGuard,
    file: &Prepared,
    expected: &str,
    owned: &mut Vec<PublishedDocument>,
) -> Result<(), Error> {
    let (path, token) = guard
        .publish_document_tracked(file)
        .map_err(|_| unavailable())?;
    if let Some(token) = token {
        owned.push(token);
    }
    if path != expected {
        return Err(unavailable());
    }
    Ok(())
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
    for reference in store::supporting_originals(db)? {
        guard
            .read_document(&reference.path, &reference.hash)
            .map_err(|_| unavailable())?;
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
    let readonly = !matches!(action, Action::Apply(..) | Action::OriginalRun(..));
    let permission = match action {
        Action::Apply(..) | Action::OriginalRun(..) => Permission::CompanyAdmin,
        Action::Original(_) => Permission::CompanyDocumentsRead,
        _ => Permission::CompanyRead,
    };
    let clock = context.clock;
    // CompanyAdmin is a financial operation here: the normal fence performs current
    // authorization, transfer exclusion and the current financial-backup check.
    let mut owned = vec![];
    let result = control_fence::execute_cookie_subject_mode(
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
            let verified_archives = crate::archive_import::verify_physical(tx, guard)?;
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
                    Ok(Output::Original {
                        bytes,
                        content_type: if reference.path.ends_with(".zip") {
                            "application/zip"
                        } else {
                            "application/json"
                        },
                    })
                }
                Action::Plan(file) | Action::Apply(file, _) | Action::OriginalRun(file, _) => {
                    if !matches!(file.mime(), "application/json" | "application/zip") {
                        return Err(Error::new(
                            "dinero_import_invalid",
                            "Dinero source must be an original JSON package or ZIP archive",
                        ));
                    }
                    let decoded = match store::decode_source(file.bytes()) {
                        Ok(decoded) => decoded,
                        Err(error)
                            if matches!(action, Action::OriginalRun(..))
                                && error.code == "dinero_import_refused" =>
                        {
                            return Ok(Output::Json(
                                json!({"ok":false,"sourceSystem":"dinero","openingBalanceLineCount":0,"historicalEntriesSkipped":0,
                                "auditTrail":["Import started from source system 'dinero'"],"appliedRules":["DK-BOOKKEEPING-BALANCED-001"],"errors":[error.message]}),
                            ));
                        }
                        Err(error) => return Err(error),
                    };
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
                        Action::OriginalRun(_, dry_run) => {
                            tx.execute_batch("SAVEPOINT original_import94")
                                .map_err(|_| unavailable())?;
                            let business = (|| {
                                let prepared = store::prepare(tx, &decoded, latest)?;
                                let receipts = store::receipt_publications(tx, &decoded)?;
                                publish(guard, file, &decoded.source.path, &mut owned)?;
                                for receipt in receipts {
                                    let doc = Prepared::new(receipt.path, receipt.bytes)
                                        .map_err(|_| unavailable())?;
                                    publish(guard, &doc, &doc.relative_path(), &mut owned)?;
                                }
                                if let Some(archive) = &decoded.archive {
                                    let doc = Prepared::new(
                                        "dinero-closed-history.json".into(),
                                        archive.bytes.clone(),
                                    )
                                    .map_err(|_| unavailable())?;
                                    publish(guard, &doc, &archive.source.path, &mut owned)?;
                                }
                                store::run_original(
                                    tx,
                                    &prepared,
                                    latest,
                                    subject,
                                    now,
                                    &verified_archives,
                                )
                            })();
                            if dry_run || business.is_err() {
                                tx.execute_batch("ROLLBACK TO original_import94")
                                    .map_err(|_| unavailable())?;
                                for token in owned.drain(..).rev() {
                                    guard.rollback_document(token).map_err(|_| unavailable())?;
                                }
                            }
                            tx.execute_batch("RELEASE original_import94")
                                .map_err(|_| unavailable())?;
                            match business {
                                Ok(mut value) => {
                                    if dry_run {
                                        value["dryRun"] = json!(true);
                                        value.as_object_mut().unwrap().remove("bilag");
                                    }
                                    Ok(Output::Json(value))
                                }
                                Err(error)
                                    if matches!(
                                        error.code,
                                        "dinero_import_refused"
                                            | "opening_balance_refused"
                                            | "company_setup_refused"
                                    ) =>
                                {
                                    Ok(Output::Json(
                                        json!({"ok":false,"sourceSystem":"dinero","cutOverDate":decoded.parsed.cut_over_date,
                                        "openingBalanceLineCount":decoded.parsed.opening_lines.len(),"historicalEntriesSkipped":decoded.parsed.vouchers.len(),
                                        "auditTrail":["Dinero import transaction rolled back"],"appliedRules":["DK-BOOKKEEPING-BALANCED-001"],"errors":[error.message]}),
                                    ))
                                }
                                Err(error) => Err(error),
                            }
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
                                    let receipts = store::receipt_publications(tx, &decoded)?
                                        .into_iter()
                                        .map(|source| {
                                            Prepared::new(source.path, source.bytes)
                                                .map_err(|_| unavailable())
                                        })
                                        .collect::<Result<Vec<_>, _>>()?;
                                    publish(guard, file, &decoded.source.path, &mut owned)?;
                                    for receipt in &receipts {
                                        publish(
                                            guard,
                                            receipt,
                                            &receipt.relative_path(),
                                            &mut owned,
                                        )?;
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
                                        publish(
                                            guard,
                                            &document,
                                            &archive.source.path,
                                            &mut owned,
                                        )?;
                                    }
                                    store::apply_with_archives(
                                        tx,
                                        &prepared,
                                        input,
                                        latest,
                                        subject,
                                        now,
                                        &verified_archives,
                                    )
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
    );
    if result
        .as_ref()
        .is_err_and(|failure| failure.stage == control_fence::FailureStage::NotCommitted)
        && company.is_autocommit()
    {
        let mut cleanup_failed = false;
        for token in owned.into_iter().rev() {
            if guard.rollback_document(token).is_err() {
                cleanup_failed = true;
            }
        }
        if cleanup_failed {
            return result.map_err(|mut failure| {
                failure.error = Error::new("dinero_original_cleanup_failed", "Import rolled back but an owned original could not be cleaned; inspect retained files before retrying");
                failure
            });
        }
    }
    result
}
