//! Fixed recovery operations on the admitted Owner control connection. No database openers.
use super::*;
use rentemester_application::{password_reset, reset_request};
use rentemester_runtime::recovery::{ResetSender, SensitiveUrl};
impl Owner {
    /// Preserve known committed delivery failures separately from an unknown DB commit.
    pub fn request_password_reset(
        &mut self,
        runtime: &reset_request::Runtime<'_>,
        sender: &mut dyn ResetSender,
        email: &str,
        redirect: Option<&str>,
    ) -> Result<Attempt<Result<(), reset_request::Failure>>, OwnerError> {
        self.admit_auth(runtime.cookies.workspace())?;
        let outcome = reset_request::execute(
            self.held.0.as_mut().unwrap().control.as_mut().unwrap(),
            runtime,
            sender,
            email,
            redirect,
        );
        let reusable = outcome
            .as_ref()
            .map_or_else(|f| f.connection_reusable, |_| true);
        let retired = self.finish(reusable);
        Ok(Attempt { outcome, retired })
    }
    /// Read-only callback preview. The final writer must resolve the token again.
    pub fn password_reset_callback(
        &mut self,
        runtime: &reset_request::Runtime<'_>,
        token: &str,
        redirect: Option<&str>,
    ) -> AuthAttempt<SensitiveUrl> {
        self.control_auth(runtime.cookies.workspace(), |control| {
            reset_request::callback(control, runtime, token, redirect)
        })
    }
    pub fn complete_password_reset(
        &mut self,
        runtime: &password_reset::Runtime<'_>,
        token: &str,
        password: &str,
    ) -> AuthAttempt<()> {
        self.admit()?;
        let outcome = password_reset::execute(
            self.held.0.as_mut().unwrap().control.as_mut().unwrap(),
            runtime,
            token,
            password,
        );
        let reusable = outcome
            .as_ref()
            .map_or_else(|f| f.connection_reusable, |_| true);
        let retired = self.finish(reusable);
        Ok(Attempt { outcome, retired })
    }
}
#[cfg(test)]
mod tests;
