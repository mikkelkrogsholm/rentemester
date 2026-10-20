//! Fixed phase boundaries; provider I/O stays outside the Owner.
use super::*;
use rentemester_application::public_einvoice::{self as app, Admission, Intent, Next, Outcome};
impl Owner {
    pub fn public_einvoice_submissions(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, _g, context| {
            app::submissions(c, l, context)
        })
    }

    pub fn public_einvoice_configuration(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        environment: Option<&str>,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, _g, context| {
            app::configuration(c, l, context, environment)
        })
    }
    pub fn start_public_einvoice(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        intent: Intent,
        environment: &str,
    ) -> Result<
        Attempt<Result<Committed<Next>, rentemester_application::control_fence::Failure>>,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, g, context| {
            app::start(c, l, g, context, intent, environment)
        })
    }
    pub fn finish_public_einvoice(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        admission: Admission,
        outcome: Outcome,
    ) -> Result<
        Attempt<Result<Committed<Next>, rentemester_application::control_fence::Failure>>,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, g, context| {
            app::finish(c, l, g, context, admission, outcome)
        })
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests;
