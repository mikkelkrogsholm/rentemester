//! Fresh onboarding uses the lifetime Owner connections, never lock-alias opens.
use crate::{Owner, OwnerError};
impl Owner {
    /// Only invoked immediately after create_new_with_profile by the trusted installer.
    /// The storage gate refuses an initialized chart/audit or any posted history.
    pub fn initialize_original716(
        &mut self,
        operator: &rentemester_runtime::bootstrap::Operator,
        settings: &rentemester_storage::company_creation::Settings,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        if operator.workspace() != self.workspace || settings.slug != self.slug {
            return Err(OwnerError::Context.into());
        }
        self.admit()?;
        let result = rentemester_storage::original716::bootstrap::initialize(
            self.held.0.as_mut().unwrap().company.as_mut().unwrap(),
            settings,
            operator.actor(),
        );
        self.finish(result.is_ok());
        result.map_err(Into::into)
    }
}
