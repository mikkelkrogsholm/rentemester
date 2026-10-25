//! Typed source requests. Authority remains the existing CompanyAdmin recovery fence.
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Rotate {
    pub reason: String,
    pub at: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SignedBackup {
    pub confirm: bool,
    pub at: Option<String>,
    pub archive: Option<bool>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportPublicKey {}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Verify {
    pub backup_dir: String,
    pub public_key: Option<String>,
    pub verify_key: Option<String>,
}
