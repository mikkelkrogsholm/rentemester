//! Canonical clean scanner evidence; never caller document metadata.
use crate::Error;
use unicode_normalization::UnicodeNormalization;
use unicode_properties::{GeneralCategory, UnicodeGeneralCategory};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanEvidence {
    pub sha256: String,
    pub scanner_id: String,
    pub scanner_version: Option<String>,
    pub evidence_ref: Option<String>,
}
fn refused() -> Error {
    Error::new(
        "document_scan_evidence_refused",
        "Canonical clean document scan evidence required",
    )
}
fn field(value: &str, limit: usize) -> Result<String, Error> {
    let normalized: String = crate::company_knowledge::trim(value).nfc().collect();
    if normalized.is_empty()
        || normalized.encode_utf16().count() > limit
        || normalized.chars().any(|c| {
            matches!(
                c.general_category(),
                GeneralCategory::Control | GeneralCategory::Format
            )
        })
    {
        return Err(refused());
    }
    Ok(normalized)
}
impl ScanEvidence {
    pub fn new(
        sha256: &str,
        scanner_id: &str,
        scanner_version: Option<&str>,
        evidence_ref: Option<&str>,
    ) -> Result<Self, Error> {
        if sha256.len() != 64
            || !sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(refused());
        }
        Ok(Self {
            sha256: sha256.into(),
            scanner_id: field(scanner_id, 160)?,
            scanner_version: scanner_version.map(|s| field(s, 160)).transpose()?,
            evidence_ref: evidence_ref.map(|s| field(s, 512)).transpose()?,
        })
    }
    pub fn validate(&self) -> Result<(), Error> {
        if *self
            != Self::new(
                &self.sha256,
                &self.scanner_id,
                self.scanner_version.as_deref(),
                self.evidence_ref.as_deref(),
            )?
        {
            return Err(refused());
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_unicode_bounds_and_supplied_optional_fields() {
        let hash = "a".repeat(64);
        let mut evidence =
            ScanEvidence::new(&hash, " \u{feff}e\u{301} ", Some(" 1 "), Some(" source ")).unwrap();
        assert_eq!(evidence.scanner_id, "é");
        evidence.validate().unwrap();
        evidence.scanner_id = "e\u{301}".into();
        assert!(evidence.validate().is_err());
        for invalid in [
            "",
            " \t ",
            "scanner\u{0000}",
            "scanner\u{200b}",
            "scanner\u{e0001}",
        ] {
            assert!(ScanEvidence::new(&hash, invalid, None, None).is_err());
        }
        assert!(ScanEvidence::new(&hash, &"😀".repeat(80), Some(""), None).is_err());
        assert!(ScanEvidence::new(&hash, &"😀".repeat(80), None, None).is_ok());
        assert!(ScanEvidence::new(&hash, &"😀".repeat(81), None, None).is_err());
        assert!(ScanEvidence::new(&hash.to_uppercase(), "scanner", None, None).is_err());
        assert!(ScanEvidence::new(&hash, "scanner", None, Some(&"😀".repeat(257))).is_err());
    }
}
