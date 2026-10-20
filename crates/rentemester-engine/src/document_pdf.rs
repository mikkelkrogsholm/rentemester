//! Bounded offline PDF evidence. Parser identity is supplied by the trusted runtime adapter.
use crate::Error;
use serde::{Deserialize, Serialize};
pub const LEGACY_CONTRACT: &str = "document-pdf-text-v1";
pub const NATIVE_CONTRACT: &str = "document-pdf-text-pdfium-v1";
pub const MAX_JSON: usize = 16 * 1024 * 1024;
pub fn tampered() -> Error {
    Error::new(
        "PDF_EVIDENCE_TAMPERED",
        "PDF evidence integrity verification failed",
    )
}
pub fn hex(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ParserIdentity {
    pub parser_id: String,
    pub parser_version: String,
    pub contract_version: String,
}
impl ParserIdentity {
    pub fn validate(&self) -> Result<(), Error> {
        if self.parser_id.is_empty()
            || self.parser_version.is_empty()
            || self.parser_id.chars().any(char::is_control)
            || self.parser_version.chars().any(char::is_control)
            || !matches!(
                self.contract_version.as_str(),
                LEGACY_CONTRACT | NATIVE_CONTRACT
            )
            || (self.contract_version == LEGACY_CONTRACT
                && (self.parser_id != "pdfjs-dist" || self.parser_version != "6.2.108"))
            || (self.contract_version == NATIVE_CONTRACT && self.parser_id != "pdfium")
        {
            return Err(tampered());
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Ok,
    NoTextLayer,
    MalformedPdf,
    EncryptedPdf,
    UnsupportedPdf,
    ResourceLimit,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    SourceInvalid,
    SourceUnavailable,
    Timeout,
    StdoutLimit,
    StderrLimit,
    Crashed,
    ProtocolViolation,
    NoTextLayer,
    MalformedPdf,
    EncryptedPdf,
    UnsupportedPdf,
    PageLimit,
    ItemLimit,
    TextLimit,
    TamperedResult,
    Internal,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LayoutItem {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub font: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_eol: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Page {
    pub page_number: usize,
    pub width: f64,
    pub height: f64,
    pub rotation: u16,
    pub text: String,
    pub layout: Vec<LayoutItem>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ParseResult {
    pub contract_version: String,
    pub input_sha256: String,
    pub status: Status,
    pub error_code: Option<ErrorCode>,
    pub pages: Vec<Page>,
}
impl ParseResult {
    pub fn validate(&self, source_hash: &str, identity: &ParserIdentity) -> Result<(), Error> {
        identity.validate()?;
        if !hex(source_hash)
            || self.input_sha256 != source_hash
            || self.contract_version != identity.contract_version
            || self.pages.len() > 200
        {
            return Err(tampered());
        }
        let consistent = matches!(
            (self.status, self.error_code),
            (Status::Ok, None)
                | (Status::NoTextLayer, Some(ErrorCode::NoTextLayer))
                | (Status::MalformedPdf, Some(ErrorCode::MalformedPdf))
                | (Status::EncryptedPdf, Some(ErrorCode::EncryptedPdf))
                | (Status::UnsupportedPdf, Some(ErrorCode::UnsupportedPdf))
                | (
                    Status::ResourceLimit,
                    Some(ErrorCode::PageLimit | ErrorCode::ItemLimit | ErrorCode::TextLimit)
                )
        );
        if !consistent {
            return Err(tampered());
        }
        let mut items = 0usize;
        let mut text = 0usize;
        for (index, page) in self.pages.iter().enumerate() {
            items = items.checked_add(page.layout.len()).ok_or_else(tampered)?;
            text = text
                .checked_add(page.text.encode_utf16().count())
                .ok_or_else(tampered)?;
            if page.page_number != index + 1
                || !page.width.is_finite()
                || !page.height.is_finite()
                || page.width <= 0.0
                || page.height <= 0.0
                || !matches!(page.rotation, 0 | 90 | 180 | 270)
                || page.layout.len() > 25_000
                || items > 200_000
                || text > 5 * 1024 * 1024
                || page.layout.iter().any(|i| {
                    ![i.x, i.y, i.width, i.height]
                        .into_iter()
                        .all(f64::is_finite)
                })
            {
                return Err(tampered());
            }
        }
        Ok(())
    }
    pub fn item_count(&self) -> usize {
        self.pages.iter().map(|p| p.layout.len()).sum()
    }
    pub fn text_length(&self) -> usize {
        self.pages
            .iter()
            .map(|p| p.text.encode_utf16().count())
            .sum()
    }
}
/// Refuses unsupported UTF-16 surrogates explicitly; never substitutes replacement characters.
pub fn decode(bytes: &[u8]) -> Result<ParseResult, Error> {
    if bytes.len() > MAX_JSON {
        return Err(tampered());
    }
    if unpaired_surrogate(bytes) {
        return Err(Error::new(
            "PDF_EVIDENCE_UNSUPPORTED_UNICODE",
            "PDF evidence contains unsupported unpaired UTF-16; retained bytes were not changed",
        ));
    }
    serde_json::from_slice(bytes).map_err(|_| tampered())
}
#[cfg(test)]
mod tests;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationalError {
    SourceInvalid,
    SourceUnavailable,
    Timeout,
    StdoutLimit,
    StderrLimit,
    Crashed,
    ProtocolViolation,
    TamperedResult,
    Internal,
    WorkerUnavailable,
    MemoryLimit,
}
impl OperationalError {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SourceInvalid => "source_invalid",
            Self::SourceUnavailable => "source_unavailable",
            Self::Timeout => "timeout",
            Self::StdoutLimit => "stdout_limit",
            Self::StderrLimit => "stderr_limit",
            Self::Crashed => "crashed",
            Self::ProtocolViolation => "protocol_violation",
            Self::TamperedResult => "tampered_result",
            Self::Internal => "internal",
            Self::WorkerUnavailable => "worker_unavailable",
            Self::MemoryLimit => "memory_limit",
        }
    }
}

// Inspect JSON escapes without replacing UTF-16 units that Rust strings cannot represent.
fn unpaired_surrogate(bytes: &[u8]) -> bool {
    fn unit(bytes: &[u8], i: usize) -> Option<u16> {
        if bytes.get(i..i + 2)? != b"\\u" {
            return None;
        }
        let digits = std::str::from_utf8(bytes.get(i + 2..i + 6)?).ok()?;
        u16::from_str_radix(digits, 16).ok()
    }
    let mut string = false;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            string = !string;
            i += 1;
            continue;
        }
        if string && bytes[i] == b'\\' {
            if let Some(first) = unit(bytes, i) {
                if (0xd800..=0xdbff).contains(&first) {
                    if !unit(bytes, i + 6).is_some_and(|next| (0xdc00..=0xdfff).contains(&next)) {
                        return true;
                    }
                    i += 12;
                    continue;
                }
                if (0xdc00..=0xdfff).contains(&first) {
                    return true;
                }
                i += 6;
                continue;
            }
            i += 2;
            continue;
        }
        i += 1;
    }
    false
}
