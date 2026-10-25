//! Original backup operations retain the Owner's admitted company handles.
use super::*;
use rentemester_application::backup_original122::{self as app, Action};
use serde_json::Value;
impl Owner {
    pub fn backup_original122(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        action: Action,
    ) -> Result<
        Attempt<Result<Committed<Value>, rentemester_application::control_fence::Failure>>,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, _, ctx| app::human(c, l, ctx, action))
    }
    pub fn backup_original122_service(
        &mut self,
        key: &ServiceKey,
        clock: &dyn Clock,
        action: Action,
    ) -> Result<
        Attempt<Result<service_company::ServiceResult<Value>, service_company::ServiceFailure>>,
        OwnerError,
    > {
        self.service_document_action(key, clock, |c, l, _, ctx| app::service(c, l, ctx, action))
    }
}

mod original717;
#[cfg(test)]
#[path = "backup_original122_tests.rs"]
mod tests;

impl Owner {
    pub fn backup_views_original717(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        configs: &[rentemester_runtime::backup_destination::Config],
        op: rentemester_application::backup_registration::original717::Operation,
    ) -> Result<
        Attempt<Result<Committed<Value>, rentemester_application::control_fence::Failure>>,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, _, ctx| {
            rentemester_application::backup_registration::original717::human(c, l, ctx, configs, op)
        })
    }
    pub fn backup_views_original717_service(
        &mut self,
        key: &ServiceKey,
        clock: &dyn Clock,
        configs: &[rentemester_runtime::backup_destination::Config],
        op: rentemester_application::backup_registration::original717::Operation,
    ) -> Result<
        Attempt<Result<service_company::ServiceResult<Value>, service_company::ServiceFailure>>,
        OwnerError,
    > {
        self.service_document_action(key, clock, |c, l, _, ctx| {
            rentemester_application::backup_registration::original717::service(
                c, l, ctx, configs, op,
            )
        })
    }
}
