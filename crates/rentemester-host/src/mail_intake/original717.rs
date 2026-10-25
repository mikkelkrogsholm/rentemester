use super::*;
use app::original717::{Input, PollAdmission};
type Result717<T> = Result<
    Attempt<Result<Committed<T>, rentemester_application::control_fence::Failure>>,
    OwnerError,
>;
impl Owner {
    pub fn original717_mail_intake_ingest(
        &mut self,
        cookies: &rentemester_runtime::auth::CookieVerifier,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        input: &Input,
    ) -> Result717<serde_json::Value> {
        self.document_action(cookie, clock, |c, l, g, context| {
            app::original717::ingest(c, l, g, cookies, context, input)
        })
    }
    pub fn original717_prepare_imap_intake_poll(
        &mut self,
        cookies: &rentemester_runtime::auth::CookieVerifier,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        config: imap::Config,
    ) -> Result717<PollAdmission> {
        self.document_action(cookie, clock, |c, l, _g, context| {
            app::original717::prepare(c, l, cookies, context, config)
        })
    }
    pub fn original717_finish_imap_intake_poll(
        &mut self,
        cookies: &rentemester_runtime::auth::CookieVerifier,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        admission: PollAdmission,
        input: &Input,
    ) -> Result717<serde_json::Value> {
        self.document_action(cookie, clock, |c, l, g, context| {
            app::original717::finish(c, l, g, cookies, context, admission, input)
        })
    }
}
