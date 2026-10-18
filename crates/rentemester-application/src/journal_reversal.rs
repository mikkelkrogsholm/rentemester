//! Authenticated append-only manual reversal on admitted connections; no product opener or transport.
use crate::{
    access::Permission,
    control_fence::{
        self, Committed, CompanySelection, CookieAuthorization, Failure, FailureStage,
        SessionRequirement,
    },
};
use rentemester_engine::Error;
use rentemester_runtime::auth::{Clock, VerifiedCookie};
use rentemester_storage::idempotency::{PrincipalKind, Receipt};
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::Value;
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
    if crate::journal_mutation::Command::Reverse(&input)
        .validate()
        .is_err()
    {
        return Err(invalid());
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
            crate::journal_mutation::apply(
                tx,
                subject,
                PrincipalKind::User,
                clock,
                policy.max_future_days,
                crate::journal_mutation::Command::Reverse(&input),
                &mut business,
            )
            .map(|value| ReversalResult {
                result: value.result,
                receipt: value.receipt,
            })
        },
    );
    result.map_err(|fence| ReversalFailure { fence, business })
}

#[cfg(test)]
mod tests;
