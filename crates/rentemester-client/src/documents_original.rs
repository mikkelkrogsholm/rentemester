//! Original document contracts. Local files belong to the calling process, never the server.
use crate::{
    ApiClient, Error, MAX_DOCUMENT_INGEST_BODY, MAX_DOCUMENT_ORIGINAL, MAX_RESPONSE, company_path,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use reqwest::Method;
use serde_json::{Value, json};
use std::{fs, io::Read, path::Path};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentsOriginalMutation {
    Ingest,
    Enrich(i64),
    CompanyContext(i64),
}
fn object(properties: Value, required: &[&str]) -> Value {
    crate::domain::object(properties, required)
}
pub fn documents_original_schema(kind: DocumentsOriginalMutation) -> Value {
    if matches!(kind, DocumentsOriginalMutation::CompanyContext(_)) {
        return object(
            json!({"sourceReference":{"type":"string","minLength":1,"maxLength":2000},"businessUseReason":{"type":"string","minLength":1,"maxLength":2000}}),
            &["sourceReference", "businessUseReason"],
        );
    }
    let text = json!({"type":"string"});
    let party = object(
        json!({"name":text,"address":text,"vatOrCvr":text,"countryCode":{"type":"string","minLength":2,"maxLength":2},"identifierKind":{"type":"string","enum":["dk_cvr","eu_vat","non_eu"]}}),
        &[],
    );
    let vat = object(
        json!({"classification":{"type":"string","enum":["dk_purchase_25","exempt"]},"netAmount":{"type":"number","minimum":0},"vatAmount":{"type":"number","minimum":0}}),
        &["classification", "netAmount"],
    );
    let metadata = object(
        json!({"source":text,"documentType":{"type":"string","enum":["purchase_sale","cash_register_receipt","internal_voucher"]},"issueDate":text,"invoiceNo":text,"deliveryDescription":text,"currency":text,"paymentDetails":text,"amountIncVat":{"type":"number"},"vatAmount":{"type":"number"},"sender":party,"recipient":party,"purchaseVatLines":{"type":"array","minItems":1,"items":vat},"reverseChargeWordingConfirmed":{"type":"boolean"},"danishSimplifiedPurchaseInvoice":{"type":"boolean"},"exemptionCode":{"type":["string","null"],"enum":["FOREIGN_PHYSICAL_ONLY",null]},"sourceBankTransactionId":{"type":"integer","minimum":1,"maximum":9007199254740991u64},"accountingRationale":{"type":"string","minLength":1,"maxLength":2000}}),
        &["source"],
    );
    let mut properties = json!({"metadata":metadata});
    if kind == DocumentsOriginalMutation::Ingest {
        properties["vendorId"] =
            json!({"type":"integer","minimum":1,"maximum":9007199254740991u64});
        properties["force"] = json!({"type":"boolean"});
    }
    object(properties, &["metadata"])
}
fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && !matches!(name, "." | "..")
        && !name
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':'))
}
/// Snapshot a bounded regular file; reject symlink components and special files before opening.
pub fn documents_original_file(path: &Path) -> Result<(String, Vec<u8>), Error> {
    let invalid = || {
        Error::InvalidInput("expected bounded regular local file with safe basename, no symlinks")
    };
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|s| safe_name(s))
        .ok_or_else(invalid)?
        .to_owned();
    let mut prefix = std::path::PathBuf::new();
    for component in path.components() {
        prefix.push(component);
        if fs::symlink_metadata(&prefix)
            .map_err(|_| invalid())?
            .file_type()
            .is_symlink()
        {
            return Err(invalid());
        }
    }
    let before = fs::symlink_metadata(path).map_err(|_| invalid())?;
    if !before.is_file() || before.len() == 0 || before.len() > MAX_DOCUMENT_ORIGINAL as u64 {
        return Err(invalid());
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        #[cfg(target_os = "macos")]
        options.custom_flags(0x100 | 4);
        #[cfg(not(target_os = "macos"))]
        options.custom_flags(0x20000 | 2048);
    }
    let mut file = options.open(path).map_err(|_| invalid())?;
    let opened = file.metadata().map_err(|_| invalid())?;
    if !same_file(&before, &opened) {
        return Err(invalid());
    }
    let mut bytes = Vec::new();
    file.by_ref()
        .take(MAX_DOCUMENT_ORIGINAL as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid())?;
    let after = file.metadata().map_err(|_| invalid())?;
    let current = fs::symlink_metadata(path).map_err(|_| invalid())?;
    if bytes.len() as u64 != opened.len()
        || !same_file(&opened, &after)
        || !same_file(&opened, &current)
    {
        return Err(invalid());
    }
    Ok((name, bytes))
}
fn same_file(a: &fs::Metadata, b: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        a.is_file()
            && b.is_file()
            && a.dev() == b.dev()
            && a.ino() == b.ino()
            && a.len() == b.len()
            && a.mtime() == b.mtime()
            && a.mtime_nsec() == b.mtime_nsec()
            && a.ctime() == b.ctime()
            && a.ctime_nsec() == b.ctime_nsec()
    }
    #[cfg(not(unix))]
    {
        a.is_file() && b.is_file() && a.len() == b.len() && a.modified().ok() == b.modified().ok()
    }
}
fn exact(v: &Value, fields: &[&str]) -> bool {
    v.as_object()
        .is_some_and(|o| o.len() == fields.len() && o.keys().all(|k| fields.contains(&k.as_str())))
}
fn id(v: &Value) -> bool {
    v.as_u64()
        .is_some_and(|n| (1..=9_007_199_254_740_991).contains(&n))
}
fn nullable_text(v: &Value) -> bool {
    v.is_null() || v.is_string()
}
fn errors(v: &Value) -> bool {
    v.as_array().is_some_and(|a| a.iter().all(Value::is_string))
}
const ROW_FIELDS: &[&str] = &[
    "id",
    "documentNo",
    "source",
    "originalFilename",
    "documentType",
    "invoiceDate",
    "amountIncVat",
    "currency",
    "status",
    "storedPath",
    "purchaseVatLines",
    "senderVatOrCvr",
    "supplierCountryCode",
    "supplierIdentifierKind",
    "supplierIdentityStatus",
    "sourceBankTransactionId",
    "accountingRationale",
    "preparedBy",
    "preparedByProgram",
];
fn row(v: &Value) -> bool {
    exact(v, ROW_FIELDS)
        && id(&v["id"])
        && v["storedPath"].is_null()
        && ROW_FIELDS
            .iter()
            .filter(|k| {
                !matches!(
                    **k,
                    "id" | "amountIncVat" | "purchaseVatLines" | "sourceBankTransactionId"
                )
            })
            .all(|k| nullable_text(&v[*k]))
        && (v["amountIncVat"].is_null() || v["amountIncVat"].is_number())
        && (v["sourceBankTransactionId"].is_null() || id(&v["sourceBankTransactionId"]))
        && (v["purchaseVatLines"].is_null()
            || v["purchaseVatLines"].as_array().is_some_and(|a| {
                a.iter().all(|line| {
                    let schema = documents_original_schema(DocumentsOriginalMutation::Enrich(1));
                    crate::domain::body(
                        &serde_json::to_vec(&json!({"source":"", "purchaseVatLines":[line]}))
                            .unwrap(),
                        &schema["properties"]["metadata"],
                    )
                    .is_ok()
                })
            }))
}
fn valid(v: &Value, kind: Option<DocumentsOriginalMutation>) -> bool {
    if v["ok"] == false {
        return v.as_object().is_some_and(|o| {
            o.keys()
                .all(|k| ["ok", "errors", "code", "appliedRules"].contains(&k.as_str()))
        }) && errors(&v["errors"])
            && v["errors"].as_array().is_some_and(|a| !a.is_empty())
            && v.get("code").is_none_or(Value::is_string)
            && v.get("appliedRules").is_none_or(errors);
    }
    if v["ok"] != true {
        return false;
    }
    match kind {
        None => {
            exact(v, &["ok", "documents", "errors"])
                && v["errors"] == json!([])
                && v["documents"].as_array().is_some_and(|a| {
                    a.iter().all(row)
                        && a.windows(2)
                            .all(|p| p[0]["id"].as_u64() > p[1]["id"].as_u64())
                })
        }
        Some(DocumentsOriginalMutation::Ingest) => {
            exact(
                v,
                &[
                    "ok",
                    "documentId",
                    "documentNo",
                    "sha256",
                    "storedPath",
                    "message",
                ],
            ) && id(&v["documentId"])
                && v["documentNo"].is_string()
                && v["sha256"]
                    .as_str()
                    .is_some_and(|s| s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit()))
                && v["storedPath"].is_null()
                && v["message"].as_str()
                    == Some(
                        format!(
                            "Bilag {} er indlæst.",
                            v["documentNo"].as_str().unwrap_or("")
                        )
                        .as_str(),
                    )
        }
        Some(DocumentsOriginalMutation::Enrich(n)) => {
            exact(v, &["ok", "documentId", "enriched"])
                && v["documentId"] == n
                && v["enriched"].is_boolean()
        }
        Some(DocumentsOriginalMutation::CompanyContext(n)) => {
            exact(v, &["ok", "documentId", "applied", "errors"])
                && v["documentId"] == n
                && v["applied"].is_boolean()
                && v["errors"] == json!([])
        }
    }
}
impl ApiClient {
    pub async fn documents_original_list(&self, company: &str) -> Result<Value, Error> {
        self.documents_original_request(company, "list", None, None)
            .await
    }
    pub async fn documents_original_mutation(
        &self,
        company: &str,
        kind: DocumentsOriginalMutation,
        body: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if !confirmed || body.len() > MAX_DOCUMENT_INGEST_BODY {
            return Err(Error::InvalidInput(
                "confirmed bounded document body required",
            ));
        }
        let v: Value = serde_json::from_slice(&body)
            .map_err(|_| Error::InvalidInput("invalid document JSON"))?;
        let mut input = v.clone();
        let o = input
            .as_object_mut()
            .ok_or(Error::InvalidInput("document body must be object"))?;
        if o.remove("confirm") != Some(json!(true)) {
            return Err(Error::InvalidInput("confirm true required"));
        }
        let suffix = match kind {
            DocumentsOriginalMutation::Ingest => {
                let name = o
                    .remove("fileName")
                    .ok_or(Error::InvalidInput("fileName required"))?;
                let encoded = o
                    .remove("fileBase64")
                    .ok_or(Error::InvalidInput("fileBase64 required"))?;
                if !name.as_str().is_some_and(safe_name) {
                    return Err(Error::InvalidInput("safe basename required"));
                }
                let encoded = encoded
                    .as_str()
                    .filter(|s| s.len() <= 34_952_536)
                    .ok_or(Error::InvalidInput("bounded base64 required"))?;
                let bytes = STANDARD
                    .decode(encoded)
                    .map_err(|_| Error::InvalidInput("invalid base64"))?;
                if bytes.is_empty() || bytes.len() > MAX_DOCUMENT_ORIGINAL {
                    return Err(Error::InvalidInput("original must be 1..25MiB"));
                }
                "ingest".to_owned()
            }
            DocumentsOriginalMutation::Enrich(n) | DocumentsOriginalMutation::CompanyContext(n) => {
                if !(1..=9_007_199_254_740_991).contains(&n) {
                    return Err(Error::InvalidInput("positive exact document id required"));
                }
                format!(
                    "{n}/{}",
                    if matches!(kind, DocumentsOriginalMutation::Enrich(_)) {
                        "enrich"
                    } else {
                        "company-context"
                    }
                )
            }
        };
        crate::domain::body(
            &serde_json::to_vec(&input).unwrap(),
            &documents_original_schema(kind),
        )?;
        if matches!(kind, DocumentsOriginalMutation::CompanyContext(_))
            && ["sourceReference", "businessUseReason"]
                .iter()
                .any(|k| input[*k].as_str().is_none_or(|s| s.trim().is_empty()))
        {
            return Err(Error::InvalidInput("nonblank company context required"));
        }
        self.documents_original_request(company, &suffix, Some(body), Some(kind))
            .await
    }
    async fn documents_original_request(
        &self,
        company: &str,
        suffix: &str,
        body: Option<Vec<u8>>,
        kind: Option<DocumentsOriginalMutation>,
    ) -> Result<Value, Error> {
        let write = body.is_some();
        let failure = || {
            if write {
                Error::OutcomeUnknown
            } else {
                Error::Protocol
            }
        };
        let mut response = self
            .send(
                if write { Method::POST } else { Method::GET },
                &company_path(company, &format!("documents/original/{suffix}"))?,
                &[],
                body,
                self.cookie.as_ref().map(|c| c.as_str()),
            )
            .await?;
        let status = response.status();
        if status.is_server_error() || status.as_u16() == 408 {
            return Err(if write {
                Error::OutcomeUnknown
            } else {
                Error::Transport
            });
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| failure())? {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE {
                return Err(failure());
            }
            bytes.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| failure())?;
        if valid(&value, kind) && (status.is_success() || value["ok"] == false) {
            Ok(value)
        } else if !status.is_success() {
            Err(Error::Refused {
                status: status.as_u16(),
            })
        } else {
            Err(failure())
        }
    }
}
