//! Transaction-local shared orchestration; only trusted current-principal adapters call this.
use crate::{journal_post, journal_reversal};
use rentemester_engine::{Error, fiscal_year::Date};
use rentemester_runtime::auth::Clock;
use rentemester_storage::{
    idempotency::{self, Input as ReceiptInput, Operation, Outcome, PrincipalKind, Receipt},
    journal_post::Actor,
};
use rusqlite::Transaction;
use serde_json::{Value, json};
#[derive(Clone, Copy)]
pub(crate) enum Command<'a> {
    Post(&'a journal_post::Input),
    Reverse(&'a journal_reversal::Input),
}
impl<'a> Command<'a> {
    pub(crate) fn validate(self) -> Result<(), Error> {
        let valid = match self {
            Self::Post(i) => {
                i.format == journal_post::FORMAT && i.request.created_by_program.is_none()
            }
            Self::Reverse(i) => i.format == journal_reversal::FORMAT,
        };
        if !valid
            || serde_json::to_vec(&self.request()?)
                .map_err(|_| invalid())?
                .len()
                > rentemester_storage::journal_preview::MAX_REQUEST_BYTES as usize
        {
            return Err(invalid());
        }
        if let Some(key) = self.key() {
            idempotency::validate_key(key)?;
        }
        Ok(())
    }
    fn key(self) -> Option<&'a str> {
        match self {
            Self::Post(i) => i.idempotency_key.as_deref(),
            Self::Reverse(i) => i.idempotency_key.as_deref(),
        }
    }
    fn request(self) -> Result<Value, Error> {
        match self {
            Self::Post(i) => serde_json::to_value(&i.request),
            Self::Reverse(i) => serde_json::to_value(&i.request),
        }
        .map_err(|_| invalid())
    }
    fn format(self) -> &'a str {
        match self {
            Self::Post(i) => &i.format,
            Self::Reverse(i) => &i.format,
        }
    }
    fn version(self) -> &'a str {
        match self {
            Self::Post(i) => &i.expected_rule_version,
            Self::Reverse(i) => &i.expected_rule_version,
        }
    }
}
fn invalid() -> Error {
    Error::new("invalid_input", "manual mutation envelope refused")
}
pub(crate) struct MutationResult {
    pub result: Value,
    pub receipt: Option<Receipt>,
}
pub(crate) fn apply(
    tx: &Transaction<'_>,
    subject: &str,
    kind: PrincipalKind,
    clock: &dyn Clock,
    max_future_days: u32,
    command: Command<'_>,
    business: &mut Option<Value>,
) -> Result<MutationResult, Error> {
    rentemester_storage::journal_post::require_current(tx)?;
    // Historical opening proof precedes receipt replay; current planning is not historical proof.
    rentemester_storage::opening_balance::validate_history(tx)?;
    let now = clock.now_unix_ms().map_err(|e| {
        Error::new(
            e.code(),
            match command {
                Command::Post(_) => "posting clock unavailable",
                Command::Reverse(_) => "reversal clock unavailable",
            },
        )
    })?;
    let latest =
        rentemester_runtime::posting_time::latest_date(now, max_future_days).map_err(|e| {
            Error::new(
                e.code(),
                match command {
                    Command::Post(_) => "posting horizon unavailable",
                    Command::Reverse(_) => "reversal horizon unavailable",
                },
            )
        })?;
    let latest = Date::parse(&latest)?;
    let actor = format!("user:{subject}");
    let program = match kind {
        PrincipalKind::User => "rentemester-rust",
        PrincipalKind::ServiceAccount => "service-principal",
    };
    let effect = |tx: &Transaction<'_>| {
        let actor = Actor {
            subject: &actor,
            program,
        };
        match command {
            Command::Post(i) => rentemester_storage::journal_post::post(
                tx,
                &i.request,
                &i.expected_rule_version,
                latest,
                actor,
            ),
            Command::Reverse(i) => rentemester_storage::journal_reversal::reverse(
                tx,
                &i.request,
                &i.expected_rule_version,
                latest,
                actor,
            ),
        }
    };
    if let Some(key) = command.key() {
        let payload = json!({"format":command.format(),"expectedRuleVersion":command.version(),"request":command.request()?});
        let operation = match command {
            Command::Post(_) => Operation::JournalPost,
            Command::Reverse(_) => Operation::JournalReverse,
        };
        match idempotency::execute(
            tx,
            ReceiptInput {
                key,
                operation,
                principal_kind: kind,
                subject,
                payload: &payload,
                created_by: &actor,
                created_by_program: program,
                now_ms: now,
            },
            effect,
        ) {
            Ok(Outcome::Accepted { result, receipt }) => Ok(MutationResult {
                result,
                receipt: Some(receipt),
            }),
            Ok(Outcome::Refused { code }) => Ok(MutationResult {
                result: json!({"ok":false,"code":code,"errors":[code]}),
                receipt: None,
            }),
            Err(f) => {
                *business = f.rejection;
                Err(f.error)
            }
        }
    } else {
        let value = effect(tx)?;
        if value.get("ok") == Some(&Value::Bool(false)) {
            *business = Some(value);
            return Err(Error::new(
                "business_rejection",
                "manual journal refused; abort transaction",
            ));
        }
        Ok(MutationResult {
            result: value,
            receipt: None,
        })
    }
}
