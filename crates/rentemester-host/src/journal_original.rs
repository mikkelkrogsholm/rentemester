//! Original journal operations retain physical owner admission and retirement.
use super::*;
use rentemester_application::journal_original::{self, Action};
impl Owner {
    pub fn journal_original_human(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        action: Action,
    ) -> Result<Attempt<Result<Committed<serde_json::Value>, journal_post::PostFailure>>, OwnerError>
    {
        self.admit()?;
        let slots = self.held.0.as_mut().ok_or(OwnerError::Retired)?;
        let outcome = journal_original::human(
            slots.control.as_mut().ok_or(OwnerError::Retired)?,
            slots.company.as_mut().ok_or(OwnerError::Retired)?,
            cookie,
            clock,
            CompanySelection {
                workspace: &self.workspace,
                slug: &self.slug,
                registered: true,
                archived: false,
            },
            action,
        );
        let reusable = outcome.as_ref().map_or_else(
            |f| f.fence.control_reusable && f.fence.company_reusable,
            |v| v.control_reusable,
        );
        let retired = self.finish(reusable);
        Ok(Attempt { outcome, retired })
    }
    pub fn journal_original_service(
        &mut self,
        key: &ServiceKey,
        clock: &dyn Clock,
        action: Action,
    ) -> Result<
        Attempt<Result<service_company::ServiceResult, service_company::ServiceFailure>>,
        OwnerError,
    > {
        self.admit()?;
        let slots = self.held.0.as_mut().ok_or(OwnerError::Retired)?;
        let outcome = journal_original::service(
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
