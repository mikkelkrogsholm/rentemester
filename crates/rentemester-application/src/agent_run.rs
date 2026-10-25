//! Current-authority, transaction-local observations and review effects for AgentRun.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
    service_company,
};
use rentemester_engine::{Error, agent_run::*};
use rentemester_runtime::namespace::NamespaceGuard;
use rentemester_storage::agent_finance::{ExpenseStep, PayableStep};
use rusqlite::{Connection, Transaction};
pub enum Action<'a> {
    Start,
    BankAccount(&'a str),
    Expenses(&'a Policy),
    Payables(&'a Policy),
    Proposal(&'a Proposal),
    Overdue,
    Reconcile,
    Deadlines,
    Report,
}
pub enum Output {
    Actor(String),
    BankAccount(String),
    Expenses(Vec<ExpenseStep>),
    Payables(Vec<PayableStep>),
    Review(ReviewSnapshot),
    Exceptions(Vec<RoutedException>),
    Done,
}
fn apply(
    tx: &Transaction<'_>,
    action: Action<'_>,
    as_of: &str,
    actor: &str,
    expected: Option<&str>,
) -> Result<Output, Error> {
    if !matches!(action, Action::Start) {
        rentemester_engine::fiscal_year::Date::parse(as_of)?;
    }
    if expected.is_some_and(|e| e != actor) {
        return Err(Error::new(
            "access_denied",
            "Current actor assertion did not match",
        ));
    }
    use rentemester_storage::{agent_finance as finance, agent_review as review};
    Ok(match action {
        Action::Start => Output::Actor(actor.into()),
        Action::BankAccount(csv) => Output::BankAccount(finance::import_account(tx, csv)?),
        Action::Expenses(p) => Output::Expenses(finance::plan_expenses(tx, p, as_of)?),
        Action::Payables(p) => Output::Payables(finance::plan_payables(tx, p, as_of)?),
        Action::Proposal(p) => {
            review::record_proposal(tx, p, actor)?;
            Output::Done
        }
        Action::Overdue => {
            review::sync_overdue_payables(tx, as_of, actor)?;
            Output::Done
        }
        Action::Reconcile => {
            review::reconcile(tx, actor)?;
            Output::Done
        }
        Action::Deadlines => Output::Review(review::review_deadlines(tx, as_of, actor)?),
        Action::Report => Output::Exceptions(review::open_report_exceptions(tx)?),
    })
}
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    action: Action<'_>,
    as_of: &str,
    expected: Option<&str>,
) -> Result<Committed<Output>, Failure> {
    guard
        .ensure_connections(control, company)
        .map_err(|_| Failure {
            stage: control_fence::FailureStage::NotCommitted,
            error: Error::new("namespace_refused", "Agent namespace unavailable"),
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
        |tx, subject| apply(tx, action, as_of, &format!("user:{subject}"), expected),
    )
}
pub fn execute_service(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: service_company::Context<'_>,
    action: Action<'_>,
    as_of: &str,
    expected: Option<&str>,
) -> Result<service_company::ServiceResult<Output>, service_company::ServiceFailure> {
    guard
        .ensure_connections(control, company)
        .map_err(|_| service_company::ServiceFailure {
            usage: service_company::UsageStage::NotCommitted,
            business: service_company::BusinessStage::NotAttempted,
            error: Error::new("namespace_refused", "Agent namespace unavailable"),
            control_reusable: false,
            company_reusable: false,
            rejection: None,
        })?;
    service_company::execute_operation(
        control,
        company,
        context,
        Permission::CompanyLedgerPost,
        true,
        |tx, subject, _| {
            Ok(service_company::OperationResult {
                result: apply(
                    tx,
                    action,
                    as_of,
                    &format!("service_account:{subject}"),
                    expected,
                )?,
                receipt: None,
            })
        },
    )
}
/// Exact source summary; no wall clock or private paths.
pub fn build_summary(r: &mut Report) {
    let mut s = vec![
        format!(
            "{} bilag ingested{}",
            r.documents_ingested,
            if r.documents_rejected > 0 {
                format!(", {} afvist", r.documents_rejected)
            } else {
                String::new()
            }
        ),
        format!(
            "{} banktransaktioner importeret",
            r.bank_transactions_imported
        ),
        format!(
            "{} udgifter bogført automatisk af agenten",
            r.expenses_booked.len()
        ),
        format!(
            "{} kreditorposter afregnet automatisk fra banken",
            r.payables_matched.len()
        ),
    ];
    if r.accrual_recognitions_due > 0 {
        s.push(format!(
            "{} periodeafgrænsnings-perioder forfaldne — kræver bogføring",
            r.accrual_recognitions_due
        ));
    }
    s.push(format!(
        "{} i exception-kø — kræver et menneske{}",
        r.open_exceptions.len(),
        if r.open_exceptions.is_empty() {
            " (intet)"
        } else {
            ""
        }
    ));
    if r.upcoming_deadlines.is_empty() {
        s.push("Ingen deadlines inden for horisonten".into());
    } else {
        for d in &r.upcoming_deadlines {
            s.push(format!(
                "{} {}..{} forfalder {} ({} dage) — {}",
                if d.kind == "vat_period" {
                    "Momsangivelse"
                } else {
                    "Årsrapport"
                },
                d.period_start,
                d.period_end,
                d.due_date,
                d.days_remaining,
                if d.ready { "klar" } else { "kræver handling" }
            ));
        }
    }
    r.summary = s;
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn agent_run_summary_preserves_source() {
        let mut r = Report::new(
            "user:synthetic".into(),
            "synthetic".into(),
            "2026-10-25".into(),
        );
        build_summary(&mut r);
        assert_eq!(
            r.summary,
            vec![
                "0 bilag ingested",
                "0 banktransaktioner importeret",
                "0 udgifter bogført automatisk af agenten",
                "0 kreditorposter afregnet automatisk fra banken",
                "0 i exception-kø — kræver et menneske (intet)",
                "Ingen deadlines inden for horisonten"
            ]
        );
    }
    #[test]
    fn agent_run_actor_assertion_and_date_are_fail_closed() {
        let mut c = Connection::open_in_memory().unwrap();
        let tx = c.transaction().unwrap();
        assert_eq!(
            apply(
                &tx,
                Action::Start,
                "2026-10-25",
                "user:synthetic",
                Some("user:forged")
            )
            .err()
            .unwrap()
            .code,
            "access_denied"
        );
        assert!(apply(&tx, Action::Deadlines, "2026-02-30", "user:synthetic", None).is_err());
    }
}
