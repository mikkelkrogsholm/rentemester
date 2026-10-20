//! Canonical task reads and explicit review resolution; neither performs bookkeeping.
use crate::{
    ApiClient, Error, company_path,
    domain::{body, object},
};
use reqwest::Method;
use serde_json::{Value, json};

pub const MAX_EXCEPTION_BODY: usize = 16 * 1024;
pub fn exception_resolve_schema() -> Value {
    object(
        json!({"note":{"type":["string","null"],"maxLength":4096}}),
        &[],
    )
}
impl ApiClient {
    /// Return the server's exact task context. Reads never refresh or resolve tasks.
    pub async fn exceptions(
        &self,
        company: &str,
        status: Option<&str>,
        include_archived: bool,
    ) -> Result<Value, Error> {
        if status.is_some_and(|s| !matches!(s, "open" | "resolved" | "all")) {
            return Err(Error::InvalidInput(
                "exception status must be open, resolved or all",
            ));
        }
        let mut query = vec![(
            "includeArchived",
            if include_archived { "true" } else { "false" },
        )];
        if let Some(status) = status {
            query.push(("status", status));
        }
        self.get(&company_path(company, "exceptions")?, &query)
            .await
    }
    /// Review once with current CompanyReview authority. No automatic retry or financial effect.
    /// If the response is lost, inspect the canonical queue/audit before further action.
    pub async fn resolve_exception(
        &self,
        company: &str,
        id: i64,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if !confirmed {
            return Err(Error::InvalidInput(
                "explicit exception resolution confirmation required",
            ));
        }
        if id <= 0 {
            return Err(Error::InvalidInput("positive exception id required"));
        }
        if bytes.len() > MAX_EXCEPTION_BODY {
            return Err(Error::InvalidInput(
                "exception resolution exceeds native16KiB body",
            ));
        }
        body(&bytes, &exception_resolve_schema())?;
        self.request(
            Method::POST,
            &company_path(company, &format!("exceptions/{id}/resolve"))?,
            &[],
            Some(bytes),
        )
        .await
    }
}
