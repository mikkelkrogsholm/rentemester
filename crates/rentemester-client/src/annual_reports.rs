//! Read-only annual preparation. All figures, prerequisites and effective locks belong to the server.
use crate::{ApiClient, Error, company_path, valid_commitment_start};
use serde_json::Value;
/// Reuse the existing canonical Gregorian wire-date validator; no fiscal-year derivation.
pub fn valid_annual_bounds(start: &str, end: &str) -> bool {
    valid_commitment_start(start) && valid_commitment_start(end) && start <= end
}
impl ApiClient {
    /// One origin-bound GET. A nested report.ok:false is a valid blocker report, unchanged.
    pub async fn annual_report(
        &self,
        company: &str,
        start: &str,
        end: &str,
    ) -> Result<Value, Error> {
        if !valid_annual_bounds(start, end) {
            return Err(Error::InvalidInput(
                "annual report requires ascending real fiscalYearStart/fiscalYearEnd YYYY-MM-DD dates",
            ));
        }
        self.get(
            &company_path(company, "annual-report")?,
            &[("fiscalYearStart", start), ("fiscalYearEnd", end)],
        )
        .await
    }
}
