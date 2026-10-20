//! Explicit local PDF inspection; the server owns originals, parsing and evidence.
use crate::{ApiClient, Error, MAX_RESPONSE, company_path};
use reqwest::Method;
use serde_json::{Value, json};

fn id(value: i64) -> Result<(), Error> {
    if (1..=9_007_199_254_740_991).contains(&value) {
        Ok(())
    } else {
        Err(Error::InvalidInput("positive exact document id required"))
    }
}
impl ApiClient {
    pub async fn document_parse(
        &self,
        company: &str,
        document_id: i64,
        confirmed: bool,
    ) -> Result<Value, Error> {
        id(document_id)?;
        if !confirmed {
            return Err(Error::InvalidInput(
                "explicit PDF parsing confirmation required",
            ));
        }
        self.pdf_request(
            Method::POST,
            &company_path(company, &format!("documents/{document_id}/parse"))?,
            &[],
            Some(serde_json::to_vec(&json!({"confirm":true})).map_err(|_| Error::Protocol)?),
        )
        .await
    }
    pub async fn document_parse_status(
        &self,
        company: &str,
        document_id: i64,
    ) -> Result<Value, Error> {
        id(document_id)?;
        self.pdf_request(
            Method::GET,
            &company_path(company, &format!("documents/{document_id}/parse-status"))?,
            &[],
            None,
        )
        .await
    }
    pub async fn document_parsed_text(
        &self,
        company: &str,
        document_id: i64,
        offset: u64,
        limit: u8,
    ) -> Result<Value, Error> {
        id(document_id)?;
        if offset > 9_007_199_254_740_991 || !(1..=10).contains(&limit) {
            return Err(Error::InvalidInput(
                "exact nonnegative offset and limit1..10 required",
            ));
        }
        let offset = offset.to_string();
        let limit = limit.to_string();
        self.pdf_request(
            Method::GET,
            &company_path(company, &format!("documents/{document_id}/parsed-text"))?,
            &[("offset", &offset), ("limit", &limit)],
            None,
        )
        .await
    }
    pub async fn documents_parse_pending(
        &self,
        company: &str,
        cursor: u64,
        limit: u8,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if !confirmed || cursor > 9_007_199_254_740_991 || !(1..=100).contains(&limit) {
            return Err(Error::InvalidInput(
                "confirmation, exact nonnegative cursor and limit1..100 required",
            ));
        }
        self.pdf_request(
            Method::POST,
            &company_path(company, "documents/parse-pending")?,
            &[],
            Some(
                serde_json::to_vec(&json!({"confirm":true,"cursor":cursor,"limit":limit}))
                    .map_err(|_| Error::Protocol)?,
            ),
        )
        .await
    }
    // Preserve curated evidence/refusal codes for agent callers. A transport failure
    // after a write remains unknown, including an incomplete or oversized response.
    async fn pdf_request(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, &str)],
        body: Option<Vec<u8>>,
    ) -> Result<Value, Error> {
        let write = body.is_some();
        let failure = || {
            if write {
                Error::OutcomeUnknown
            } else {
                Error::Transport
            }
        };
        let mut response = self
            .send(
                method,
                path,
                query,
                body,
                self.cookie.as_ref().map(|c| c.as_str()),
            )
            .await?;
        let status = response.status();
        if status.is_server_error() || status.as_u16() == 408 {
            return Err(failure());
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| failure())? {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE {
                return Err(failure());
            }
            bytes.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| {
            if status.is_success() {
                failure()
            } else {
                Error::Refused {
                    status: status.as_u16(),
                }
            }
        })?;
        if value["ok"] == false
            && (value["code"].is_string() || status.is_success() && value["errors"].is_array())
        {
            return Ok(value);
        }
        if status.is_success() && value["ok"] == true {
            Ok(value)
        } else {
            Err(Error::Refused {
                status: status.as_u16(),
            })
        }
    }
}
