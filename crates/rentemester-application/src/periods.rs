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
    Readiness(&'a str, &'a str),
    Review(&'a rentemester_storage::periods::Bounds),
    Close(&'a rentemester_storage::periods::Close),
    Reopen(&'a rentemester_storage::periods::Reopen),
    Report(&'a rentemester_storage::periods::Report),
}
fn evidence(tx: &Transaction<'_>, guard: &mut NamespaceGuard) -> Result<Vec<Value>, Error> {
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
            guard
                .read_document(&original.path, &original.hash)
                .map_err(|_| {
                    Error::new(
                        "period_evidence_unavailable",
                        "original unavailable or integrity mismatch",
                    )
                })?;
            if kind == "issued_invoice" {
                crate::sales::verify_invoice(tx, guard, id)?;
            }
            Ok::<(), Error>(())
        })();
        if let Err(error) = check {
            result.push(json!({"documentId":id,"error":error.message}));
        }
    }
    let mut banks = tx.prepare("SELECT source_bank_transaction_id FROM journal_entries WHERE source_bank_transaction_id IS NOT NULL UNION SELECT bank_transaction_id FROM bank_journal_reconciliation_links UNION SELECT bank_transaction_id FROM bank_reconciliation_correction_events UNION SELECT bank_transaction_id FROM invoice_claim_payments").map_err(|_| Error::new("period_evidence_unavailable", "bank source register unavailable"))?;
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
            let hash = rentemester_storage::bank_reconciliation::source_hash(tx, id)?;
            let bytes = guard.read_bank_statement(&hash).map_err(|_| {
                Error::new(
                    "period_evidence_unavailable",
                    "bank original unavailable or integrity mismatch",
                )
            })?;
            rentemester_storage::bank_reconciliation::verify_source(tx, id, &bytes)
        })();
        if let Err(error) = check {
            result.push(json!({"bankTransactionId":id,"error":error.message}));
        }
    }
    if let Err(error) = crate::archive_import::verify_physical(tx, guard) {
        result.push(json!({"source":"closed-history-archive","error":error.message}));
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
    let permission = if matches!(action, Action::List) {
        Permission::CompanyRead
    } else {
        Permission::CompanyReview
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
        matches!(action, Action::Readiness(..)),
        |tx, actor| {
            let (start, end) = match action {
                Action::List => return rentemester_storage::periods::list(tx, slug),
                Action::Report(input) => {
                    return rentemester_storage::periods::report(tx, input, actor);
                }
                Action::Readiness(start, end) => (start, end),
                Action::Review(input) => {
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
            let packet =
                rentemester_storage::periods::compute(tx, start, end, evidence(tx, guard))?;
            match action {
                Action::Readiness(..) => Ok(packet),
                Action::Review(_) => rentemester_storage::periods::review(tx, &packet, actor),
                Action::Close(input) => {
                    rentemester_storage::periods::close(tx, input, &packet, actor, today)
                }
                Action::Reopen(input) => {
                    rentemester_storage::periods::reopen(tx, input, &packet, actor)
                }
                Action::List | Action::Report(_) => unreachable!(),
            }
        },
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
