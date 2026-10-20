//! Canonical dated OSS base-only report.
use crate::{ApiClient, Error, company_path, valid_commitment_start};
use serde_json::Value;
impl ApiClient {
    pub async fn oss_report(&self, company: &str, from: &str, to: &str) -> Result<Value, Error> {
        if !valid_commitment_start(from) || !valid_commitment_start(to) || from > to {
            return Err(Error::InvalidInput(
                "ordered real from/to YYYY-MM-DD dates required",
            ));
        }
        self.get(
            &company_path(company, "oss-report")?,
            &[("from", from), ("to", to)],
        )
        .await
    }
}
