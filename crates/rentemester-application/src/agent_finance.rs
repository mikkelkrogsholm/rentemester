//! Admitted finance effects: replan and execute the complete canonical application writer
//! within each current-authority fence, retaining physical source and VAT guards.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
    service_company,
};
use rentemester_engine::{
    Error,
    agent_run::{BookedExpense, PayableMatch, Policy},
};
use rentemester_runtime::namespace::NamespaceGuard;
use rentemester_storage::{
    agent_finance::{self as store, ExpenseStep, PayableStep},
    idempotency::PrincipalKind,
};
use rusqlite::{Connection, Transaction};
use serde_json::Value;

pub enum Selection {
    Book { bank_id: i64, document_id: i64 },
    Pay { bank_id: i64, payable_id: i64 },
}
pub enum Candidate {
    Book(rentemester_storage::expense_original::Input, BookedExpense),
    Pay(rentemester_storage::payable_original::Pay, PayableMatch),
}
fn stale() -> Error {
    Error::new(
        "agent_finance_candidate_changed",
        "Canonical finance candidate no longer admissible",
    )
}
fn select(
    tx: &Transaction<'_>,
    policy: &Policy,
    as_of: &str,
    selection: &Selection,
) -> Result<Candidate, Error> {
    match selection {
        Selection::Book {
            bank_id,
            document_id,
        } => store::plan_expenses(tx, policy, as_of)?
            .into_iter()
            .find_map(|step| match step {
                ExpenseStep::Book { candidate, summary }
                    if candidate.bank_transaction_id == *bank_id
                        && candidate.document_id == *document_id =>
                {
                    Some(Candidate::Book(candidate, summary))
                }
                _ => None,
            })
            .ok_or_else(stale),
        Selection::Pay {
            bank_id,
            payable_id,
        } => store::plan_payables(tx, policy, as_of)?
            .into_iter()
            .find_map(|step| match step {
                PayableStep::Pay { candidate, summary }
                    if candidate.bank_transaction_id == *bank_id
                        && candidate.payable_id == *payable_id =>
                {
                    Some(Candidate::Pay(candidate, summary))
                }
                _ => None,
            })
            .ok_or_else(stale),
    }
}
fn physical(
    tx: &Transaction<'_>,
    guard: &mut NamespaceGuard,
    candidate: &Candidate,
) -> Result<(), Error> {
    let (doc, bank) = match candidate {
        Candidate::Book(i, _) => (i.document_id, i.bank_transaction_id),
        Candidate::Pay(i, s) => (s.document_id, i.bank_transaction_id),
    };
    rentemester_storage::invoice_extraction::validate_history(tx)?;
    let original = rentemester_storage::documents::evidence(tx, doc)?;
    let bytes = guard
        .read_document(&original.path, &original.hash)
        .map_err(|_| {
            Error::new(
                "document_evidence_unavailable",
                "Retained original unavailable or changed",
            )
        })?;
    for retained in rentemester_storage::invoice_extraction::sources(tx)? {
        if retained.document_id == doc
            && (retained.sha256 != original.hash || retained.pdf_bytes != bytes)
        {
            return Err(stale());
        }
    }
    rentemester_storage::bank_original::verify_journals(tx)?;
    let hash = rentemester_storage::bank_reconciliation::source_hash(tx, bank)?;
    let bytes = guard.read_bank_statement(&hash).map_err(|_| {
        Error::new(
            "bank_statement_original_unavailable",
            "Retained bank original unavailable or changed",
        )
    })?;
    rentemester_storage::bank_reconciliation::verify_source(tx, bank, &bytes)
}
/// One admitted effect, never a whole-loop transaction. Policy is trusted host configuration.
/// The expected actor is only an equality assertion against the current authenticated principal.
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    policy: &Policy,
    as_of: &str,
    selection: &Selection,
    expected_actor: Option<&str>,
) -> Result<Committed<Value>, Failure> {
    guard
        .ensure_connections(control, company)
        .map_err(|_| Failure {
            stage: control_fence::FailureStage::NotCommitted,
            error: stale(),
            control_reusable: false,
            company_reusable: false,
        })?;
    control_fence::execute_cookie_subject(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Current,
        },
        context.selection,
        Permission::CompanyLedgerPost,
        |tx, subject| {
            let candidate = select(tx, policy, as_of, selection)?;
            physical(tx, guard, &candidate)?;
            write(
                tx,
                guard,
                subject,
                PrincipalKind::User,
                candidate,
                expected_actor,
                context.clock,
            )
        },
    )
}
/// Service counterpart; current grants/usage are resolved by the existing service fence.
pub fn execute_service(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: service_company::Context<'_>,
    policy: &Policy,
    as_of: &str,
    selection: &Selection,
    expected_actor: Option<&str>,
) -> Result<service_company::ServiceResult<Value>, service_company::ServiceFailure> {
    guard
        .ensure_connections(control, company)
        .map_err(|_| service_company::ServiceFailure {
            usage: service_company::UsageStage::NotCommitted,
            business: service_company::BusinessStage::NotAttempted,
            error: stale(),
            control_reusable: false,
            company_reusable: false,
            rejection: None,
        })?;
    let clock = context.clock;
    service_company::execute_operation(
        control,
        company,
        context,
        Permission::CompanyLedgerPost,
        true,
        |tx, subject, _| {
            let candidate = select(tx, policy, as_of, selection)?;
            physical(tx, guard, &candidate)?;
            Ok(service_company::OperationResult {
                result: write(
                    tx,
                    guard,
                    subject,
                    PrincipalKind::ServiceAccount,
                    candidate,
                    expected_actor,
                    clock,
                )?,
                receipt: None,
            })
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn agent_finance_missing_current_candidate_refuses_without_effect() {
        let mut control = Connection::open_in_memory().unwrap();
        let mut company = Connection::open_in_memory().unwrap();
        rentemester_storage::fresh::initialize_empty_pair(
            &mut control,
            &mut company,
            "Synthetic finance",
        )
        .unwrap();
        let tx = company
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        for selection in [
            Selection::Book {
                bank_id: 1,
                document_id: 1,
            },
            Selection::Pay {
                bank_id: 1,
                payable_id: 1,
            },
        ] {
            assert!(select(&tx, &Policy::default(), "2026-10-18", &selection).is_err());
        }
        assert_eq!(
            tx.query_row::<i64, _, _>("SELECT count(*) FROM journal_entries", [], |r| r.get(0))
                .unwrap(),
            0
        );
    }
}

fn write(
    tx: &Transaction<'_>,
    guard: &mut NamespaceGuard,
    subject: &str,
    kind: PrincipalKind,
    candidate: Candidate,
    expected: Option<&str>,
    clock: &dyn rentemester_runtime::auth::Clock,
) -> Result<Value, Error> {
    let service = matches!(kind, PrincipalKind::ServiceAccount);
    let actual = format!(
        "{}:{subject}",
        if service { "service_account" } else { "user" }
    );
    if expected.is_some_and(|e| e != actual) {
        return Err(Error::new(
            "access_denied",
            "Current actor assertion did not match",
        ));
    }
    match candidate {
        Candidate::Book(input, mut summary) => {
            let result = crate::expense_original::book_tx_agent(
                tx, guard, subject, &input, expected, clock, service,
            )
            .map_err(|f| f.error)?;
            summary.journal_entry_no = result["entryNo"].as_str().map(str::to_owned);
            Ok(serde_json::json!({"result":result,"expense":summary}))
        }
        Candidate::Pay(input, mut summary) => {
            let result = crate::payable_original::authorized_agent(
                tx,
                guard,
                rentemester_storage::payable_original::Action::Pay(&input),
                subject,
                kind,
                clock,
            )?;
            summary.journal_entry_no = result["data"]["journalEntryId"]
                .as_i64()
                .map(|id| id.to_string());
            Ok(serde_json::json!({"result":result,"payable":summary}))
        }
    }
}
