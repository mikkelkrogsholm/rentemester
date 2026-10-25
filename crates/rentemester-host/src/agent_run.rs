//! Original AgentRun coordinator and per-effect finance on the existing Owner.
//! The HTTP adapter performs ordered physical intake before these admitted phases.
use super::*;
use rentemester_application::agent_finance::{self as app, Selection};
use rentemester_engine::agent_run::Policy;
impl Owner {
    /// Replan and execute one canonical expense/payment under current human authority.
    pub fn agent_finance_effect(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        policy: &Policy,
        as_of: &str,
        selection: &Selection,
        expected_actor: Option<&str>,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |control, company, guard, context| {
            app::execute(
                control,
                company,
                guard,
                context,
                policy,
                as_of,
                selection,
                expected_actor,
            )
        })
    }
    /// Service counterpart; the standard service fence resolves current grants and usage.
    pub fn agent_finance_effect_service(
        &mut self,
        key: &ServiceKey,
        clock: &dyn Clock,
        policy: &Policy,
        as_of: &str,
        selection: &Selection,
        expected_actor: Option<&str>,
    ) -> Result<
        Attempt<
            Result<
                service_company::ServiceResult<serde_json::Value>,
                service_company::ServiceFailure,
            >,
        >,
        OwnerError,
    > {
        self.service_document_action(key, clock, |control, company, guard, context| {
            app::execute_service(
                control,
                company,
                guard,
                context,
                policy,
                as_of,
                selection,
                expected_actor,
            )
        })
    }
}

