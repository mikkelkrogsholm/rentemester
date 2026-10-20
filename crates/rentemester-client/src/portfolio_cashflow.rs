//! Canonical current-authority portfolio and imported bank history; no local aggregation.
use crate::{ApiClient, Error, company_path, valid_commitment_start};
use serde_json::Value;
impl ApiClient {
    pub async fn portfolio(&self, as_of: Option<&str>) -> Result<Value, Error> {
        if as_of.is_some_and(|date| !valid_commitment_start(date)) {
            return Err(Error::InvalidInput("asOf must be a real YYYY-MM-DD date"));
        }
        self.get(
            "/api/portfolio",
            &as_of.map(|date| vec![("asOf", date)]).unwrap_or_default(),
        )
        .await
    }
    pub async fn cashflow(&self, company: &str, year: Option<&str>) -> Result<Value, Error> {
        if year.is_some_and(|value| {
            value.is_empty()
                || value.len() > 32
                || value.trim() != value
                || value.chars().any(|c| c.is_control() || c == '\u{fffd}')
        }) {
            return Err(Error::InvalidInput(
                "bounded unchanged fiscal year label required",
            ));
        }
        self.get(
            &company_path(company, "cashflow")?,
            &year.map(|label| vec![("year", label)]).unwrap_or_default(),
        )
        .await
    }
}
