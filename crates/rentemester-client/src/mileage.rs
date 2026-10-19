//! User-chosen mileage documentation only; server owns arithmetic/advisories, never journal posting.
use crate::{
    ApiClient, Error, company_path,
    domain::{body, object},
};
use reqwest::Method;
use serde_json::{Value, json};
pub const MAX_MILEAGE_BODY: usize = 32 * 1024;
pub fn mileage_body_schema() -> Value {
    let text = json!({"type":"string","maxLength":4096});
    let optional = json!({"type":["string","null"],"maxLength":4096});
    object(
        json!({"tripDate":{"type":"string","pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"},"purpose":text,"fromLocation":text,"toLocation":text,"kilometers":{"type":"number"},"vehicle":text,"driver":text,"ratePerKm":{"type":"number"},"rateBasis":text,"rateSource":optional,"notes":optional,"confirm":{"const":true}}),
        &[
            "tripDate",
            "purpose",
            "fromLocation",
            "toLocation",
            "kilometers",
            "vehicle",
            "driver",
            "ratePerKm",
            "rateBasis",
            "confirm",
        ],
    )
}
impl ApiClient {
    pub async fn mileage(&self, company: &str, year: &str) -> Result<Value, Error> {
        crate::budget::fiscal_selector(year, 16)?;
        self.get(&company_path(company, "mileage")?, &[("year", year)])
            .await
    }
    /// Exact chosen route/rate/basis once. No tax eligibility inference, keys, duplicate protection or retries.
    pub async fn register_mileage(
        &self,
        company: &str,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if !confirmed {
            return Err(Error::InvalidInput(
                "explicit mileage confirmation required",
            ));
        }
        if bytes.len() > MAX_MILEAGE_BODY {
            return Err(Error::InvalidInput("mileage exceeds native32KiB body"));
        }
        body(&bytes, &mileage_body_schema())?;
        self.request(
            Method::POST,
            &company_path(company, "mileage")?,
            &[],
            Some(bytes),
        )
        .await
    }
}