use rentemester_application::agent_run::{self as run, Action, Output};
use rentemester_engine::agent_run::{Proposal, RULE, Report, Request};
use rentemester_storage::agent_finance::{ExpenseStep, PayableStep};
/// An uncertain effect is never translated into a known report or retried.
#[derive(Debug)]
pub struct RunFailure {
    pub code: &'static str,
    pub message: String,
    pub unknown: bool,
}
impl RunFailure {
    fn owner(_: OwnerError) -> Self {
        Self {
            code: "outcome_unknown",
            message: "Agent owner unavailable; inspect canonical state before another run".into(),
            unknown: true,
        }
    }
    fn protocol() -> Self {
        Self {
            code: "agent_run_invalid",
            message: "Invalid AgentRun request or response".into(),
            unknown: false,
        }
    }
    pub fn authority(&self) -> bool {
        matches!(self.code, "access_denied" | "invalid_credentials")
    }
}
fn human<T>(
    a: Attempt<Result<Committed<T>, rentemester_application::control_fence::Failure>>,
) -> Result<T, RunFailure> {
    let retired = a.retired;
    match a.outcome {
        Ok(v) if !retired && v.control_reusable => Ok(v.value),
        Ok(_) => Err(RunFailure::owner(OwnerError::Retired)),
        Err(f) => Err(RunFailure {
            code: f.error.code,
            message: f.error.message,
            unknown: retired
                || f.stage == rentemester_application::control_fence::FailureStage::OutcomeUnknown
                || !f.control_reusable
                || !f.company_reusable,
        }),
    }
}
fn service<T>(
    a: Attempt<Result<service_company::ServiceResult<T>, service_company::ServiceFailure>>,
) -> Result<T, RunFailure> {
    let retired = a.retired;
    match a.outcome {
        Ok(v) if !retired && v.control_reusable => Ok(v.result),
        Ok(_) => Err(RunFailure::owner(OwnerError::Retired)),
        Err(f) => Err(RunFailure {
            code: f.error.code,
            message: f.error.message,
            unknown: retired
                || f.usage == service_company::UsageStage::OutcomeUnknown
                || f.business == service_company::BusinessStage::OutcomeUnknown
                || !f.control_reusable
                || !f.company_reusable,
        }),
    }
}
enum Principal<'a> {
    Human(&'a VerifiedCookie),
    Service(&'a ServiceKey),
}
impl Owner {
    fn run_action(
        &mut self,
        p: &Principal<'_>,
        clock: &dyn Clock,
        action: Action<'_>,
        date: &str,
        expected: Option<&str>,
    ) -> Result<Output, RunFailure> {
        match p {
            Principal::Human(c) => human(
                self.document_action(c, clock, |a, b, g, x| {
                    run::execute(a, b, g, x, action, date, expected)
                })
                .map_err(RunFailure::owner)?,
            ),
            Principal::Service(k) => service(
                self.service_document_action(k, clock, |a, b, g, x| {
                    run::execute_service(a, b, g, x, action, date, expected)
                })
                .map_err(RunFailure::owner)?,
            ),
        }
    }
    fn run_finance(
        &mut self,
        p: &Principal<'_>,
        clock: &dyn Clock,
        policy: &Policy,
        date: &str,
        s: &Selection,
        actor: &str,
    ) -> Result<serde_json::Value, RunFailure> {
        match p {
            Principal::Human(c) => human(
                self.agent_finance_effect(c, clock, policy, date, s, Some(actor))
                    .map_err(RunFailure::owner)?,
            ),
            Principal::Service(k) => service(
                self.agent_finance_effect_service(k, clock, policy, date, s, Some(actor))
                    .map_err(RunFailure::owner)?,
            ),
        }
    }
    fn run_start(
        &mut self,
        p: &Principal<'_>,
        clock: &dyn Clock,
        request: &Request,
        policy: &Policy,
    ) -> Result<Report, RunFailure> {
        if !request.confirm {
            return Err(RunFailure::protocol());
        }

        rentemester_storage::agent_finance::validate_policy(policy).map_err(|e| RunFailure {
            code: e.code,
            message: e.message,
            unknown: false,
        })?;
        let Output::Actor(actor) = self.run_action(
            p,
            clock,
            Action::Start,
            &request.as_of,
            request.expected_actor.as_deref(),
        )?
        else {
            return Err(RunFailure::protocol());
        };
        let mut report = Report::new(actor, self.slug.clone(), request.as_of.clone());
        if rentemester_engine::fiscal_year::Date::parse(&request.as_of).is_err() {
            report.ok = false;
            report.errors.push(format!(
                "--as-of must be a valid YYYY-MM-DD date (got: {})",
                request.as_of
            ));
        }
        Ok(report)
    }
    pub fn agent_run_start(
        &mut self,
        c: &VerifiedCookie,
        clock: &dyn Clock,
        r: &Request,
        policy: &Policy,
    ) -> Result<Report, RunFailure> {
        self.run_start(&Principal::Human(c), clock, r, policy)
    }
    pub fn agent_run_start_service(
        &mut self,
        k: &ServiceKey,
        clock: &dyn Clock,
        r: &Request,
        policy: &Policy,
    ) -> Result<Report, RunFailure> {
        self.run_start(&Principal::Service(k), clock, r, policy)
    }
    pub fn agent_run_proposal(
        &mut self,
        c: &VerifiedCookie,
        clock: &dyn Clock,
        p: &Proposal,
        r: &Report,
    ) -> Result<(), RunFailure> {
        self.run_action(
            &Principal::Human(c),
            clock,
            Action::Proposal(p),
            &r.as_of,
            Some(&r.actor),
        )
        .map(|_| ())
    }
    pub fn agent_run_proposal_service(
        &mut self,
        k: &ServiceKey,
        clock: &dyn Clock,
        p: &Proposal,
        r: &Report,
    ) -> Result<(), RunFailure> {
        self.run_action(
            &Principal::Service(k),
            clock,
            Action::Proposal(p),
            &r.as_of,
            Some(&r.actor),
        )
        .map(|_| ())
    }
    pub fn agent_run_finish(
        &mut self,
        c: &VerifiedCookie,
        clock: &dyn Clock,
        policy: &Policy,
        r: Report,
    ) -> Result<Report, RunFailure> {
        self.run_finish(&Principal::Human(c), clock, policy, r)
    }
    pub fn agent_run_finish_service(
        &mut self,
        k: &ServiceKey,
        clock: &dyn Clock,
        policy: &Policy,
        r: Report,
    ) -> Result<Report, RunFailure> {
        self.run_finish(&Principal::Service(k), clock, policy, r)
    }
    /// Resolve the configured bank binding and call the ordinary importer while
    /// the same Owner remains held. The importer revalidates current account state.
    pub fn agent_run_import_bank(
        &mut self,
        c: &VerifiedCookie,
        clock: &dyn Clock,
        csv: &str,
        now: &str,
        r: &Report,
    ) -> Result<serde_json::Value, RunFailure> {
        self.run_import_bank(&Principal::Human(c), clock, csv, now, r)
    }
    pub fn agent_run_import_bank_service(
        &mut self,
        k: &ServiceKey,
        clock: &dyn Clock,
        csv: &str,
        now: &str,
        r: &Report,
    ) -> Result<serde_json::Value, RunFailure> {
        self.run_import_bank(&Principal::Service(k), clock, csv, now, r)
    }
    fn run_import_bank(
        &mut self,
        p: &Principal<'_>,
        clock: &dyn Clock,
        csv: &str,
        now: &str,
        r: &Report,
    ) -> Result<serde_json::Value, RunFailure> {
        let Output::BankAccount(account) =
            self.run_action(p, clock, Action::BankAccount(csv), &r.as_of, Some(&r.actor))?
        else {
            return Err(RunFailure::protocol());
        };
        let action = rentemester_application::bank::Action::ImportOriginal {
            content: csv.as_bytes(),
            account: Some(&account),
            profile: None,
            now,
        };
        match p {
            Principal::Human(c) => human(self.bank(c, clock, action).map_err(RunFailure::owner)?),
            Principal::Service(k) => service(
                self.bank_service(k, clock, action)
                    .map_err(RunFailure::owner)?,
            ),
        }
    }
    fn run_finish(
        &mut self,
        p: &Principal<'_>,
        clock: &dyn Clock,
        policy: &Policy,
        mut r: Report,
    ) -> Result<Report, RunFailure> {
        if r.company != self.slug {
            return Err(RunFailure::protocol());
        }
        r.phases.extend(["book".into(), "route".into()]);
        let Output::Expenses(steps) =
            self.run_action(p, clock, Action::Expenses(policy), &r.as_of, Some(&r.actor))?
        else {
            return Err(RunFailure::protocol());
        };
        for step in steps {
            match step {
                ExpenseStep::Route(proposal) => {
                    self.run_action(
                        p,
                        clock,
                        Action::Proposal(&proposal),
                        &r.as_of,
                        Some(&r.actor),
                    )?;
                }
                ExpenseStep::Book { candidate, summary } => {
                    let selection = Selection::Book {
                        bank_id: candidate.bank_transaction_id,
                        document_id: candidate.document_id,
                    };
                    match self.run_finance(p, clock, policy, &r.as_of, &selection, &r.actor) {
                        Ok(v) => r.expenses_booked.push(
                            serde_json::from_value(v["expense"].clone())
                                .map_err(|_| RunFailure::protocol())?,
                        ),
                        Err(f) if !f.unknown && !f.authority() => {
                            let proposal=Proposal{kind:"AGENT_BOOKING_BLOCKED".into(),severity:"high".into(),message:format!("Booking bank transaction {} against document {} was blocked by the ledger: {}",summary.bank_transaction_id,summary.document_id,f.message),required_action:Some("Resolve the guardrail (e.g. validate the supplier VAT id) then re-run the agent.".into()),related_bank_transaction_id:Some(summary.bank_transaction_id),related_document_id:Some(summary.document_id),source_evidence:serde_json::json!({"rule":RULE,"ledgerErrors":[f.message]}),posting_preview:None};
                            self.run_action(
                                p,
                                clock,
                                Action::Proposal(&proposal),
                                &r.as_of,
                                Some(&r.actor),
                            )?;
                        }
                        Err(f) => return Err(f),
                    }
                }
            }
        }
        r.phases.push("payables".into());
        let Output::Payables(steps) =
            self.run_action(p, clock, Action::Payables(policy), &r.as_of, Some(&r.actor))?
        else {
            return Err(RunFailure::protocol());
        };
        for step in steps {
            match step {
                PayableStep::Route(proposal) => {
                    self.run_action(
                        p,
                        clock,
                        Action::Proposal(&proposal),
                        &r.as_of,
                        Some(&r.actor),
                    )?;
                }
                PayableStep::Pay { candidate, .. } => {
                    let s = Selection::Pay {
                        bank_id: candidate.bank_transaction_id,
                        payable_id: candidate.payable_id,
                    };
                    match self.run_finance(p, clock, policy, &r.as_of, &s, &r.actor) {
                        Ok(v) => r.payables_matched.push(
                            serde_json::from_value(v["payable"].clone())
                                .map_err(|_| RunFailure::protocol())?,
                        ),
                        Err(f) if !f.unknown && !f.authority() => {}
                        Err(f) => return Err(f),
                    }
                }
            }
        }
        self.run_action(p, clock, Action::Overdue, &r.as_of, Some(&r.actor))?;
        r.phases.push("reconcile".into());
        self.run_action(p, clock, Action::Reconcile, &r.as_of, Some(&r.actor))?;
        r.phases.push("deadlines".into());
        let Output::Review(review) =
            self.run_action(p, clock, Action::Deadlines, &r.as_of, Some(&r.actor))?
        else {
            return Err(RunFailure::protocol());
        };
        r.accrual_recognitions_due = review.accrual_recognitions_due;
        r.upcoming_deadlines = review.upcoming_deadlines;
        r.phases.push("report".into());
        let Output::Exceptions(exceptions) =
            self.run_action(p, clock, Action::Report, &r.as_of, Some(&r.actor))?
        else {
            return Err(RunFailure::protocol());
        };
        r.open_exceptions = exceptions;
        run::build_summary(&mut r);
        Ok(r)
    }
}
#[cfg(test)]
mod tests;
