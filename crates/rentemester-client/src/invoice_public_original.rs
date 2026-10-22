//! Original public-invoice operations. All selection, rendering and delivery stay on the server.
use crate::{ApiClient, Error, MAX_RESPONSE, company_path};
use reqwest::Method;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OriginalPublicInvoiceKind {
    ExportPublic,
    ExportPublicOioubl,
    SubmitPublicPeppol,
    TransmitDigisense,
}
impl OriginalPublicInvoiceKind {
    pub const ALL: [Self; 4] = [
        Self::ExportPublic,
        Self::ExportPublicOioubl,
        Self::SubmitPublicPeppol,
        Self::TransmitDigisense,
    ];
    pub fn suffix(self) -> &'static str {
        match self {
            Self::ExportPublic => "export-public",
            Self::ExportPublicOioubl => "export-public-oioubl",
            Self::SubmitPublicPeppol => "submit-public-peppol",
            Self::TransmitDigisense => "transmit-digisense",
        }
    }
    pub fn is_export(self) -> bool {
        matches!(self, Self::ExportPublic | Self::ExportPublicOioubl)
    }
}
pub fn invoice_public_original_schema(kind: OriginalPublicInvoiceKind) -> Value {
    let text = json!({"type":"string","minLength":1,"maxLength":4096});
    let mut fields = json!({"documentId":{"type":"integer","minimum":1,"maximum":9007199254740991u64},"invoiceNumber":text});
    let mut required = vec![];
    if !kind.is_export() {
        fields["confirm"] = json!({"const":true});
        required.push("confirm");
    }
    if kind == OriginalPublicInvoiceKind::SubmitPublicPeppol {
        fields["accessPoint"] = crate::domain::object(
            json!({"accessPointId":text,"endpointUrl":text,"senderEndpointId":text}),
            &["accessPointId", "endpointUrl", "senderEndpointId"],
        );
        fields["acknowledgement"] = crate::domain::object(
            json!({"transmissionId":text,"acknowledgedAt":text}),
            &["transmissionId", "acknowledgedAt"],
        );
        required.push("accessPoint");
    }
    if kind == OriginalPublicInvoiceKind::TransmitDigisense {
        fields["companyKey"] = text;
    }
    crate::domain::object(fields, &required)
}
pub fn validate_invoice_public_original(
    kind: OriginalPublicInvoiceKind,
    body: &[u8],
) -> Result<(), Error> {
    crate::domain::body(body, &invoice_public_original_schema(kind))?;
    let value: Value = serde_json::from_slice(body)
        .map_err(|_| Error::InvalidInput("expected public invoice JSON"))?;
    if value.get("documentId").is_none() && value.get("invoiceNumber").is_none() {
        return Err(Error::InvalidInput("documentId or invoiceNumber required"));
    }
    for object in [&value, &value["accessPoint"], &value["acknowledgement"]] {
        if object.as_object().is_some_and(|o| {
            o.values()
                .any(|v| v.as_str().is_some_and(|s| s.trim().is_empty()))
        }) {
            return Err(Error::InvalidInput("nonblank identity fields required"));
        }
    }
    Ok(())
}
fn matches_hash(value: &Value, text: &str, hash: &str) -> bool {
    value[text]
        .as_str()
        .zip(value[hash].as_str())
        .is_some_and(|(text, hash)| format!("{:x}", Sha256::digest(text.as_bytes())) == hash)
}
/// Verify exact returned UTF-8 artifact bytes, including immediately before local writing.
pub fn public_invoice_artifact(
    kind: OriginalPublicInvoiceKind,
    value: &Value,
) -> Result<&str, Error> {
    let (text, hash) = if kind.is_export() {
        ("xml", "sha256")
    } else {
        ("envelope", "envelopeSha256")
    };
    if value["ok"] != true || !matches_hash(value, text, hash) {
        return Err(Error::Protocol);
    }
    value[text].as_str().ok_or(Error::Protocol)
}
impl ApiClient {
    /// One request only; ambiguous audited/delivery operations must never be replayed automatically.
    pub async fn invoice_public_original(
        &self,
        company: &str,
        kind: OriginalPublicInvoiceKind,
        body: Vec<u8>,
    ) -> Result<Value, Error> {
        validate_invoice_public_original(kind, &body)?;
        let preview = kind == OriginalPublicInvoiceKind::ExportPublic;
        let unknown = || {
            if preview {
                Error::Transport
            } else {
                Error::OutcomeUnknown
            }
        };
        let protocol = || {
            if preview {
                Error::Protocol
            } else {
                Error::OutcomeUnknown
            }
        };
        let path = company_path(company, &format!("invoices/original/{}", kind.suffix()))?;
        let mut response = self
            .send(
                Method::POST,
                &path,
                &[],
                Some(body),
                self.cookie.as_ref().map(|v| v.as_str()),
            )
            .await
            .map_err(|e| {
                if preview && e == Error::OutcomeUnknown {
                    Error::Transport
                } else {
                    e
                }
            })?;
        let status = response.status();
        if !preview && (status.is_server_error() || status.as_u16() == 408) {
            return Err(Error::OutcomeUnknown);
        }
        if !status.is_success() {
            return Err(Error::Refused {
                status: status.as_u16(),
            });
        }
        let mut bytes = vec![];
        while let Some(chunk) = response.chunk().await.map_err(|_| unknown())? {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE {
                return Err(unknown());
            }
            bytes.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| protocol())?;
        if !value["ok"].is_boolean()
            || !value["errors"]
                .as_array()
                .is_some_and(|a| a.iter().all(Value::is_string))
        {
            return Err(protocol());
        }
        if value["ok"] == false {
            if value["errors"].as_array().is_none_or(Vec::is_empty) {
                return Err(protocol());
            }
            return Ok(value);
        }
        if !value["errors"].as_array().is_some_and(Vec::is_empty)
            || !value["invoiceNumber"].is_string()
            || !value["appliedRules"]
                .as_array()
                .is_some_and(|a| a.iter().all(Value::is_string))
        {
            return Err(protocol());
        }
        public_invoice_artifact(kind, &value).map_err(|_| protocol())?;
        if !kind.is_export()
            && (!value["duplicate"].is_boolean()
                || !value["submissionReference"].is_string()
                || !value["idempotencyKey"].is_string()
                || !value["oioublSha256"].is_string()
                || !matches!(
                    value["status"].as_str(),
                    Some("prepared" | "acknowledged" | "uncertain" | "failed")
                ))
        {
            return Err(protocol());
        }
        Ok(value)
    }
}
