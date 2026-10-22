//! Original planning projections run only inside the current held company fence.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
    service_company,
};
pub use budget::Set;
pub use liquidity::{Monthly, Weekly};
use rentemester_engine::Error;
use rentemester_storage::{
    budget_original as budget, idempotency::PrincipalKind, liquidity_original as liquidity,
};
use rusqlite::{Connection, Transaction};
use serde_json::Value;
pub enum Action<'a> {
    Set(&'a Set),
    List {
        period: Option<&'a str>,
        account_no: Option<&'a str>,
    },
    Compare {
        from: &'a str,
        to: &'a str,
    },
    Monthly(&'a Monthly),
    Weekly(&'a Weekly),
    /// Preserve current authority precedence for structural transport refusals.
    Invalid {
        write: bool,
    },
}
impl Action<'_> {
    fn mutation(&self) -> bool {
        matches!(self, Self::Set(_) | Self::Invalid { write: true })
    }
    fn permission(&self) -> Permission {
        if self.mutation() {
            Permission::CompanyDraftWrite
        } else {
            Permission::CompanyRead
        }
    }
}
fn apply(
    control: &Transaction<'_>,
    company: &Transaction<'_>,
    slug: &str,
    subject: &str,
    kind: PrincipalKind,
    action: Action<'_>,
) -> Result<Value, Error> {
    match action {
        Action::Set(input) => budget::set(company, input, subject, kind),
        Action::List { period, account_no } => budget::list(company, period, account_no),
        Action::Compare { from, to } => budget::vs_actual(company, from, to),
        Action::Monthly(input) => liquidity::monthly(company, input),
        Action::Weekly(input) => liquidity::weekly(control, company, slug, input),
        Action::Invalid { .. } => Err(Error::new(
            "invalid_input",
            "Invalid original budget request",
        )),
    }
}
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    context: Context<'_>,
    action: Action<'_>,
) -> Result<Committed<Value>, Failure> {
    let slug = context.selection.slug;
    control_fence::execute_cookie_both(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Current,
        },
        context.selection,
        action.permission(),
        !action.mutation(),
        false,
        |control, company, subject| {
            apply(control, company, slug, subject, PrincipalKind::User, action)
        },
    )
}
pub fn execute_service(
    control: &mut Connection,
    company: &mut Connection,
    context: service_company::Context<'_>,
    action: Action<'_>,
) -> Result<service_company::ServiceResult<Value>, service_company::ServiceFailure> {
    let slug = context.selection.slug;
    service_company::execute_operation_both(
        control,
        company,
        context,
        action.permission(),
        action.mutation(),
        |control, company, subject, _| {
            Ok(service_company::OperationResult {
                result: apply(
                    control,
                    company,
                    slug,
                    subject,
                    PrincipalKind::ServiceAccount,
                    action,
                )?,
                receipt: None,
            })
        },
    )
}
