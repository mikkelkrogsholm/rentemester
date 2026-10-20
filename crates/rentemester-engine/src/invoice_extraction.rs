//! Cited invoice evidence validation. Extraction never changes voucher metadata or books an entry.
use crate::{Error, document};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
pub const MAX_PDF: usize = 10 * 1024 * 1024;
pub const MAX_RESPONSE: usize = 1024 * 1024;
pub const NATIVE_CODEC: &str = "rentemester-native-invoice-extraction/1";
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderIdentity {
    pub id: String,
    pub version: String,
}
impl ProviderIdentity {
    pub fn validate(&self) -> Result<(), Error> {
        if [self.id.as_str(), self.version.as_str()].iter().any(|s| {
            s.trim() != *s || s.is_empty() || s.len() > 200 || s.chars().any(char::is_control)
        }) || self.version.contains("#retry-")
        {
            return Err(Error::new(
                "EXTRACTION_INVALID_PROVIDER",
                "Explicit bounded provider identity required",
            ));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProviderOutput {
    pub fields: Vec<Value>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProviderFailure {
    Unavailable,
}
impl ProviderFailure {
    pub fn as_str(&self) -> &'static str {
        "EXTRACTION_PROVIDER_UNAVAILABLE"
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Context {
    pub supplied_metadata: SuppliedMetadata,
    pub selected_buyer: SelectedBuyer,
    pub confidence_threshold: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SuppliedMetadata {
    pub invoice_number: Option<String>,
    pub currency: Option<String>,
    pub gross_amount: Option<f64>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SelectedBuyer {
    pub name: Option<String>,
    pub country: Option<String>,
    pub legal_id: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceBox {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    pub key: String,
    pub value: Value,
    pub confidence: f64,
    pub page: i64,
    pub source_text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#box: Option<EvidenceBox>,
}
pub const KEYS: [&str; 15] = [
    "invoiceNumber",
    "supplierName",
    "supplierCountry",
    "supplierLegalId",
    "supplierLegalIdKind",
    "buyerName",
    "buyerCountry",
    "buyerLegalId",
    "buyerLegalIdKind",
    "invoiceDate",
    "currency",
    "netAmount",
    "vatAmount",
    "grossAmount",
    "reverseChargeWording",
];
fn nonempty(v: &Value, max: usize) -> bool {
    v.as_str()
        .is_some_and(|s| !s.trim().is_empty() && s.trim().encode_utf16().count() <= max)
}
pub fn fields(raw: &[Value]) -> Result<Vec<Field>, Vec<String>> {
    let mut errors = vec![];
    let mut seen = BTreeSet::new();
    let mut result = vec![];
    if raw.len() > 100 || serde_json::to_vec(raw).map_or(true, |v| v.len() > MAX_RESPONSE) {
        return Err(vec!["EXTRACTION_INVALID_RESPONSE".into()]);
    }
    for value in raw {
        let parsed = serde_json::from_value::<Field>(value.clone());
        let Ok(f) = parsed else {
            errors.push("EXTRACTION_INVALID_PROVENANCE".into());
            continue;
        };
        if !KEYS.contains(&f.key.as_str())
            || !f.confidence.is_finite()
            || !(0.0..=1.0).contains(&f.confidence)
            || f.page < 1
            || f.page > 9_007_199_254_740_991
            || !nonempty(&Value::String(f.source_text.clone()), 4000)
        {
            errors.push("EXTRACTION_INVALID_PROVENANCE".into());
        }
        if !seen.insert(f.key.clone()) {
            errors.push("EXTRACTION_DUPLICATE_FIELD".into());
        }
        if f.r#box.as_ref().is_some_and(|b| {
            [b.x, b.y, b.width, b.height].iter().any(|n| !n.is_finite())
                || b.width < 0.0
                || b.height < 0.0
        }) {
            errors.push("EXTRACTION_INVALID_BOX".into());
        }
        if ["supplierLegalId", "buyerLegalId"].contains(&f.key.as_str()) && !nonempty(&f.value, 64)
        {
            errors.push(
                "legal identifiers require cited source text and a non-empty exact value".into(),
            );
        }
        let invalid = match f.key.as_str() {
            "invoiceDate" => !f.value.as_str().is_some_and(|s| {
                s.len() == 10
                    && s.bytes().enumerate().all(|(i, c)| {
                        if i == 4 || i == 7 {
                            c == b'-'
                        } else {
                            c.is_ascii_digit()
                        }
                    })
            }),
            "netAmount" | "vatAmount" | "grossAmount" => {
                !f.value.as_f64().is_some_and(|v| v.is_finite() && v >= 0.0)
            }
            "currency" => {
                !nonempty(&f.value, 3)
                    || f.value
                        .as_str()
                        .is_some_and(|s| s.trim().encode_utf16().count() != 3)
            }
            _ => false,
        };
        if invalid {
            errors.push(format!("invalid {} value", f.key));
        }
        result.push(f);
    }
    errors.sort();
    errors.dedup();
    errors.truncate(20);
    if errors.is_empty() {
        Ok(result)
    } else {
        Err(errors)
    }
}
pub fn conflicts(fields: &[Field], context: &Context) -> Vec<String> {
    let get = |key: &str| fields.iter().find(|f| f.key == key);
    let mut errors = vec![];
    for key in [
        "invoiceNumber",
        "supplierName",
        "buyerName",
        "invoiceDate",
        "currency",
        "netAmount",
        "vatAmount",
        "grossAmount",
    ] {
        if get(key).is_none_or(|f| f.confidence < context.confidence_threshold) {
            errors.push(format!("{key} is missing or below material confidence"));
        }
    }
    if let (Some(n), Some(v), Some(g)) = (get("netAmount"), get("vatAmount"), get("grossAmount"))
        && let (Some(n), Some(v), Some(g)) = (n.value.as_f64(), v.value.as_f64(), g.value.as_f64())
        && (n + v - g).abs() > 0.01
    {
        errors.push("net amount plus VAT must equal gross amount".into());
    }
    for (key, value, message) in [
        (
            "invoiceNumber",
            context
                .supplied_metadata
                .invoice_number
                .clone()
                .map(Value::String),
            "invoice number",
        ),
        (
            "currency",
            context
                .supplied_metadata
                .currency
                .clone()
                .map(Value::String),
            "currency",
        ),
        ("grossAmount", None, "gross amount"),
    ] {
        if value
            .as_ref()
            .is_some_and(|v| !v.is_string() || v.as_str() != Some(""))
            && get(key).map(|f| &f.value) != value.as_ref()
        {
            errors.push(format!("{message} conflicts with supplied metadata"));
        }
    }
    if context
        .supplied_metadata
        .gross_amount
        .is_some_and(|v| get("grossAmount").and_then(|f| f.value.as_f64()) != Some(v))
    {
        errors.push("gross amount conflicts with supplied metadata".into());
    }
    for (key, value, label) in [
        ("buyerName", &context.selected_buyer.name, "name"),
        ("buyerCountry", &context.selected_buyer.country, "country"),
        ("buyerLegalId", &context.selected_buyer.legal_id, "legalId"),
    ] {
        if value.as_ref().is_some_and(|s| !s.is_empty())
            && get(key).and_then(|f| f.value.as_str()) != value.as_deref()
        {
            errors.push(format!("buyer {label} conflicts with selected company"));
        }
    }
    for side in ["supplier", "buyer"] {
        let name = get(&format!("{side}Name"));
        let country = get(&format!("{side}Country"));
        let id = get(&format!("{side}LegalId"));
        let kind = get(&format!("{side}LegalIdKind"));
        match (name, country, id, kind) {
            (Some(_), Some(country), Some(id), Some(kind)) => {
                let party = document::Party {
                    country_code: country.value.as_str().map(str::to_owned),
                    identifier_kind: kind.value.as_str().map(str::to_owned),
                    vat_or_cvr: id.value.as_str().map(str::to_owned),
                    ..Default::default()
                };
                if !id
                    .value
                    .as_str()
                    .is_some_and(|s| id.source_text.contains(s))
                    || document::supplier(&party).is_err()
                {
                    errors.push(format!(
                        "{side} legal identifier is not supported by cited typed identity evidence"
                    ));
                }
            }
            _ => errors.push(format!("{side} identity evidence is missing")),
        }
    }
    errors
}
pub fn tampered() -> Error {
    Error::new(
        "EXTRACTION_EVIDENCE_TAMPERED",
        "Invoice extraction evidence is inconsistent or uses an unsupported codec",
    )
}
/// Exact provider base/retry grammar; never adopt a prefix collision.
pub fn retry_number(stored: &str, base: &str) -> Option<u64> {
    if stored == base {
        return Some(0);
    }
    let n = stored.strip_prefix(base)?.strip_prefix("#retry-")?;
    if n.starts_with('0') || n.is_empty() {
        return None;
    }
    n.parse().ok().filter(|n| *n > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn source_field_limits_duplicates_citations_and_retry_identity() {
        let field = json!({"key":"supplierLegalId","value":"DK12345678","confidence":0.8,"page":1,"sourceText":"Invoice DK12345678","box":{"x":-1.0,"y":0.0,"width":1.0,"height":0.0}});
        assert!(fields(std::slice::from_ref(&field)).is_ok());
        assert!(
            fields(&[field.clone(), field.clone()])
                .unwrap_err()
                .contains(&"EXTRACTION_DUPLICATE_FIELD".to_owned())
        );
        let mut bad = field.clone();
        bad["box"]["width"] = json!(-0.1);
        assert!(fields(&[bad]).is_err());
        let mut unicode = field;
        unicode["sourceText"] = json!("😀".repeat(2001));
        assert!(fields(&[unicode]).is_err());
        assert_eq!(retry_number("1.1", "1"), None);
        assert_eq!(retry_number("1#retry-01", "1"), None);
        assert_eq!(retry_number("1#retry-2", "1"), Some(2));
        assert!(
            ProviderIdentity {
                id: "http-json-v1".into(),
                version: "1#retry-1".into()
            }
            .validate()
            .is_err()
        );
    }
}
