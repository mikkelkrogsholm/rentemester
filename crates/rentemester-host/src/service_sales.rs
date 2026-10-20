//! Service sales use the same admitted physical owner and namespace as human sales.
use super::*;
use rentemester_application::sales::{Output, Request};
impl Owner {
    pub fn sales_service(
        &mut self,
        key: &ServiceKey,
        clock: &dyn Clock,
        request: Request<'_>,
    ) -> Result<
        Attempt<Result<service_company::ServiceResult<Output>, service_company::ServiceFailure>>,
        OwnerError,
    > {
        self.admit()?;
        let slots = self.held.0.as_mut().ok_or(OwnerError::Retired)?;
        let outcome = rentemester_application::sales::execute_service(
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
            request,
        );
        let reusable = outcome.as_ref().map_or_else(
            |f| f.control_reusable && f.company_reusable,
            |v| v.control_reusable,
        );
        let retired = self.finish(reusable);
        Ok(Attempt { outcome, retired })
    }
}
