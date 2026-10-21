//! Company history reads through the admitted physical owner and current subject.
use super::*;
use rentemester_storage::audit_log::Query;
impl Owner {
    pub fn audit_log(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        query: &Query,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |control, company, _, context| {
            rentemester_application::audit_log::read(control, company, context, query)
        })
    }
    pub fn audit_log_service(
        &mut self,
        key: &ServiceKey,
        clock: &dyn Clock,
        query: &Query,
    ) -> Result<
        Attempt<Result<service_company::ServiceResult, service_company::ServiceFailure>>,
        OwnerError,
    > {
        self.admit()?;
        let slots = self.held.0.as_mut().ok_or(OwnerError::Retired)?;
        let outcome = rentemester_application::audit_log::read_service(
            slots.control.as_mut().ok_or(OwnerError::Retired)?,
            slots.company.as_mut().ok_or(OwnerError::Retired)?,
            service_company::Context {
                key,
                clock,
                max_future_days: 45,
                selection: CompanySelection {
                    workspace: &self.workspace,
                    slug: &self.slug,
                    registered: true,
                    archived: false,
                },
            },
            query,
        );
        let reusable = outcome.as_ref().map_or_else(
            |failure| failure.control_reusable && failure.company_reusable,
            |value| value.control_reusable,
        );
        let retired = self.finish(reusable);
        Ok(Attempt { outcome, retired })
    }
}
