//! Fixed batch entrypoint on the already admitted Owner; no per-item openers/commits.
use crate::*;
impl Owner {
    pub fn bookkeeping_batch(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        action: rentemester_application::bookkeeping_batch::Action<'_>,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, guard, context| {
            rentemester_application::bookkeeping_batch::execute(c, l, guard, context, action)
        })
    }
}
#[cfg(all(test, target_os = "macos"))]
mod tests;
