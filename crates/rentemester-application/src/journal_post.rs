//! Authenticated real manual posting on admitted connections; no product opener or transport.
use crate::{
    access::Permission,
    control_fence::{
        self, Committed, CompanySelection, CookieAuthorization, Failure, FailureStage,
        SessionRequirement,
    },
};
use rentemester_engine::{Error, manual_journal::ManualJournal};
use rentemester_runtime::auth::{Clock, VerifiedCookie};
use rentemester_storage::idempotency::{PrincipalKind, Receipt};
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::Value;
pub const FORMAT: &str = "rentemester-manual-journal-post/1";
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Input {
    pub format: String,
    pub expected_rule_version: String,
    pub idempotency_key: Option<String>,
    pub request: ManualJournal,
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
pub struct PostResult {
    pub result: Value,
    pub receipt: Option<Receipt>,
}
pub struct PostFailure {
    pub fence: Failure,
    pub business: Option<Value>,
}
impl std::fmt::Debug for PostFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PostFailure")
            .field("fence", &self.fence)
            .field("has_business_rejection", &self.business.is_some())
            .finish()
    }
}
fn invalid() -> PostFailure {
    PostFailure {
        fence: Failure {
            stage: FailureStage::NotCommitted,
            error: Error::new(
                "invalid_input",
                "unsupported manual posting envelope or attribution",
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
) -> Result<Committed<PostResult>, PostFailure> {
    if crate::journal_mutation::Command::Post(&input)
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
                crate::journal_mutation::Command::Post(&input),
                &mut business,
            )
            .map(|value| PostResult {
                result: value.result,
                receipt: value.receipt,
            })
        },
    );
    result.map_err(|fence| PostFailure { fence, business })
}
#[cfg(test)]
mod tests;
