//! Read-only annual preparation. All figures, prerequisites and effective locks belong to the server.
use crate::{ApiClient, Error, company_path, valid_commitment_start};
use serde_json::Value;
/// Reuse the existing canonical Gregorian wire-date validator; no fiscal-year derivation.
pub fn valid_annual_bounds(start: &str, end: &str) -> bool {
    valid_commitment_start(start) && valid_commitment_start(end) && start <= end
}
impl ApiClient {
    /// Current server-owned obligations, including blockers and unavailable coverage.
    /// Omitting year requests the server's current selection; no date is invented here.
    pub async fn obligations(&self, company: &str, year: Option<&str>) -> Result<Value, Error> {
        if year.is_some_and(|label| {
            label.is_empty()
                || label.len() > 128
                || label.trim() != label
                || label.chars().any(|c| c.is_control() || c == '\u{fffd}')
        }) {
            return Err(Error::InvalidInput(
                "bounded unmodified fiscal year label required",
            ));
        }
        let path = company_path(company, "obligations")?;
        match year {
            Some(label) => self.get(&path, &[("year", label)]).await,
            None => self.get(&path, &[]).await,
        }
    }

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
