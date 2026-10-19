//! Immutable document transport. Server owns decoding, MIME/hash, evidence, retention and authority.
use crate::{ApiClient, Error, company_path, domain::object};
use reqwest::Method;
use serde_json::{Value, json};
pub const MAX_DOCUMENT_INGEST_BODY: usize = 35 * 1024 * 1024;
pub const MAX_DOCUMENT_ORIGINAL: usize = 25 * 1024 * 1024;
pub fn document_ingest_schema() -> Value {
    let text = json!({"type":["string","null"]});
    let number = json!({"type":["number","null"]});
    let mut party = object(
        json!({"name":text,"address":text,"vatOrCvr":text,"countryCode":text,"identifierKind":text}),
        &[],
    );
    party["type"] = json!(["object", "null"]);
    let vat = object(
        json!({"classification":{"type":"string"},"netAmount":{"type":"number"},"vatAmount":number}),
        &["classification", "netAmount"],
    );
    let metadata = object(
        json!({"source":{"type":"string","minLength":1,"maxLength":1024},"documentType":text,"issueDate":text,"invoiceNo":text,"deliveryDescription":text,"amountIncVat":number,"currency":text,"sender":party,"recipient":party,"vatAmount":number,"purchaseVatLines":{"type":["array","null"],"maxItems":1000,"items":vat},"reverseChargeWordingConfirmed":{"type":["boolean","null"]},"danishSimplifiedPurchaseInvoice":{"type":["boolean","null"]},"paymentDetails":text,"exemptionCode":text,"sourceBankTransactionId":{"type":["integer","null"],"minimum":1,"maximum":9007199254740991u64},"accountingRationale":text}),
        &["source"],
    );
    object(
        json!({"confirm":{"const":true},"fileName":{"type":"string","minLength":1,"maxLength":255,"description":"Safe original filename only, no local filesystem path; server checks extension and byte signature"},"fileBase64":{"type":"string","minLength":1,"maxLength":34952536,"description":"Exact caller-supplied standard base64; server decoded file limit25MiB"},"metadata":metadata,"force":{"const":false},"vendorId":{"type":"null"}}),
        &["confirm", "fileName", "fileBase64", "metadata"],
    )
}
impl ApiClient {
    /// Exact native JSON once; no inferred metadata, keys, local file access or automatic retry.
    pub async fn ingest_document(
        &self,
        company: &str,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if !confirmed {
            return Err(Error::InvalidInput(
                "explicit document ingestion confirmation required",
            ));
        }
        if bytes.len() > MAX_DOCUMENT_INGEST_BODY {
            return Err(Error::InvalidInput(
                "document ingestion exceeds native35MiB JSON bound",
            ));
        }
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|_| Error::InvalidInput("expected native document JSON object"))?;
        let allowed = [
            "confirm",
            "fileName",
            "fileBase64",
            "metadata",
            "force",
            "vendorId",
        ];
        if value
            .as_object()
            .is_none_or(|o| o.keys().any(|k| !allowed.contains(&k.as_str())))
            || value["confirm"] != true
            || value["fileName"]
                .as_str()
                .is_none_or(|s| s.is_empty() || s.len() > 255)
            || value["fileBase64"]
                .as_str()
                .is_none_or(|s| s.is_empty() || s.len() > 34_952_536)
            || !value["metadata"].is_object()
            || value.get("force").is_some_and(|v| v != false)
            || value.get("vendorId").is_some_and(|v| !v.is_null())
        {
            return Err(Error::InvalidInput(
                "expected native confirmed document envelope; forced/vendor-linked uploads unavailable",
            ));
        }
        // Metadata whitelist, source validation and all factual policy remain server-owned.
        self.request(
            Method::POST,
            &company_path(company, "documents/ingest")?,
            &[],
            Some(bytes),
        )
        .await
    }
    /// Authorized recorded MIME and exact source bytes; no file writes, rendering or metadata inference.
    pub async fn document_original(&self, company: &str, id: i64) -> Result<Value, Error> {
        if !(1..=9_007_199_254_740_991).contains(&id) {
            return Err(Error::InvalidInput("positive exact document id required"));
        }
        let mut response = self
            .send(
                Method::GET,
                &company_path(company, &format!("documents/{id}/file"))?,
                &[],
                None,
                self.cookie.as_ref().map(|c| c.as_str()),
            )
            .await?;
        if !response.status().is_success() {
            return Err(Error::Refused {
                status: response.status().as_u16(),
            });
        }
        let mime = response
            .headers()
            .get("content-type")
            .and_then(|h| h.to_str().ok())
            .ok_or(Error::Protocol)?
            .to_owned();
        if ![
            "application/pdf",
            "image/png",
            "image/jpeg",
            "text/plain",
            "application/json",
            "application/xml",
        ]
        .contains(&mime.split(';').next().unwrap_or(""))
            || response
                .content_length()
                .is_some_and(|n| n > MAX_DOCUMENT_ORIGINAL as u64)
        {
            return Err(Error::Protocol);
        }
        let mut bytes = vec![];
        while let Some(chunk) = response.chunk().await.map_err(|_| Error::Transport)? {
            if bytes.len().saturating_add(chunk.len()) > MAX_DOCUMENT_ORIGINAL {
                return Err(Error::Protocol);
            }
            bytes.extend_from_slice(&chunk);
        }
        if bytes.is_empty() {
            return Err(Error::Protocol);
        }
        Ok(json!({"ok":true,"original":{"contentType":mime,"bytes":bytes}}))
    }
}
