//! Explicit service payable operations through the same admitted physical namespace.
use super::*;
impl Owner {
    pub fn expense_service(
        &mut self,
        key: &ServiceKey,
        clock: &dyn Clock,
        action: rentemester_application::expense::Action<'_>,
    ) -> Result<
        Attempt<Result<service_company::ServiceResult, service_company::ServiceFailure>>,
        OwnerError,
    > {
        self.admit()?;
        let slots = self.held.0.as_mut().ok_or(OwnerError::Retired)?;
        let outcome = rentemester_application::expense::execute_service(
            slots.control.as_mut().ok_or(OwnerError::Retired)?,
            slots.company.as_mut().ok_or(OwnerError::Retired)?,
            &mut slots.guard,
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
            action,
        );
        let reusable = outcome.as_ref().map_or_else(
            |f| f.control_reusable && f.company_reusable,
            |v| v.control_reusable,
        );
        let retired = self.finish(reusable);
        Ok(Attempt { outcome, retired })
    }
}
