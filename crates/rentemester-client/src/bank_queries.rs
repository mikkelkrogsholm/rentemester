//! Original bank reads: one canonical GET, with all filters and accounting on the server.
use crate::{ApiClient, Error, company_path};
use reqwest::Method;
use serde_json::Value;
#[derive(Clone, Default)]
pub struct BankQuery {
    pub status: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub text_match: Option<String>,
    /// Original currency decimal units, forwarded without rounding or DKK conversion.
    pub amount: Option<String>,
    pub account: Option<String>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}
impl BankQuery {
    fn pairs(&self, reconcile: bool) -> Result<Vec<(&'static str, String)>, Error> {
        if self.limit.is_some_and(|n| !(1..=5000).contains(&n))
            || self.offset.is_some_and(|n| n > 9007199254740991)
            || self
                .amount
                .as_ref()
                .is_some_and(|s| s.trim().parse::<f64>().ok().is_none_or(|n| !n.is_finite()))
        {
            return Err(Error::InvalidInput(
                "finite original-unit amount, limit1..5000 and safe nonnegative offset required",
            ));
        }
        if reconcile
            && (self.from.is_none()
                || self.to.is_none()
                || self.limit.is_some()
                || self.offset.is_some())
        {
            return Err(Error::InvalidInput(
                "reconciliation requires from/to and forbids pagination",
            ));
        }
        let mut pairs = vec![];
        for (key, value) in [
            ("status", &self.status),
            ("from", &self.from),
            ("to", &self.to),
            ("textMatch", &self.text_match),
            ("amount", &self.amount),
            ("account", &self.account),
        ] {
            if let Some(value) = value {
                pairs.push((key, value.clone()));
            }
        }
        if let Some(n) = self.limit {
            pairs.push(("limit", n.to_string()));
        }
        if let Some(n) = self.offset {
            pairs.push(("offset", n.to_string()));
        }
        Ok(pairs)
    }
}
impl ApiClient {
    pub async fn bank_transactions(
        &self,
        company: &str,
        query: &BankQuery,
    ) -> Result<Value, Error> {
        self.bank_query(company, "bank/transactions", query.pairs(false)?)
            .await
    }
    pub async fn bank_reconciliation(
        &self,
        company: &str,
        query: &BankQuery,
    ) -> Result<Value, Error> {
        self.bank_query(company, "bank/reconciliation", query.pairs(true)?)
            .await
    }
    pub async fn bank_account_list(
        &self,
        company: &str,
        include_inactive: Option<bool>,
    ) -> Result<Value, Error> {
        self.bank_query(
            company,
            "bank/accounts",
            include_inactive
                .map(|b| vec![("includeInactive", b.to_string())])
                .unwrap_or_default(),
        )
        .await
    }
    async fn bank_query(
        &self,
        company: &str,
        path: &str,
        pairs: Vec<(&str, String)>,
    ) -> Result<Value, Error> {
        let query = pairs
            .iter()
            .map(|(k, v)| (*k, v.as_str()))
            .collect::<Vec<_>>();
        let mut response = self
            .send(
                Method::GET,
                &company_path(company, path)?,
                &query,
                None,
                self.cookie.as_ref().map(|c| c.as_str()),
            )
            .await?;
        if !response.status().is_success() {
            return Err(Error::Refused {
                status: response.status().as_u16(),
            });
        }
        let mut bytes = vec![];
        while let Some(chunk) = response.chunk().await.map_err(|_| Error::Transport)? {
            if bytes.len().saturating_add(chunk.len()) > 96 * 1024 * 1024 {
                return Err(Error::Protocol);
            }
            bytes.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| Error::Protocol)?;
        if !value["ok"].is_boolean()
            || !value["errors"]
                .as_array()
                .is_some_and(|a| a.iter().all(Value::is_string))
            || value.get("code").is_some_and(|c| !c.is_string())
            || value
                .get("appliedRules")
                .is_some_and(|a| !a.as_array().is_some_and(|a| a.iter().all(Value::is_string)))
        {
            return Err(Error::Protocol);
        }
        if value["ok"] == false {
            if value.get("data").is_some() {
                return Err(Error::Protocol);
            }
            return Ok(value);
        }
        let data = &value["data"];
        let valid = match path {
            "bank/transactions" => data["rows"].is_array() && data["count"].is_u64(),
            "bank/accounts" => data["accounts"].is_array() && data["count"].is_u64(),
            _ => {
                ["periodStart", "periodEnd"]
                    .iter()
                    .all(|k| data[k].is_string())
                    && ["matchedCount", "unmatchedCount"]
                        .iter()
                        .all(|k| data[k].is_u64())
                    && ["matched", "unmatched"].iter().all(|k| data[k].is_array())
                    && ["matchedAmountTotal", "unmatchedAmountTotal"]
                        .iter()
                        .all(|k| !data[k].is_null())
            }
        };
        if !data.is_object() || !valid {
            return Err(Error::Protocol);
        }
        Ok(value)
    }
}
