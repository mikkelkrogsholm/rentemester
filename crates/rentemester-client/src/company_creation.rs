//! Workspace-scoped fresh company creation. All defaults and authorization belong to the server.
use crate::{
    ApiClient, Error,
    domain::{body, object},
};
use reqwest::Method;
use serde_json::{Value, json};
pub const MAX_COMPANY_CREATION_BODY: usize = 16 * 1024;
pub fn company_creation_body_schema() -> Value {
    let text = json!({"type":["string","null"]});
    let mut payment = object(
        json!({"bankName":text,"registrationNo":text,"accountNo":text,"iban":text}),
        &[],
    );
    payment["type"] = json!(["object", "null"]);
    object(
        json!({
            "name":{"type":"string","minLength":1,"maxLength":1024},
            "slug":text,"cvr":text,
            "fiscalYearStartMonth":{"type":["string","null"],"description":"Native string month 1 through 12; omitted uses server default"},
            "fiscalYearLabelStrategy":{"type":["string","null"],"description":"Native end-year/start-year/span; server validates"},
            "vatPeriodType":{"type":["string","null"],"description":"Native month/quarter/half-year or explicit null for no VAT registration; omitted uses server default"},
            "payment":payment
        }),
        &["name"],
    )
}
impl ApiClient {
    /// Send exact reviewed bytes once; no company selector, copied chart, actor, key or retry.
    /// On an unknown outcome inspect companies/current profile before further explicit action.
    pub async fn create_company(&self, bytes: Vec<u8>, confirmed: bool) -> Result<Value, Error> {
        if !confirmed {
            return Err(Error::InvalidInput(
                "explicit new-company confirmation required",
            ));
        }
        if bytes.len() > MAX_COMPANY_CREATION_BODY {
            return Err(Error::InvalidInput("company creation exceeds native16KiB"));
        }
        body(&bytes, &company_creation_body_schema())?;
        self.request(Method::POST, "/api/companies", &[], Some(bytes))
            .await
    }
}
