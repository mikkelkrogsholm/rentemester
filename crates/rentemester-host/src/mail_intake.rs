//! Fixed two-phase receive-only IMAP operations. No provider network under Owner.
use super::*;
use rentemester_application::mail_intake as app;
use rentemester_runtime::imap;
impl Owner {
    pub fn mail_intake(
        &mut self,
        cookies: &rentemester_runtime::auth::CookieVerifier,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        key: Option<&ServiceKey>,
        action: app::Action<'_>,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, g, context| {
            app::execute(c, l, g, cookies, context, key, action)
        })
    }
    pub fn prepare_mail_poll(
        &mut self,
        cookies: &rentemester_runtime::auth::CookieVerifier,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
    ) -> Result<
        Attempt<Result<Committed<app::Admission>, rentemester_application::control_fence::Failure>>,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, g, context| {
            app::prepare(c, l, g, cookies, context)
        })
    }
    pub fn finish_mail_poll(
        &mut self,
        cookies: &rentemester_runtime::auth::CookieVerifier,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        admission: app::Admission,
        outcome: Result<imap::Batch, imap::Failure>,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, g, context| {
            app::finish(c, l, g, cookies, context, admission, outcome)
        })
    }
    fn service_mail<T>(
        &mut self,
        work: impl FnOnce(
            &mut rusqlite::Connection,
            &mut rusqlite::Connection,
            &mut NamespaceGuard,
            CompanySelection<'_>,
        )
            -> Result<Committed<T>, rentemester_application::control_fence::Failure>,
    ) -> Result<
        Attempt<Result<Committed<T>, rentemester_application::control_fence::Failure>>,
        OwnerError,
    > {
        self.admit()?;
        let slots = self.held.0.as_mut().unwrap();
        let outcome = work(
            slots.control.as_mut().unwrap(),
            slots.company.as_mut().unwrap(),
            &mut slots.guard,
            CompanySelection {
                workspace: &self.workspace,
                slug: &self.slug,
                registered: true,
                archived: false,
            },
        );
        let reusable = outcome.as_ref().map_or_else(
            |f| f.control_reusable && f.company_reusable,
            |v| v.control_reusable,
        );
        let retired = self.finish(reusable);
        Ok(Attempt { outcome, retired })
    }
    pub fn mail_poll_candidates(
        &mut self,
        key: &ServiceKey,
        clock: &dyn Clock,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.service_mail(|c, l, g, s| app::candidates(c, l, g, key, clock, s))
    }
    pub fn prepare_service_mail_poll(
        &mut self,
        key: &ServiceKey,
        clock: &dyn Clock,
    ) -> Result<
        Attempt<Result<Committed<app::Admission>, rentemester_application::control_fence::Failure>>,
        OwnerError,
    > {
        self.service_mail(|c, l, g, s| app::prepare_service(c, l, g, key, clock, s))
    }
    pub fn finish_service_mail_poll(
        &mut self,
        key: &ServiceKey,
        clock: &dyn Clock,
        admission: app::Admission,
        outcome: Result<imap::Batch, imap::Failure>,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.service_mail(|c, l, g, s| {
            app::finish_service(c, l, g, key, clock, s, admission, outcome)
        })
    }
}
#[cfg(all(test, target_os = "macos"))]
mod tests;

mod original717;
