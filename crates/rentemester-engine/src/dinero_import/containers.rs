//! Bounded source-container decoding only. ZIP parsing, encoding, record checks
//! and CRC verification belong to the pinned ZIP library; no disk extraction.
use super::{SourceFile, refused};
use crate::Error;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    io::{Cursor, Read},
};
#[cfg(test)]
mod tests;
pub const MAX_CONTAINER_BYTES: usize = 25 * 1024 * 1024;
pub const MAX_FILE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_TOTAL_BYTES: usize = 18 * 1024 * 1024;
pub const MAX_FILES: usize = 4096;
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    Json,
    Zip,
}
#[derive(Clone, Debug)]
pub struct Container {
    pub kind: Kind,
    pub files: Vec<SourceFile>,
    pub currency: String,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Package {
    format: String,
    currency: String,
    files: Vec<Encoded>,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Encoded {
    path: String,
    content_base64: String,
}
const PACKAGE: &str = "rentemester-dinero-source/1";
fn path(path: &str) -> Result<(), Error> {
    let drive = path.as_bytes().get(1) == Some(&b':') && path.as_bytes()[0].is_ascii_alphabetic();
    if path.is_empty()
        || path.len() > 1024
        || path.starts_with('/')
        || drive
        || path.contains('\\')
        || path.contains("..")
        || path.chars().any(char::is_control)
        || path.split('/').any(|p| p.is_empty() || p == ".")
    {
        return Err(refused("unsafe source container path"));
    }
    Ok(())
}
/// Native input policy preserves exact paths, rejects aliases, and never strips
/// wrapper roots or silently drops files. Folder callers own filesystem reads.
pub fn validate_paths(files: &[SourceFile]) -> Result<(), Error> {
    if files.is_empty() || files.len() > MAX_FILES {
        return Err(refused("source file count must be between1and4096"));
    }
    let mut names = BTreeSet::new();
    let mut total = 0usize;
    for f in files {
        path(&f.path)?;
        if !names.insert(f.path.to_lowercase()) {
            return Err(refused("duplicate or case-colliding source paths"));
        }
        if f.bytes.len() > MAX_FILE_BYTES {
            return Err(refused("source file exceeds16MiB"));
        }
        total = total
            .checked_add(f.bytes.len())
            .ok_or_else(|| refused("source size overflow"))?;
        if total > MAX_TOTAL_BYTES {
            return Err(refused("total source files exceed18MiB"));
        }
    }
    Ok(())
}
/// Deterministic folder transport in the existing JSON contract. This hash is a
/// package hash, never a claim about raw ZIP bytes or an original directory file.
pub fn package(files: &[SourceFile]) -> Result<Vec<u8>, Error> {
    validate_paths(files)?;
    let mut files = files.iter().collect::<Vec<_>>();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let bytes = serde_json::to_vec(&Package {
        format: PACKAGE.into(),
        currency: "DKK".into(),
        files: files
            .into_iter()
            .map(|f| Encoded {
                path: f.path.clone(),
                content_base64: STANDARD.encode(&f.bytes),
            })
            .collect(),
    })
    .map_err(|_| refused("source package encoding failed"))?;
    if bytes.len() > MAX_CONTAINER_BYTES {
        return Err(refused("source package exceeds25MiB"));
    }
    Ok(bytes)
}
pub fn decode(bytes: &[u8], kind: Kind) -> Result<Container, Error> {
    if bytes.is_empty() || bytes.len() > MAX_CONTAINER_BYTES {
        return Err(refused("source container must be1byte to25MiB"));
    }
    let (mut files, currency) = match kind {
        Kind::Json => {
            let p: Package = serde_json::from_slice(bytes)
                .map_err(|_| refused("invalid native JSON source package"))?;
            if p.format != PACKAGE || p.files.is_empty() || p.files.len() > MAX_FILES {
                return Err(refused("unsupported source package or inventory size"));
            }
            let mut files = Vec::with_capacity(p.files.len());
            let mut total = 0usize;
            for f in p.files {
                if f.content_base64.len() > MAX_FILE_BYTES.div_ceil(3) * 4 {
                    return Err(refused("encoded source file exceeds16MiB"));
                }
                let decoded = STANDARD
                    .decode(f.content_base64.as_bytes())
                    .map_err(|_| refused("invalid source base64"))?;
                if STANDARD.encode(&decoded) != f.content_base64 {
                    return Err(refused("noncanonical source base64"));
                }
                total = total
                    .checked_add(decoded.len())
                    .ok_or_else(|| refused("source size overflow"))?;
                if decoded.len() > MAX_FILE_BYTES || total > MAX_TOTAL_BYTES {
                    return Err(refused("decoded source package exceeds file bounds"));
                }
                files.push(SourceFile {
                    path: f.path,
                    bytes: decoded,
                });
            }
            (files, p.currency)
        }
        Kind::Zip => (zip_files(bytes)?, "DKK".into()),
    };
    validate_paths(&files)?;
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(Container {
        kind,
        files,
        currency,
    })
}
fn zip_files(bytes: &[u8]) -> Result<Vec<SourceFile>, Error> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|_| refused("invalid or truncated ZIP container"))?;
    // This library has no pre-allocation central-directory entry limit. Its
    // metadata input is bounded by the raw25MiB admission above; decoded file
    // count and bytes are separately bounded before decompression.
    if archive.is_empty() {
        return Err(refused("empty ZIP inventory"));
    }
    archive
        .validate_header_consistency()
        .map_err(|_| refused("ZIP local/central records, duplicate names or ranges disagree"))?;
    let mut files = vec![];
    let mut seen = BTreeSet::new();
    let mut total = 0usize;
    for index in 0..archive.len() {
        let mut f = archive
            .by_index_with_options(
                index,
                zip::read::ZipReadOptions::new().enforce_header_consistency(true),
            )
            .map_err(|_| refused("encrypted or unsupported ZIP entry"))?;
        let name = f
            .name()
            .map_err(|_| refused("ZIP filename encoding is invalid"))?
            .into_owned();
        if f.encrypted()
            || !matches!(
                f.compression(),
                zip::CompressionMethod::Stored | zip::CompressionMethod::Deflated
            )
        {
            return Err(refused("encrypted or unsupported ZIP compression"));
        }
        if f.is_symlink() {
            return Err(refused("ZIP symlinks are unsupported"));
        }
        let mode = f.unix_mode().unwrap_or(0) & 0o170000;
        let directory = f.is_dir();
        let regular_mode = if directory { 0o040000 } else { 0o100000 };
        if mode != 0 && mode != regular_mode {
            return Err(refused("ZIP nonregular entry is unsupported"));
        }
        let normalized = if directory {
            name.strip_suffix('/')
                .ok_or_else(|| refused("invalid ZIP directory marker"))?
        } else {
            &name
        };
        path(normalized)?;
        if !seen.insert(normalized.to_lowercase()) {
            return Err(refused("duplicate or case-colliding ZIP entry paths"));
        }
        if directory {
            if f.size() != 0 {
                return Err(refused("ZIP directory marker contains data"));
            }
            let mut b = [0u8; 1];
            if f.read(&mut b)
                .map_err(|_| refused("ZIP directory CRC is invalid"))?
                != 0
            {
                return Err(refused("ZIP directory marker contains data"));
            }
            continue;
        }
        if files.len() >= MAX_FILES {
            return Err(refused("ZIP file count exceeds4096"));
        }
        let remaining = MAX_TOTAL_BYTES
            .checked_sub(total)
            .ok_or_else(|| refused("ZIP total size exceeds18MiB"))?;
        let limit = remaining.min(MAX_FILE_BYTES);
        if f.size() > limit as u64 {
            return Err(refused("ZIP decoded entry exceeds source byte bounds"));
        }
        let expected = f.size();
        let mut decoded = Vec::with_capacity(
            usize::try_from(expected).map_err(|_| refused("ZIP size overflow"))?,
        );
        (&mut f)
            .take(limit as u64 + 1)
            .read_to_end(&mut decoded)
            .map_err(|_| refused("ZIP entry CRC, compressed data or size is invalid"))?;
        if decoded.len() > limit || decoded.len() as u64 != expected {
            return Err(refused(
                "ZIP extracted size differs from declared inventory",
            ));
        }
        total += decoded.len();
        files.push(SourceFile {
            path: name,
            bytes: decoded,
        });
    }
    Ok(files)
}
