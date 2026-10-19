//! Source snapshots and control effects remain inside the same admitted Owner mutex.
use super::*;
use rentemester_application::eliminations as app;
use serde_json::Value;
impl Owner {
    pub fn eliminations(
        &mut self,
        cookies: &rentemester_runtime::auth::CookieVerifier,
        clock: &dyn Clock,
        proof: &VerifiedCookie,
        anchor: Option<&str>,
        action: app::Action<'_>,
    ) -> AuthAttempt<Value> {
        let catalog = self.catalog()?;
        let prepared = self.control_auth(cookies.workspace(), |c| {
            app::prepare(c, cookies, clock, proof, &catalog, anchor, action)
        })?;
        let prepared = match prepared.outcome {
            Ok(v) => v,
            Err(_) => return Ok(prepared),
        };
        let mut fresh = None;
        if !prepared["snapshot"].is_null() {
            let input: rentemester_engine::eliminations::Input =
                serde_json::from_value(prepared["snapshot"].clone())
                    .map_err(|_| OwnerError::Context)?;
            let report = self.intercompany(
                cookies,
                clock,
                proof,
                None,
                rentemester_application::intercompany::Action::Reconciliation(&input.as_of),
            )?;
            match report.outcome {
                Ok(v) => fresh = Some(v),
                Err(_) => return Ok(report),
            }
        }
        self.control_auth(cookies.workspace(), |c| {
            app::execute(
                c,
                cookies,
                clock,
                proof,
                &catalog,
                anchor,
                action,
                fresh.as_ref(),
            )
        })
    }
}
#[cfg(test)]
#[path = "eliminations_tests.rs"]
mod tests;
