//! Fixed opening-balance use cases through the current admitted company Owner.
use super::*;
use rentemester_application::opening_balance::{self as app, Action};
impl Owner {
    pub fn opening_balance(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        action: Action<'_>,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, g, context| {
            app::execute(c, l, g, context, action)
        })
    }
}
#[cfg(all(test, target_os = "macos"))]
mod tests;
