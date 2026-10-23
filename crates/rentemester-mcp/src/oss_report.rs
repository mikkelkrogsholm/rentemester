//! The single existing OSS selector now uses the original source contract.
use serde_json::Value;
pub fn tools() -> Vec<Value> {
    vec![crate::vat_original::tool("vat_oss_report")]
}
