//! Explicit local operator archive/trust input. No source directory or implicit key lookup.
use std::{io::Read, path::Path};
pub fn verified(
    path: &Path,
    key: &Path,
) -> Result<rentemester_runtime::backup_artifact::VerifiedArtifact, Box<dyn std::error::Error>> {
    let trusted = rentemester_runtime::backup_keys::trusted_key(key)?;
    let mut bytes = Vec::new();
    let f = std::fs::File::open(path)?;
    if !f.metadata()?.is_file()
        || f.metadata()?.len() > rentemester_runtime::backup_artifact::MAX_WIRE_BYTES as u64
    {
        return Err("backup artifact bound/type refused".into());
    }
    f.take(rentemester_runtime::backup_artifact::MAX_WIRE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    Ok(rentemester_runtime::backup_artifact::verify(
        &bytes, &trusted,
    )?)
}
