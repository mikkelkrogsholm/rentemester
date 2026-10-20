//! Read-only server-scored bank suggestions. No local scoring or financial action.
use crate::{ApiClient, Error, company_path};
use serde_json::Value;

impl ApiClient {
    /// Preserve the canonical source object, including ok:false for an unknown or
    /// already matched selector. Confidence is a score and may exceed one.
    pub async fn bank_suggest_matches(
        &self,
        company: &str,
        bank_transaction_id: Option<i64>,
        max: Option<usize>,
    ) -> Result<Value, Error> {
        if bank_transaction_id.is_some_and(|id| !(1..=9_007_199_254_740_991).contains(&id))
            || max == Some(0)
        {
            return Err(Error::InvalidInput(
                "positive safe bank transaction id and positive max required",
            ));
        }
        let id = bank_transaction_id.map(|id| id.to_string());
        let max = max.map(|max| max.to_string());
        let mut query = vec![];
        if let Some(id) = id.as_deref() {
            query.push(("bankTransactionId", id));
        }
        if let Some(max) = max.as_deref() {
            query.push(("max", max));
        }
        self.get(&company_path(company, "bank/match-suggestions")?, &query)
            .await
    }
}
