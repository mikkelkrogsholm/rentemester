//! Explicit VIES observation and dated immutable EU sales report.
use crate::{ApiClient, Error, company_path, valid_commitment_start};
use reqwest::Method;
use serde_json::{Value, json};
impl ApiClient {
    pub async fn vies_validate(&self, company: &str, vat_or_cvr: &str) -> Result<Value, Error> {
        if vat_or_cvr.trim().is_empty()
            || vat_or_cvr.len() > 64
            || vat_or_cvr.chars().any(char::is_control)
        {
            return Err(Error::InvalidInput("bounded EU VAT number required"));
        }
        self.request(
            Method::POST,
            &company_path(company, "vies/validate")?,
            &[],
            Some(
                serde_json::to_vec(&json!({"vatOrCvr":vat_or_cvr}))
                    .map_err(|_| Error::InvalidInput("VAT JSON unavailable"))?,
            ),
        )
        .await
    }
    pub async fn eu_sales(&self, company: &str, from: &str, to: &str) -> Result<Value, Error> {
        if !valid_commitment_start(from) || !valid_commitment_start(to) || from > to {
            return Err(Error::InvalidInput(
                "ordered real from/to YYYY-MM-DD dates required",
            ));
        }
        self.get(
            &company_path(company, "eu-sales")?,
            &[("from", from), ("to", to)],
        )
        .await
    }
}
