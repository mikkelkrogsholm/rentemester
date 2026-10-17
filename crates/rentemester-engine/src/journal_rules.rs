//! Compiled manual-policy version gate, not a generic YAML interpreter or source checksum.
use crate::Error;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
pub const BALANCED: &str = "DK-BOOKKEEPING-BALANCED-001";
pub const APPEND_ONLY: &str = "DK-BOOKKEEPING-APPEND-ONLY-001";
pub const DOCUMENT: &str = "DK-BOOKKEEPING-DOCUMENT-001";
pub const FX: &str = "DK-BOOKKEEPING-FX-001";
pub const PERIOD_LOCK: &str = "DK-BOOKKEEPING-PERIOD-LOCK-001";
pub const VAT_CODES: [&str; 13] = [
    "DK_PURCHASE_25",
    "DK_PURCHASE_EXEMPT",
    "DK_SALE_25",
    "DK_SALE_EXEMPT",
    "EU_SERVICE_REVERSE_CHARGE",
    "EU_GOODS_ACQUISITION",
    "NON_EU_SERVICE_REVERSE_CHARGE",
    "REPRESENTATION_SPECIAL",
    "REPRESENTATION_NON_DEDUCTIBLE_VAT",
    "DK_BAD_DEBT_25",
    "REVERSE_CHARGE_EXEMPT",
    "DOMESTIC_REVERSE_CHARGE_EXEMPT",
    "OSS_EU_CONSUMER",
];
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuleBundle {
    pub name: String,
    pub version: String,
    pub rule_ids: Vec<String>,
}
#[derive(Debug)]
pub struct ManualJournalRules {
    version: String,
}
impl ManualJournalRules {
    pub fn from_bundles(mut bundles: Vec<RuleBundle>) -> Result<Self, Error> {
        let invalid = || {
            Error::new(
                "invalid_rules",
                "invalid or incomplete manual rule metadata",
            )
        };
        let mut names = BTreeSet::new();
        let mut ids = BTreeSet::new();
        for b in &bundles {
            if b.name.is_empty()
                || !b
                    .name
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
                || !names.insert(b.name.as_str())
                || b.version.is_empty()
                || !b
                    .version
                    .bytes()
                    .all(|c| c.is_ascii_graphic() && c != b';' && c != b'=')
            {
                return Err(invalid());
            }
            for id in &b.rule_ids {
                if id.is_empty()
                    || !id
                        .bytes()
                        .all(|c| c.is_ascii_graphic() && c != b';' && c != b'=')
                    || !ids.insert(id.as_str())
                {
                    return Err(invalid());
                }
            }
        }
        for (name, version) in [
            ("bookkeeping", "dk-bookkeeping-v0.0.8"),
            ("vat", "dk-vat-v0.0.5"),
        ] {
            let b = bundles
                .iter()
                .find(|b| b.name == name)
                .ok_or_else(invalid)?;
            if b.version != version {
                return Err(Error::new(
                    "unsupported_rule_version",
                    "manual rule policy version is not implemented",
                ));
            }
        }
        let bookkeeping = bundles.iter().find(|b| b.name == "bookkeeping").unwrap();
        if [BALANCED, APPEND_ONLY, DOCUMENT, FX, PERIOD_LOCK]
            .iter()
            .any(|id| !bookkeeping.rule_ids.iter().any(|x| x == id))
        {
            return Err(invalid());
        }
        bundles.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(Self {
            version: bundles
                .iter()
                .map(|b| format!("{}={}", b.name, b.version))
                .collect::<Vec<_>>()
                .join(";"),
        })
    }
    pub fn version(&self) -> &str {
        &self.version
    }
    pub fn require_version(&self, expected: &str) -> Result<(), Error> {
        if expected != self.version {
            return Err(Error::new(
                "rule_version_mismatch",
                "expected rule version does not match supplied active bundles",
            ));
        }
        Ok(())
    }
}
