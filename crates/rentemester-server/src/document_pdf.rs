//! Explicit operator selection of the pinned native PDF worker bundle.
use serde::Deserialize;
use std::path::PathBuf;
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct Config {
    bundle_dir: PathBuf,
}
pub(crate) fn build(
    config: Option<&Config>,
) -> Result<Option<rentemester_runtime::document_pdf::Config>, String> {
    config
        .map(|c| {
            if !c.bundle_dir.is_absolute() {
                return Err("documentPdfWorker.bundleDir must be absolute".into());
            }
            rentemester_runtime::document_pdf::Config::new(c.bundle_dir.clone())
                .map_err(|_| "configured document PDF worker unavailable or invalid".into())
        })
        .transpose()
}
