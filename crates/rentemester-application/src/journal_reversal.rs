//! Authenticated append-only manual reversal on admitted connections; no product opener or transport.
use crate::{
    access::Permission,
    control_fence::{
        self, Committed, CompanySelection, CookieAuthorization, Failure, FailureStage,
        SessionRequirement,
    },
};
use rentemester_engine::{Error, fiscal_year::Date};
use rentemester_runtime::auth::{Clock, VerifiedCookie};
use rentemester_storage::{
    idempotency::{self, Input as ReceiptInput, Operation, Outcome, PrincipalKind, Receipt},
    journal_post::{self, Actor},
};
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::{Value, json};
pub const FORMAT: &str = "rentemester-manual-journal-reversal/1";
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Input {
    pub format: String,
    pub expected_rule_version: String,
    pub idempotency_key: Option<String>,
    pub request: rentemester_storage::journal_reversal::Input,
}
pub struct Policy {
    pub max_future_days: u32,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            max_future_days: 45,
        }
    }
}
pub struct ReversalResult {
    pub result: Value,
    pub receipt: Option<Receipt>,
}
pub struct ReversalFailure {
    pub fence: Failure,
    pub business: Option<Value>,
}
impl std::fmt::Debug for ReversalFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReversalFailure")
            .field("fence", &self.fence)
            .field("has_business_rejection", &self.business.is_some())
            .finish()
    }
}
fn invalid() -> ReversalFailure {
    ReversalFailure {
        fence: Failure {
            stage: FailureStage::NotCommitted,
            error: Error::new(
                "invalid_input",
                "unsupported manual reversal envelope or attribution",
            ),
            control_reusable: false,
            company_reusable: false,
        },
        business: None,
    }
}
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    cookie: &VerifiedCookie,
    clock: &dyn Clock,
    selection: CompanySelection<'_>,
    policy: &Policy,
    input: Input,
) -> Result<Committed<ReversalResult>, ReversalFailure> {
    if input.format != FORMAT {
        return Err(invalid());
    }
    if serde_json::to_vec(&input.request)
        .map_err(|_| invalid())?
        .len()
        > rentemester_storage::journal_preview::MAX_REQUEST_BYTES as usize
    {
        return Err(invalid());
    }
    if let Some(key) = &input.idempotency_key {
        idempotency::validate_key(key).map_err(|_| invalid())?;
    }
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
            journal_post::require_current(tx)?;
            let now = clock
                .now_unix_ms()
                .map_err(|e| Error::new(e.code(), "reversal clock unavailable"))?;
            let latest =
                rentemester_runtime::posting_time::latest_date(now, policy.max_future_days)
                    .map_err(|e| Error::new(e.code(), "reversal horizon unavailable"))?;
            let latest = Date::parse(&latest)?;
            let actor = format!("user:{subject}");
            let apply = |tx: &rusqlite::Transaction<'_>| {
                rentemester_storage::journal_reversal::reverse(
                    tx,
                    &input.request,
                    &input.expected_rule_version,
                    latest,
                    Actor {
                        subject: &actor,
                        program: "rentemester-rust",
                    },
                )
            };
            if let Some(key) = &input.idempotency_key {
                let payload = json!({"format":input.format,"expectedRuleVersion":input.expected_rule_version,"request":input.request});
                match idempotency::execute(
                    tx,
                    ReceiptInput {
                        key,
                        operation: Operation::JournalReverse,
                        principal_kind: PrincipalKind::User,
                        subject,
                        payload: &payload,
                        created_by: &actor,
                        created_by_program: "rentemester-rust",
                        now_ms: now,
                    },
                    apply,
                ) {
                    Ok(Outcome::Accepted { result, receipt }) => Ok(ReversalResult {
                        result,
                        receipt: Some(receipt),
                    }),
                    Ok(Outcome::Refused { code }) => Ok(ReversalResult {
                        result: json!({"ok":false,"code":code,"errors":[code]}),
                        receipt: None,
                    }),
                    Err(f) => {
                        business = f.rejection;
                        Err(f.error)
                    }
                }
            } else {
                let value = apply(tx)?;
                if value.get("ok") == Some(&Value::Bool(false)) {
                    business = Some(value);
                    return Err(Error::new(
                        "business_rejection",
                        "manual journal refused; abort transaction",
                    ));
                }
                Ok(ReversalResult {
                    result: value,
                    receipt: None,
                })
            }
        },
    );
    result.map_err(|fence| ReversalFailure { fence, business })
}

#[cfg(test)]
mod tests;
