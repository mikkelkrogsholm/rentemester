//! Bounded per-effect AgentRun finance operations on the existing Owner namespace.
//! These are not a complete run and are deliberately not advertised as the HTTP run route.
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
