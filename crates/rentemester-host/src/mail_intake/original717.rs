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

impl Owner {
    pub fn original717_prepare_mail_ingest(
        &mut self,
        cookies: &rentemester_runtime::auth::CookieVerifier,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        input: Input,
        policy: rentemester_application::documents::ScanPolicy,
        poll: Option<&PollAdmission>,
    ) -> Result717<app::original717::IngestAdmission> {
        self.document_action(cookie, clock, |c, l, _g, context| {
            app::original717::prepare_ingest(c, l, cookies, context, input, policy, poll)
        })
    }
    pub fn original717_finish_mail_ingest(
        &mut self,
        cookies: &rentemester_runtime::auth::CookieVerifier,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        admission: app::original717::IngestAdmission,
        scans: Vec<rentemester_engine::document_scan::ScanEvidence>,
    ) -> Result717<serde_json::Value> {
        self.document_action(cookie, clock, |c, l, g, context| {
            app::original717::finish_ingest(c, l, g, cookies, context, admission, scans)
        })
    }
}
impl Owner {
    pub fn original717_oversized_mail(
        &mut self,
        cookies: &rentemester_runtime::auth::CookieVerifier,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        poll: &PollAdmission,
        uid_validity: u32,
        refusals: &[imap::Refusal],
    ) -> Result717<serde_json::Value> {
        self.document_action(cookie, clock, |c, l, _g, context| {
            app::original717::oversized(c, l, cookies, context, poll, uid_validity, refusals)
        })
    }
}
