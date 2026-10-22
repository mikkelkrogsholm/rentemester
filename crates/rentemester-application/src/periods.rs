//! Period decisions share the current user/role fence and the owned document namespace.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
};
use rentemester_engine::Error;
use rentemester_runtime::namespace::NamespaceGuard;
use rusqlite::{Connection, Transaction};
use serde_json::{Value, json};
pub enum Action<'a> {
    List,
    OriginalList,
    McpReview(&'a rentemester_storage::periods::Bounds),
    McpClose(&'a rentemester_storage::periods::original::CloseInput),
    OriginalStatus(i64),
    OriginalReview(&'a rentemester_storage::periods::original::Review),
    OriginalClose(&'a rentemester_storage::periods::original::CloseInput),
    OriginalReopen(&'a rentemester_storage::periods::Reopen),
    Readiness(&'a str, &'a str),
    Review(&'a rentemester_storage::periods::Bounds),
    Close(&'a rentemester_storage::periods::Close),
    Reopen(&'a rentemester_storage::periods::Reopen),
    Report(&'a rentemester_storage::periods::Report),
}
fn evidence(tx: &Transaction<'_>, guard: &mut NamespaceGuard) -> Result<Vec<Value>, Error> {
    evidence_mode(tx, guard, false)
}
pub(crate) fn audit_evidence(
    tx: &Transaction<'_>,
    guard: &mut NamespaceGuard,
) -> Result<Vec<Value>, Error> {
    evidence_mode(tx, guard, true)
}
fn evidence_mode(
    tx: &Transaction<'_>,
    guard: &mut NamespaceGuard,
    audit: bool,
) -> Result<Vec<Value>, Error> {
    let mut result = vec![];
    let mut q = tx
        .prepare("SELECT id,document_type FROM documents ORDER BY id")
        .map_err(|_| {
            Error::new(
                "period_evidence_unavailable",
                "document register unavailable",
            )
        })?;
    let mut rows = q.query([]).map_err(|_| {
        Error::new(
            "period_evidence_unavailable",
            "document register unavailable",
        )
    })?;
    while let Some(row) = rows.next().map_err(|_| {
        Error::new(
            "period_evidence_unavailable",
            "document register unavailable",
        )
    })? {
        let id: i64 = row.get(0).map_err(|_| {
            Error::new(
                "period_evidence_unavailable",
                "document reference unavailable",
            )
        })?;
        let kind: String = row
            .get(1)
            .map_err(|_| Error::new("period_evidence_unavailable", "document kind unavailable"))?;
        let check = (|| {
            let original = rentemester_storage::documents::evidence(tx, id)?;
            let bytes = guard
                .read_document(&original.path, &original.hash)
                .map_err(|_| {
                    Error::new(
                        "period_evidence_unavailable",
                        "original unavailable or integrity mismatch",
                    )
                })?;
            if audit && kind == "credit_note" {
                let payload: String = tx
                    .query_row(
                        "SELECT payload_json FROM documents WHERE id=?1",
                        [id],
                        |row| row.get(0),
                    )
                    .map_err(|_| {
                        Error::new(
                            "period_evidence_unavailable",
                            "credit original metadata unavailable",
                        )
                    })?;
                if bytes != payload.as_bytes() {
                    return Err(Error::new(
                        "period_evidence_unavailable",
                        "credit original and stored payload differ",
                    ));
                }
            }
            if kind == "issued_invoice" {
                crate::sales::verify_invoice(tx, guard, id)?;
            }
            Ok::<(), Error>(())
        })();
        if let Err(error) = check {
            result.push(json!({"documentId":id,"error":error.message}));
        }
    }
    let bank_sql = if audit {
        "SELECT id FROM bank_transactions ORDER BY id"
    } else {
        "SELECT source_bank_transaction_id FROM journal_entries WHERE source_bank_transaction_id IS NOT NULL UNION SELECT bank_transaction_id FROM bank_journal_reconciliation_links UNION SELECT bank_transaction_id FROM bank_reconciliation_correction_events UNION SELECT bank_transaction_id FROM invoice_claim_payments"
    };
    let mut banks = tx.prepare(bank_sql).map_err(|_| {
        Error::new(
            "period_evidence_unavailable",
            "bank source register unavailable",
        )
    })?;
    let ids = banks
        .query_map([], |row| row.get::<_, i64>(0))
        .map_err(|_| {
            Error::new(
                "period_evidence_unavailable",
                "bank source register unavailable",
            )
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| {
            Error::new(
                "period_evidence_unavailable",
                "bank source reference unavailable",
            )
        })?;
    for id in ids {
        let check = (|| {
            let hash = if audit {
                tx.query_row(
                    "SELECT source_file_hash FROM bank_transactions WHERE id=?1",
                    [id],
                    |row| row.get::<_, String>(0),
                )
                .map_err(|_| {
                    Error::new(
                        "audit_originals_unavailable",
                        "bank source hash unavailable",
                    )
                })?
            } else {
                rentemester_storage::bank_reconciliation::source_hash(tx, id)?
            };
            let bytes = guard.read_bank_statement(&hash).map_err(|_| {
                Error::new(
                    "period_evidence_unavailable",
                    "bank original unavailable or integrity mismatch",
                )
            })?;
            if audit {
                rentemester_storage::bank_reconciliation::verify_audit_source(tx, id, &bytes)
            } else {
                rentemester_storage::bank_reconciliation::verify_source(tx, id, &bytes)
            }
        })();
        if let Err(error) = check {
            result.push(json!({"bankTransactionId":id,"error":error.message}));
        }
    }
    if let Err(error) = crate::archive_import::verify_physical(tx, guard) {
        result.push(json!({"source":"closed-history-archive","error":error.message}));
    }
    if let Err(error) = crate::dinero_import::verify_physical(tx, guard) {
        result.push(json!({"source":"dinero-import","error":error.message}));
    }
    Ok(result)
}
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    action: Action<'_>,
    today: &str,
) -> Result<Committed<Value>, Failure> {
    let slug = context.selection.slug;
    let permission = permission(&action);
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
        matches!(
            action,
            Action::List | Action::OriginalList | Action::Readiness(..) | Action::OriginalStatus(_)
        ),
        |tx, actor| apply(tx, guard, action, today, slug, actor),
    )
}

/// Reuse native close source assurance without a period decision or accounting write.
pub fn verify_sources(
    tx: &Transaction<'_>,
    guard: &mut NamespaceGuard,
) -> Result<Vec<Value>, Error> {
    let mut errors = rentemester_storage::periods::verify_sources(tx)?;
    errors.extend(evidence(tx, guard)?);
    Ok(errors)
}

fn permission(action: &Action<'_>) -> Permission {
    match action {
        Action::List | Action::OriginalList | Action::Readiness(..) | Action::OriginalStatus(_) => {
            Permission::CompanyRead
        }
        Action::McpClose(i) if i.force == Some(true) => Permission::CompanyPeriodForceClose,
        _ => Permission::CompanyReview,
    }
}
fn apply(
    tx: &Transaction<'_>,
    guard: &mut NamespaceGuard,
    action: Action<'_>,
    today: &str,
    slug: &str,
    actor: &str,
) -> Result<Value, Error> {
    let (start, end) = match action {
        Action::OriginalStatus(id) => {
            return rentemester_storage::periods::original::status(tx, id);
        }
        Action::OriginalClose(input) | Action::McpClose(input) => {
            use rentemester_storage::periods::original;
            return original::atomic(tx, None, || {
                if !input.confirm {
                    return Err(Error::new("confirmation_required", "confirmation required"));
                }
                if let Some(value) = original::report_existing(tx, input, actor)? {
                    return Ok(value);
                }
                let packet = rentemester_storage::periods::compute(
                    tx,
                    &input.period_start,
                    &input.period_end,
                    evidence(tx, guard),
                )?;
                original::atomic(tx, Some(&packet), || {
                    if matches!(action, Action::McpClose(_)) {
                        rentemester_storage::periods::close_mcp(tx, input, &packet, actor, today)
                    } else {
                        original::close_current(tx, input, &packet, actor, today)
                    }
                })
            });
        }
        Action::OriginalReview(input) => (&*input.period_start, &*input.period_end),
        Action::OriginalReopen(input) => (&*input.period_start, &*input.period_end),
        Action::OriginalList => return rentemester_storage::periods::list_original(tx),
        Action::List => return rentemester_storage::periods::list(tx, slug),
        Action::Report(input) => {
            return rentemester_storage::periods::report(tx, input, actor);
        }
        Action::Readiness(start, end) => (start, end),
        Action::Review(input) | Action::McpReview(input) => {
            if !input.confirm {
                return Err(Error::new(
                    "confirmation_required",
                    "review confirmation required",
                ));
            }
            (&*input.period_start, &*input.period_end)
        }
        Action::Close(input) => (&*input.period_start, &*input.period_end),
        Action::Reopen(input) => (&*input.period_start, &*input.period_end),
    };
    let packet = rentemester_storage::periods::compute(tx, start, end, evidence(tx, guard))?;
    match action {
        Action::OriginalReview(input) => {
            rentemester_storage::periods::original::review_expected(tx, input, &packet, actor)
        }
        Action::OriginalReopen(input) => {
            rentemester_storage::periods::original::atomic(tx, None, || {
                rentemester_storage::periods::original::reopen_current(tx, input, &packet, actor)
            })
        }
        Action::Readiness(..) => Ok(packet),
        Action::McpReview(_) => {
            Ok(json!({"ok":true,"review":rentemester_storage::periods::review(tx,&packet,actor)?}))
        }
        Action::Review(_) => rentemester_storage::periods::review(tx, &packet, actor),
        Action::Close(input) => {
            rentemester_storage::periods::close(tx, input, &packet, actor, today)
        }
        Action::Reopen(input) => rentemester_storage::periods::reopen(tx, input, &packet, actor),
        Action::List
        | Action::OriginalList
        | Action::McpClose(_)
        | Action::Report(_)
        | Action::OriginalStatus(_)
        | Action::OriginalClose(_) => unreachable!(),
    }
}

pub fn execute_service(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: crate::service_company::Context<'_>,
    action: Action<'_>,
    today: &str,
) -> Result<crate::service_company::ServiceResult<Value>, crate::service_company::ServiceFailure> {
    let permission = permission(&action);
    let mutation = !matches!(
        action,
        Action::OriginalList | Action::Readiness(..) | Action::OriginalStatus(_)
    );
    let slug = context.selection.slug;
    crate::service_company::execute_operation(
        control,
        company,
        context,
        permission,
        mutation,
        |tx, subject, _| {
            let actor = format!("service-account:{subject}");
            let result = apply(tx, guard, action, today, slug, &actor)?;
            Ok(crate::service_company::OperationResult {
                result,
                receipt: None,
            })
        },
    )
}
