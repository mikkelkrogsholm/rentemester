//! Source-linked accruals; shared server schedule and writer own all dates/amounts.
use crate::{
    ApiClient, Error, company_path,
    domain::{body, object},
};
use reqwest::Method;
use serde_json::{Value, json};
pub const MAX_ACCRUAL_BODY: usize = 256 * 1024;
#[derive(Clone, Copy, Debug)]
pub enum AccrualMutation {
    Register,
    Recognize(i64),
}
impl AccrualMutation {
    fn path(self) -> Result<String, Error> {
        match self {
            Self::Register => Ok("accruals".into()),
            Self::Recognize(id) => {
                if !(1..=9_007_199_254_740_991).contains(&id) {
                    return Err(Error::InvalidInput("positive exact accrual id required"));
                }
                Ok(format!("accruals/{id}/recognize"))
            }
        }
    }
}
pub fn accrual_body_schema(op: AccrualMutation) -> Value {
    let date = json!({"type":"string","pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"});
    let account = json!({"type":"string","description":"Explicit current account from canonical accounts; server validates type/normal balance/direct posting"});
    let uint = json!({"type":"integer","minimum":0,"maximum":4294967295u64});
    match op {
        AccrualMutation::Register => object(
            json!({"accrualType":{"type":"string","enum":["prepaid_expense","accrued_expense","deferred_revenue"]},"description":{"type":"string","minLength":1,"maxLength":500},"totalAmount":{"type":"number","exclusiveMinimum":0},"recognitionPeriods":uint,"firstRecognitionDate":date,"registrationDate":date,"periodStepMonths":uint,"resultAccountNo":account,"balanceAccountNo":account,"settlementAccountNo":account,"documentId":{"type":"integer","minimum":1,"maximum":9007199254740991u64},"note":{"type":["string","null"],"maxLength":4096},"confirm":{"const":true}}),
            &[
                "accrualType",
                "description",
                "totalAmount",
                "recognitionPeriods",
                "firstRecognitionDate",
                "registrationDate",
                "periodStepMonths",
                "resultAccountNo",
                "balanceAccountNo",
                "settlementAccountNo",
                "documentId",
                "confirm",
            ],
        ),
        AccrualMutation::Recognize(_) => object(
            json!({"periodIndex":uint,"transactionDate":date,"settlementAccountNo":{"type":["string","null"],"description":"Explicit active asset/debit settlement account required for accrued_expense recognition; server-owned policy"},"confirm":{"const":true}}),
            &["periodIndex", "transactionDate", "confirm"],
        ),
    }
}
impl ApiClient {
    pub async fn accruals(&self, company: &str) -> Result<Value, Error> {
        self.get(&company_path(company, "accruals")?, &[]).await
    }
    pub async fn accrual_schedule(&self, company: &str, id: i64) -> Result<Value, Error> {
        if !(1..=9_007_199_254_740_991).contains(&id) {
            return Err(Error::InvalidInput("positive exact accrual id required"));
        }
        self.get(
            &company_path(company, &format!("accruals/{id}/schedule"))?,
            &[],
        )
        .await
    }
    /// Exact bytes once, no keys/retries. Registration posts its own journal with no dedup promise.
    /// Recognition requires explicit reviewed schedule period/date; unknown outcomes require reads.
    pub async fn accrual_mutation(
        &self,
        company: &str,
        op: AccrualMutation,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if !confirmed {
            return Err(Error::InvalidInput(
                "explicit accrual confirmation required",
            ));
        }
        let path = op.path()?;
        if bytes.len() > MAX_ACCRUAL_BODY {
            return Err(Error::InvalidInput("accrual body exceeds native 256KiB"));
        }
        body(&bytes, &accrual_body_schema(op))?;
        self.request(
            Method::POST,
            &company_path(company, &path)?,
            &[],
            Some(bytes),
        )
        .await
    }
}
