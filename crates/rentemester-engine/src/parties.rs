//! Canonical party taxonomy and explicit typed identity. No accounting role/default inference.
use crate::{
    Error,
    company_knowledge::{text, trim},
    document,
};
use serde::{Deserialize, Serialize};
pub const KINDS: &[&str] = &[
    "person",
    "organization",
    "public_authority",
    "financial_institution",
];
pub const ROLES: &[&str] = &[
    "issuer",
    "supplier",
    "customer",
    "recipient",
    "payer",
    "payee",
    "processor",
    "acquirer",
    "related_company",
    "establishment",
    "location",
    "payment_descriptor",
    "vendor",
    "owner",
    "adviser",
    "employee",
    "authority",
    "bank",
];
pub fn refused(message: impl Into<String>) -> Error {
    Error::new("party_refused", message)
}
#[derive(Deserialize, Clone)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Identifier {
    pub country: String,
    pub identifier: Option<String>,
    pub identifier_kind: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedIdentifier {
    pub country: String,
    pub identifier: String,
    pub identifier_kind: String,
}
pub fn identity(i: &Identifier) -> Result<ResolvedIdentifier, Error> {
    let country = trim(&i.country).to_uppercase();
    let value = i
        .identifier
        .as_deref()
        .unwrap_or("")
        .chars()
        .filter(|c| !(*c != '\u{0085}' && (c.is_whitespace() || *c == '\u{feff}')))
        .collect::<String>()
        .to_uppercase();
    let result = document::supplier(&document::Party {
        country_code: Some(country),
        vat_or_cvr: Some(value),
        identifier_kind: Some(i.identifier_kind.clone()),
        ..Default::default()
    })
    .map_err(|_| refused("explicit typed identifier requires human resolution"))?;
    Ok(ResolvedIdentifier {
        country: result.country,
        identifier: result
            .identifier
            .filter(|s| !s.is_empty())
            .ok_or_else(|| refused("party identifiers must not be empty"))?,
        identifier_kind: result.kind,
    })
}
pub fn bounded(s: &str, max: usize) -> Result<String, Error> {
    text(s, max).map_err(|_| refused("bounded nonblank text required"))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn typed_identifiers_do_not_infer_vat_country() {
        let i = |country: &str, kind: &str, id: &str| Identifier {
            country: country.into(),
            identifier_kind: kind.into(),
            identifier: Some(id.into()),
        };
        assert_eq!(
            identity(&i(" dk ", "dk_cvr", "12 34 56 78"))
                .unwrap()
                .identifier,
            "DK12345678"
        );
        assert!(identity(&i("US", "eu_vat", "US1234")).is_err());
        assert!(identity(&i("DE", "non_eu", "DE1234")).is_err());
        assert!(identity(&i("US", "non_eu", "")).is_err());
        assert_eq!(ROLES.len(), 18);
    }
}
