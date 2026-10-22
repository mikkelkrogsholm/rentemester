//! Original manual journal operations through the current native principal fences.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CompanySelection, CookieAuthorization, SessionRequirement},
    journal_mutation::{self, Command},
    journal_post, journal_reversal, service_company,
};
use rentemester_engine::{Error, fiscal_year::Date, manual_journal::ManualJournal};
use rentemester_runtime::auth::{Clock, VerifiedCookie};
use rentemester_storage::{
    idempotency::PrincipalKind,
    journal_original::{self as storage, Selector},
};
use rusqlite::{Connection, Transaction};
use serde::Deserialize;
use serde_json::{Value, json};

pub enum Action {
    Post(ManualJournal),
    DryRun(ManualJournal),
    Reverse(Reverse),
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Reverse {
    pub entry_id: Option<Value>,
    pub entry_no: Option<String>,
    pub match_text: Option<String>,
    pub match_date: Option<String>,
    pub match_document_id: Option<Value>,
    pub transaction_date: Date,
    pub reason: String,
    pub confirm: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Wire {
    input: Value,
    #[serde(default)]
    confirm: bool,
}
pub fn parse(operation: &str, bytes: &[u8]) -> Result<Action, Error> {
    let invalid = || Error::new("invalid_input", "invalid original journal request");
    if bytes.len() > 65_536 {
        return Err(invalid());
    }
    if operation == "reverse" {
        let value: Reverse = serde_json::from_slice(bytes).map_err(|_| invalid())?;
        if !value.confirm || value.reason.trim().is_empty() {
            return Err(invalid());
        }
        return Ok(Action::Reverse(value));
    }
    let wire: Wire = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    let input = storage::adapt(wire.input)
        .map_err(|error| Error::new("journal_input_refused", error.message))?;
    match operation {
        "post" if wire.confirm => Ok(Action::Post(input)),
        "dry-run" => Ok(Action::DryRun(input)),
        _ => Err(invalid()),
    }
}
fn apply(
    tx: &Transaction<'_>,
    subject: &str,
    kind: PrincipalKind,
    clock: &dyn Clock,
    max_future_days: u32,
    action: Action,
    business: &mut Option<Value>,
) -> Result<Value, Error> {
    let version = rentemester_storage::journal_preview::manual_journal_rule_version()?;
    match action {
        Action::Post(request) | Action::DryRun(request) => {
            // Mode is handled by the caller below before this branch.
            let input = journal_post::Input {
                format: journal_post::FORMAT.into(),
                expected_rule_version: version,
                idempotency_key: None,
                request,
                recovery_required: false,
            };
            journal_mutation::apply(
                tx,
                subject,
                kind,
                clock,
                max_future_days,
                Command::Post(&input),
                business,
            )
            .map(|r| r.result)
        }
        Action::Reverse(input) => {
            let selector = Selector {
                entry_id: input.entry_id,
                entry_no: input.entry_no,
                match_text: input.match_text,
                match_date: input.match_date,
                match_document_id: input.match_document_id,
            };
            let entry_id = match storage::resolve(tx, &selector) {
                Ok(id) => id,
                Err(error) if error.code == "journal_selection_usage" => {
                    *business = Some(
                        json!({"ok":false,"code":"journal_selection_usage","appliedRules":[],"errors":[error.message]}),
                    );
                    return Err(error);
                }
                Err(error) => return Err(error),
            };
            let input = journal_reversal::Input {
                format: journal_reversal::FORMAT.into(),
                expected_rule_version: version,
                idempotency_key: None,
                request: rentemester_storage::journal_reversal::Input {
                    entry_id,
                    transaction_date: input.transaction_date,
                    reason: input.reason,
                },
            };
            journal_mutation::apply(
                tx,
                subject,
                kind,
                clock,
                max_future_days,
                Command::Reverse(&input),
                business,
            )
            .map(|r| r.result)
        }
    }
}
fn operation(
    tx: &Transaction<'_>,
    subject: &str,
    kind: PrincipalKind,
    clock: &dyn Clock,
    max_future_days: u32,
    action: Action,
    business: &mut Option<Value>,
) -> Result<Value, Error> {
    if let Action::DryRun(request) = action {
        let version = rentemester_storage::journal_preview::manual_journal_rule_version()?;
        let input = journal_post::Input {
            format: journal_post::FORMAT.into(),
            expected_rule_version: version,
            idempotency_key: None,
            request,
            recovery_required: false,
        };
        storage::dry_run(tx, &input.request, || {
            journal_mutation::apply(
                tx,
                subject,
                kind,
                clock,
                max_future_days,
                Command::Post(&input),
                business,
            )
            .map(|r| r.result)
        })
    } else {
        apply(tx, subject, kind, clock, max_future_days, action, business)
    }
}
pub fn human(
    control: &mut Connection,
    company: &mut Connection,
    cookie: &VerifiedCookie,
    clock: &dyn Clock,
    selection: CompanySelection<'_>,
    action: Action,
) -> Result<Committed<Value>, journal_post::PostFailure> {
    let mut business = None;
    let result = control_fence::execute_cookie_subject(
        control,
        company,
        CookieAuthorization {
            cookie,
            clock,
            requirement: SessionRequirement::Current,
        },
        selection,
        Permission::CompanyLedgerPost,
        |tx, subject| {
            operation(
                tx,
                subject,
                PrincipalKind::User,
                clock,
                45,
                action,
                &mut business,
            )
        },
    );
    result.map_err(|fence| journal_post::PostFailure { fence, business })
}
pub fn service(
    control: &mut Connection,
    company: &mut Connection,
    context: service_company::Context<'_>,
    action: Action,
) -> Result<service_company::ServiceResult, service_company::ServiceFailure> {
    let clock = context.clock;
    let horizon = context.max_future_days;
    service_company::execute_operation(
        control,
        company,
        context,
        Permission::CompanyLedgerPost,
        true,
        |tx, subject, business| {
            operation(
                tx,
                subject,
                PrincipalKind::ServiceAccount,
                clock,
                horizon,
                action,
                business,
            )
            .map(|result| service_company::OperationResult {
                result,
                receipt: None,
            })
        },
    )
}

#[cfg(test)]
mod tests;
