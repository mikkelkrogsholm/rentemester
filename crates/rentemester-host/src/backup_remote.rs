//! Split authorization around provider reads. No owner lock or SQL transaction leaves these calls.
use super::*;
use rentemester_application::backup_remote as app;
impl Owner {
    pub fn prepare_backup_remote(
        &mut self,
        runtime: &app::Runtime<'_>,
        proof: &VerifiedCookie,
        request: &app::Request,
    ) -> AuthAttempt<app::Preparation> {
        let catalog = self.catalog()?;
        self.control_auth(runtime.local.cookies.workspace(), |control| {
            app::prepare(control, proof, &catalog, runtime, request)
        })
    }
    pub fn finish_backup_remote(
        &mut self,
        runtime: &app::Runtime<'_>,
        proof: &VerifiedCookie,
        prepared: &app::Prepared,
        observation: &rentemester_runtime::backup_remote::Observation,
    ) -> AuthAttempt<serde_json::Value> {
        let catalog = self.catalog()?;
        self.control_auth(runtime.local.cookies.workspace(), |control| {
            app::finish(control, proof, &catalog, runtime, prepared, observation)
        })
    }
    pub fn backup_remote_verification(
        &mut self,
        cookies: &rentemester_runtime::auth::CookieVerifier,
        clock: &dyn Clock,
        proof: &VerifiedCookie,
        destination: &str,
        operation: &str,
    ) -> AuthAttempt<serde_json::Value> {
        let catalog = self.catalog()?;
        self.control_auth(cookies.workspace(), |control| {
            app::inspect(
                control,
                proof,
                &catalog,
                cookies,
                clock,
                destination,
                operation,
            )
        })
    }
}
