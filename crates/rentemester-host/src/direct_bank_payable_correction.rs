//! Fixed source-bound correction on the admitted Owner. No public callbacks or path reopeners.
use crate::*;
impl Owner {
    pub fn direct_bank_payable_correction(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        action: rentemester_application::direct_bank_payable_correction::Action<'_>,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, g, context| {
            rentemester_application::direct_bank_payable_correction::execute(
                c, l, g, context, action,
            )
        })
    }
    pub fn direct_bank_payable_correction_service(
        &mut self,
        key: &ServiceKey,
        clock: &dyn Clock,
        action: rentemester_application::direct_bank_payable_correction::Action<'_>,
    ) -> Result<
        Attempt<Result<service_company::ServiceResult, service_company::ServiceFailure>>,
        OwnerError,
    > {
        self.admit()?;
        let slots = self.held.0.as_mut().ok_or(OwnerError::Retired)?;
        let outcome = rentemester_application::direct_bank_payable_correction::execute_service(
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
#[cfg(all(test, target_os = "macos"))]
mod tests;
