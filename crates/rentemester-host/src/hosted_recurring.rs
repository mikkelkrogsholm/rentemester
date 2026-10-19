//! Fixed hosted operations on the existing held Owner, lease and selected-company guard.
use super::*;
use rentemester_application::hosted_recurring;
impl Owner {
    pub fn hosted_recurring(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        key: Option<&ServiceKey>,
        action: hosted_recurring::Action<'_>,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |control, company, guard, context| {
            hosted_recurring::execute(control, company, guard, context, key, action)
        })
    }
    pub fn hosted_recurring_candidates(
        &mut self,
        key: &ServiceKey,
        clock: &dyn Clock,
        limit: usize,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.admit()?;
        let slots = self.held.0.as_mut().unwrap();
        let outcome = hosted_recurring::candidates(
            slots.control.as_mut().unwrap(),
            slots.company.as_mut().unwrap(),
            key,
            clock,
            CompanySelection {
                workspace: &self.workspace,
                slug: &self.slug,
                registered: true,
                archived: false,
            },
            limit,
        );
        let reusable = outcome.as_ref().map_or_else(
            |f| f.control_reusable && f.company_reusable,
            |v| v.control_reusable,
        );
        let retired = self.finish(reusable);
        Ok(Attempt { outcome, retired })
    }
    pub fn dispatch_hosted_recurring(
        &mut self,
        key: &ServiceKey,
        clock: &dyn Clock,
        template_id: i64,
    ) -> Result<
        Attempt<Result<service_company::ServiceResult, service_company::ServiceFailure>>,
        OwnerError,
    > {
        self.admit()?;
        let slots = self.held.0.as_mut().unwrap();
        let outcome = hosted_recurring::dispatch(
            slots.control.as_mut().unwrap(),
            slots.company.as_mut().unwrap(),
            &mut slots.guard,
            key,
            clock,
            CompanySelection {
                workspace: &self.workspace,
                slug: &self.slug,
                registered: true,
                archived: false,
            },
            template_id,
        );
        let reusable = outcome.as_ref().map_or_else(
            |f| f.control_reusable && f.company_reusable,
            |v| v.control_reusable,
        );
        let retired = self.finish(reusable);
        Ok(Attempt { outcome, retired })
    }
}
#[cfg(test)]
mod tests;
