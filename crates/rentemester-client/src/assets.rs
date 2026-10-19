//! Source-linked assets over the shared server writer; no depreciation arithmetic here.
use crate::{
    ApiClient, Error, company_path,
    domain::{body, object},
};
use reqwest::Method;
use serde_json::{Value, json};
pub const MAX_ASSET_BODY: usize = 256 * 1024;
#[derive(Clone, Copy, Debug)]
pub enum AssetMutation {
    Register,
    Depreciate(i64),
}
impl AssetMutation {
    fn path(self) -> Result<String, Error> {
        match self {
            Self::Register => Ok("assets".into()),
            Self::Depreciate(id) => {
                if !(1..=9_007_199_254_740_991).contains(&id) {
                    return Err(Error::InvalidInput("positive exact asset id required"));
                }
                Ok(format!("assets/{id}/depreciate"))
            }
        }
    }
}
pub fn asset_body_schema(op: AssetMutation) -> Value {
    let date = json!({"type":["string","null"],"pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"});
    let account = json!({"type":["string","null"],"description":"Explicit current active directly-postable account from canonical accounts; server checks type/normal balance"});
    match op {
        AssetMutation::Register => object(
            json!({"name":{"type":"string","minLength":1,"maxLength":500},"category":{"type":"string","minLength":1,"maxLength":500},"acquisitionDate":{"type":"string","pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"},"cost":{"type":"number","exclusiveMinimum":0},"usefulLifeMonths":{"type":"integer","minimum":1,"maximum":4294967295u64},"purchaseDocumentId":{"type":"integer","minimum":1,"maximum":9007199254740991u64},"assetAccountNo":account,"depreciationExpenseAccountNo":account,"accumulatedDepreciationAccountNo":account,"note":{"type":["string","null"],"maxLength":4096},"confirm":{"const":true}}),
            &[
                "name",
                "category",
                "acquisitionDate",
                "cost",
                "usefulLifeMonths",
                "purchaseDocumentId",
                "confirm",
            ],
        ),
        AssetMutation::Depreciate(_) => object(
            json!({"transactionDate":date,"periodIndex":{"type":["integer","null"],"minimum":0,"maximum":4294967295u64},"confirm":{"const":true}}),
            &["confirm"],
        ),
    }
}
impl ApiClient {
    pub async fn assets(&self, company: &str) -> Result<Value, Error> {
        self.get(&company_path(company, "assets")?, &[]).await
    }
    pub async fn next_asset_depreciation(&self, company: &str, id: i64) -> Result<Value, Error> {
        if !(1..=9_007_199_254_740_991).contains(&id) {
            return Err(Error::InvalidInput("positive exact asset id required"));
        }
        self.get(
            &company_path(company, &format!("assets/{id}/next-depreciation"))?,
            &[],
        )
        .await
    }
    /// Exact bytes once, no keys/retries. Read server preview first and select its explicit period.
    /// Omitting periodIndex selects the next period; unknown outcomes require canonical reads.
    pub async fn asset_mutation(
        &self,
        company: &str,
        op: AssetMutation,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if !confirmed {
            return Err(Error::InvalidInput("explicit asset confirmation required"));
        }
        let path = op.path()?;
        if bytes.len() > MAX_ASSET_BODY {
            return Err(Error::InvalidInput("asset body exceeds native 256KiB"));
        }
        body(&bytes, &asset_body_schema(op))?;
        self.request(
            Method::POST,
            &company_path(company, &path)?,
            &[],
            Some(bytes),
        )
        .await
    }
}
