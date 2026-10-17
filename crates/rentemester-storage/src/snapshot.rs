use rentemester_engine::Error;
use rustix::fd::OwnedFd;
use rustix::fs::{Mode, OFlags, open, openat};
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path};
const MAX_CAPTURE_BYTES: u64 = 512 * 1024 * 1024;
#[derive(Debug, PartialEq, Eq)]
pub(super) struct SourceFile {
    pub suffix: &'static str,
    pub bytes: Vec<u8>,
    hash: Vec<u8>,
}
fn unavailable(e: impl std::fmt::Display) -> Error {
    Error::new("source_unavailable", e.to_string())
}
pub(super) fn capture(company: &Path) -> Result<Vec<SourceFile>, Error> {
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC | OFlags::NOFOLLOW;
    // Drop terminal separators/dot aliases: NOFOLLOW alone accepts linked/ on macOS.
    let normalized: std::path::PathBuf = company.components().collect();
    let root = open(&normalized, flags, Mode::empty()).map_err(unavailable)?;
    let data = openat(&root, "data", flags, Mode::empty()).map_err(unavailable)?;
    stable_capture(|| read_state(&data))
}
fn stable_capture(
    mut read: impl FnMut() -> Result<Vec<SourceFile>, Error>,
) -> Result<Vec<SourceFile>, Error> {
    for _ in 0..4 {
        let before = read()?;
        let after = read()?;
        if before
            .iter()
            .map(|f| (f.suffix, &f.hash))
            .eq(after.iter().map(|f| (f.suffix, &f.hash)))
        {
            return Ok(before);
        }
    }
    Err(Error::new(
        "source_changed",
        "SQLite source changed during all four capture attempts",
    ))
}
fn read_state(data: &OwnedFd) -> Result<Vec<SourceFile>, Error> {
    let mut result = Vec::new();
    let mut remaining = MAX_CAPTURE_BYTES;
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let name = format!("ledger.sqlite{suffix}");
        let fd = match openat(
            data,
            name.as_str(),
            OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK,
            Mode::empty(),
        ) {
            Ok(fd) => fd,
            Err(rustix::io::Errno::NOENT) if !suffix.is_empty() => continue,
            Err(e) => return Err(unavailable(e)),
        };
        let file = File::from(fd);
        let meta = file.metadata().map_err(unavailable)?;
        if !meta.is_file() || meta.len() > remaining {
            return Err(unavailable(
                "source must be regular files within the 512 MiB capture limit",
            ));
        }
        let mut bytes = Vec::new();
        file.take(remaining + 1)
            .read_to_end(&mut bytes)
            .map_err(unavailable)?;
        if bytes.len() as u64 > remaining {
            return Err(unavailable("source exceeds capture limit"));
        }
        remaining -= bytes.len() as u64;
        let hash = Sha256::digest(&bytes).to_vec();
        result.push(SourceFile {
            suffix,
            bytes,
            hash,
        });
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unstable_capture_is_bounded_and_can_recover() {
        let mut calls = 0u8;
        let error = stable_capture(|| {
            calls += 1;
            Ok(vec![SourceFile {
                suffix: "",
                bytes: vec![calls],
                hash: vec![calls],
            }])
        })
        .unwrap_err();
        assert_eq!(calls, 8);
        assert_eq!(error.code, "source_changed");
        let mut calls = 0u8;
        let captured = stable_capture(|| {
            calls += 1;
            let v = if calls < 3 { calls } else { 9 };
            Ok(vec![SourceFile {
                suffix: "",
                bytes: vec![v],
                hash: vec![v],
            }])
        })
        .unwrap();
        assert_eq!(calls, 4);
        assert_eq!(captured[0].bytes, vec![9]);
    }
}
