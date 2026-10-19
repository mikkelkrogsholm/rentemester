//! Native reviewed period lifecycle. The server owns every control and state transition.
use crate::{
    ApiClient, Error, company_path,
    domain::{body, object, optional_text},
};
use reqwest::Method;
use serde_json::{Value, json};
#[derive(Clone, Copy, Debug)]
pub enum PeriodMutation {
    Review,
    Close,
    Reopen,
    Report,
}
pub fn period_body_schema(operation: PeriodMutation) -> Value {
    let date = json!({"type":"string","pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"});
    let mut properties = json!({"periodStart":date,"periodEnd":date,"confirm":{"const":true}});
    let mut required = vec!["periodStart", "periodEnd", "confirm"];
    if !matches!(operation, PeriodMutation::Review) {
        properties["kind"] = json!({"type":["string","null"],"enum":["vat_period","vat_quarter","fiscal_year","custom",null],"description":"Omission selects vat_period; VAT bounds must match registered cadence"});
    }
    match operation {
        PeriodMutation::Review => {}
        PeriodMutation::Close => {
            properties["packetHash"] = json!({"type":"string","minLength":1,"description":"Exact hash from the reviewed packet; any changed control invalidates it"});
            properties["reviewId"] = json!({"type":"integer","minimum":1});
            properties["reference"] = optional_text();
            properties["reason"] = optional_text();
            properties["force"] = json!({"type":["boolean","null"],"enum":[false,null],"description":"Forced close is unavailable"});
            required.extend(["packetHash", "reviewId"]);
        }
        PeriodMutation::Reopen => {
            properties["reason"] = json!({"type":"string","minLength":1,"maxLength":4096,"description":"Nonblank justification appended to immutable audit; only closed unreported period"});
            required.push("reason");
        }
        PeriodMutation::Report => {
            properties["packetHash"] = json!({"type":"string","minLength":1,"description":"Original closePacketHash from periods list, not a newly computed readiness hash"});
            properties["reference"] = json!({"type":"string","minLength":1,"maxLength":4096,"description":"Receipt reference for an external filing already completed; no filing/network delivery occurs; immutable once recorded"});
            required.extend(["packetHash", "reference"]);
        }
    }
    object(properties, &required)
}
impl ApiClient {
    pub async fn vat(&self, company: &str, year: Option<&str>) -> Result<Value, Error> {
        if year.is_some_and(|s| s.len() > 128) {
            return Err(Error::InvalidInput("VAT year selector too long"));
        }
        let query = year.map(|y| vec![("year", y)]).unwrap_or_default();
        self.get(&company_path(company, "vat")?, &query).await
    }
    pub async fn periods(&self, company: &str) -> Result<Value, Error> {
        self.get(&company_path(company, "periods")?, &[]).await
    }
    pub async fn period_close_readiness(
        &self,
        company: &str,
        from: &str,
        to: &str,
    ) -> Result<Value, Error> {
        if from.len() != 10 || to.len() != 10 {
            return Err(Error::InvalidInput("explicit ISO period dates required"));
        }
        self.get(
            &company_path(company, "periods/close-readiness")?,
            &[("from", from), ("to", to)],
        )
        .await
    }
    /// Exact native bytes, one attempt. Reconcile unknown outcomes with periods/readiness.
    /// Review appends audit evidence; close requires its still-current hash and id.
    pub async fn period_mutation(
        &self,
        company: &str,
        operation: PeriodMutation,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if !confirmed {
            return Err(Error::InvalidInput(
                "explicit period lifecycle confirmation required",
            ));
        }
        body(&bytes, &period_body_schema(operation))?;
        let action = match operation {
            PeriodMutation::Review => "close-review",
            PeriodMutation::Close => "close",
            PeriodMutation::Reopen => "reopen",
            PeriodMutation::Report => "report",
        };
        self.request(
            Method::POST,
            &company_path(company, &format!("periods/{action}"))?,
            &[],
            Some(bytes),
        )
        .await
    }
}
