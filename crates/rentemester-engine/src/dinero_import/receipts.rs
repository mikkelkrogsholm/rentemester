//! Pure selection of Dinero supporting originals. No document IDs, deduplication,
//! file publication or posting authority. New-byte MIME validation belongs to the
//! existing runtime document admission, after storage checks content-hash reuse.
use super::{ParsedImport, SourceFile, refused};
use crate::Error;
use serde::Serialize;
use sha2::{Digest, Sha256};
#[cfg(test)]
mod tests;
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptPlan {
    pub year: Option<u16>,
    pub receipts: Vec<Receipt>,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Receipt {
    pub source_path: String,
    pub original_filename: String,
    pub sha256: String,
    pub size_bytes: u64,
    /// Extension-derived expectation, not a claim that magic bytes were checked.
    pub mime_type: String,
    pub document_type: String,
    pub voucher_ref: Option<String>,
    pub unbooked: bool,
}
fn source_year(path: &str) -> Option<u16> {
    let (first, _) = path.split_once('/')?;
    (first.len() == 4 && first.bytes().all(|b| b.is_ascii_digit()))
        .then(|| first.parse().ok())
        .flatten()
}
fn mime(filename: &str) -> Option<&'static str> {
    match filename.rsplit_once('.')?.1.to_ascii_lowercase().as_str() {
        "pdf" => Some("application/pdf"),
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        _ => None,
    }
}
/// Source regex: /-(?:Bilag|Faktura)-(\d+)\.[A-Za-z0-9]+$/i.
/// The arbitrary basename prefix need not contain a year. Leading zeroes in
/// voucher references are identities, not numbers to normalize.
fn voucher(filename: &str) -> Option<&str> {
    let (stem, extension) = filename.rsplit_once('.')?;
    if extension.is_empty() || !extension.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return None;
    }
    let (prefix, reference) = stem.rsplit_once('-')?;
    let prefix = prefix.to_ascii_lowercase();
    if !(prefix.ends_with("-bilag") || prefix.ends_with("-faktura"))
        || reference.is_empty()
        || !reference.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    Some(reference)
}
/// Select the frozen source's latest root-year booked receipts and root unbooked
/// receipts, retaining every distinct source path even for identical content.
/// Older-year originals and unrelated entries remain in the complete package
/// inventory; they are not silently presented as live-year document links.
pub fn plan(files: &[SourceFile], parsed: &ParsedImport) -> Result<ReceiptPlan, Error> {
    let year = files.iter().filter_map(|f| source_year(&f.path)).max();
    let prefixes = year.map(|y| [format!("{y}/bilag/"), format!("{y}/faktura/")]);
    let mut booked = Vec::new();
    let mut unbooked = Vec::new();
    for file in files {
        let lower = file.path.to_lowercase();
        if prefixes
            .as_ref()
            .is_some_and(|prefixes| prefixes.iter().any(|p| lower.starts_with(p)))
        {
            if year != Some(parsed.latest_year) {
                return Err(refused(
                    "selected receipt year differs from parsed live journal year; no cross-year voucher binding",
                ));
            }
            booked.push(file);
        } else if lower.starts_with("ikke-bogførte-bilag/") {
            unbooked.push(file);
        }
    }
    // Package inventory order is canonical path order. Booked documents precede
    // unbooked documents as in the source's two ingest phases.
    booked.sort_by(|a, b| a.path.cmp(&b.path));
    unbooked.sort_by(|a, b| a.path.cmp(&b.path));
    let mut receipts = Vec::new();
    for (files, loose) in [(&booked, false), (&unbooked, true)] {
        for file in files {
            let filename = file.path.rsplit('/').next().unwrap_or("");
            let Some(mime_type) = mime(filename) else {
                if loose {
                    continue;
                }
                return Err(refused(format!(
                    "booked receipt '{}' has unsupported extension",
                    file.path
                )));
            };
            let voucher_ref = if loose {
                None
            } else {
                let reference = voucher(filename).ok_or_else(|| {
                    refused(format!(
                        "booked receipt '{}' has no source voucher reference",
                        file.path
                    ))
                })?;
                if !parsed.vouchers.iter().any(|v| v.voucher_ref == reference) {
                    return Err(refused(format!(
                        "booked receipt '{}' references unknown source voucher {reference}",
                        file.path
                    )));
                }
                Some(reference.to_owned())
            };
            receipts.push(Receipt {
                source_path: file.path.clone(),
                original_filename: filename.to_owned(),
                sha256: format!("{:x}", Sha256::digest(&file.bytes)),
                size_bytes: file.bytes.len() as u64,
                mime_type: mime_type.into(),
                document_type: if file.path.to_lowercase().contains("/faktura/") {
                    "issued_invoice_pdf"
                } else {
                    "cash_register_receipt"
                }
                .into(),
                voucher_ref,
                unbooked: loose,
            });
        }
    }
    Ok(ReceiptPlan { year, receipts })
}
