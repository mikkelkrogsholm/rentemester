//! Explicit provider extraction and retained evidence; no inferred accounting or retries.
use crate::{ApiClient, Error, MAX_RESPONSE, company_path};
use reqwest::Method;
use serde_json::Value;

fn document(value: i64) -> Result<(), Error> {
    if (1..=9_007_199_254_740_991).contains(&value) {
        Ok(())
    } else {
        Err(Error::InvalidInput("positive exact document id required"))
    }
}
impl ApiClient {
    pub async fn extract_document_invoice(
        &self,
        company: &str,
        document_id: i64,
        confirmed: bool,
    ) -> Result<Value, Error> {
        document(document_id)?;
        if !confirmed {
            return Err(Error::InvalidInput(
                "explicit invoice extraction confirmation required",
            ));
        }
        self.extraction_request(
            Method::POST,
            &company_path(company, &format!("documents/{document_id}/extract-invoice"))?,
            Some(br#"{"confirm":true}"#.to_vec()),
        )
        .await
    }

    pub async fn document_invoice_extraction(
        &self,
        company: &str,
        document_id: i64,
    ) -> Result<Value, Error> {
        document(document_id)?;
        self.extraction_request(
            Method::GET,
            &company_path(
                company,
                &format!("documents/{document_id}/invoice-extraction"),
            )?,
            None,
        )
        .await
    }

    async fn extraction_request(
        &self,
        method: Method,
        path: &str,
        body: Option<Vec<u8>>,
    ) -> Result<Value, Error> {
        let mutation = body.is_some();
        let unknown = || {
            if mutation {
                Error::OutcomeUnknown
            } else {
                Error::Transport
            }
        };
        let mut response = self
            .send(
                method,
                path,
                &[],
                body,
                self.cookie.as_ref().map(|c| c.as_str()),
            )
            .await?;
        let status = response.status();
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| unknown())? {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE {
                return Err(unknown());
            }
            bytes.extend_from_slice(&chunk);
        }
        let value: Value = match serde_json::from_slice(&bytes) {
            Ok(value) => value,
            Err(_) if status.is_success() || status.is_server_error() || status.as_u16() == 408 => {
                return Err(unknown());
            }
            Err(_) => {
                return Err(Error::Refused {
                    status: status.as_u16(),
                });
            }
        };
        // Only the explicit disabled-provider response proves that no attempt started.
        // Other server failures may follow a committed started attempt and stay unknown.
        if status.as_u16() == 503
            && value["ok"] == false
            && value["code"] == "EXTRACTION_PROVIDER_UNAVAILABLE"
        {
            return Ok(value);
        }
        if status.is_server_error() || status.as_u16() == 408 {
            return Err(unknown());
        }
        if value["ok"] == false && value["code"].is_string() {
            return Ok(value);
        }
        if status.is_success() && value["ok"] == true && value.get("extraction").is_some() {
            return Ok(value);
        }
        if status.is_success() {
            Err(unknown())
        } else {
            Err(Error::Refused {
                status: status.as_u16(),
            })
        }
    }
}
