//! Account budget revisions and dimension comparisons; no allocation, totals or accounting here.
use crate::{
    ApiClient, Error, company_path,
    domain::{body, object},
};
use reqwest::Method;
use serde_json::{Value, json};
pub const MAX_BUDGET_BODY: usize = 16 * 1024;
pub fn budget_body_schema() -> Value {
    object(
        json!({"accountNo":{"type":"string"},"period":{"type":"string","pattern":"^[0-9]{4}-[0-9]{2}$"},"amount":{"type":"number"},"notes":{"type":["string","null"],"maxLength":4096}}),
        &["accountNo", "period", "amount"],
    )
}
pub(crate) fn fiscal_selector(year: &str, max: usize) -> Result<(), Error> {
    if year.is_empty() || year.len() > max || year.chars().any(char::is_control) {
        Err(Error::InvalidInput(
            "explicit bounded current fiscal-year label required",
        ))
    } else {
        Ok(())
    }
}
impl ApiClient {
    pub async fn budget(&self, company: &str, year: &str) -> Result<Value, Error> {
        fiscal_selector(year, 128)?;
        self.get(&company_path(company, "budget")?, &[("year", year)])
            .await
    }
    pub async fn budget_vs_actual(&self, company: &str, year: &str) -> Result<Value, Error> {
        fiscal_selector(year, 128)?;
        self.get(
            &company_path(company, "budget-vs-actual")?,
            &[("year", year)],
        )
        .await
    }
    pub async fn budget_dimension_actuals(
        &self,
        company: &str,
        year: &str,
        dimension: Option<&str>,
        member: Option<&str>,
    ) -> Result<Value, Error> {
        fiscal_selector(year, 128)?;
        if member.is_some() && dimension.is_none() {
            return Err(Error::InvalidInput("member requires explicit dimension"));
        }
        for s in [dimension, member].into_iter().flatten() {
            if s.is_empty() || s.len() > 64 || s.trim() != s || s.chars().any(char::is_control) {
                return Err(Error::InvalidInput(
                    "exact bounded dimension/member selector required",
                ));
            }
        }
        let mut q = vec![("year", year)];
        if let Some(d) = dimension {
            q.push(("dimension", d));
        }
        if let Some(m) = member {
            q.push(("member", m));
        }
        self.get(&company_path(company, "budget-dimension-actuals")?, &q)
            .await
    }
    /// Append one exact account/month revision. Outer confirmation only: native body has no confirm/key.
    /// No deduplication promise or automatic retry; unknown outcomes require canonical revision reads.
    pub async fn set_budget(
        &self,
        company: &str,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if !confirmed {
            return Err(Error::InvalidInput(
                "explicit budget revision confirmation required",
            ));
        }
        if bytes.len() > MAX_BUDGET_BODY {
            return Err(Error::InvalidInput("budget exceeds native16KiB body"));
        }
        body(&bytes, &budget_body_schema())?;
        self.request(
            Method::POST,
            &company_path(company, "budget")?,
            &[],
            Some(bytes),
        )
        .await
    }
}
