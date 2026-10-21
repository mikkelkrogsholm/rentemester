//! Local source snapshots only; all accounting is performed by the server.
use rentemester_engine::{
    Error,
    dinero_import::{SourceFile, containers},
};
use std::{
    fs::{self, File, Metadata},
    io::Read,
    path::{Path, PathBuf},
};
const RAW: u64 = 25 * 1024 * 1024;
const ENTRY: u64 = 16 * 1024 * 1024;
const TOTAL: usize = 18 * 1024 * 1024;
fn fail() -> Error {
    Error::new(
        "invalid_input",
        "Dinero source must be a stable regular ZIP or directory without symbolic links, within documented source bounds",
    )
}
fn same(a: &Metadata, b: &Metadata) -> bool {
    let common = a.is_file() == b.is_file()
        && a.is_dir() == b.is_dir()
        && a.len() == b.len()
        && a.modified()
            .ok()
            .zip(b.modified().ok())
            .is_some_and(|(a, b)| a == b);
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        common && a.dev() == b.dev() && a.ino() == b.ino() && a.mode() == b.mode()
    }
    #[cfg(not(unix))]
    {
        common && a.created().ok() == b.created().ok()
    }
}
fn stable_file(path: &Path, max: u64) -> Result<(Vec<u8>, Metadata), Error> {
    let before = fs::symlink_metadata(path).map_err(|_| fail())?;
    if !before.is_file() || before.file_type().is_symlink() || before.len() > max {
        return Err(fail());
    }
    let mut file = File::open(path).map_err(|_| fail())?;
    if !same(&before, &file.metadata().map_err(|_| fail())?) {
        return Err(fail());
    }
    let mut bytes = vec![];
    Read::by_ref(&mut file)
        .take(max + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| fail())?;
    let after = fs::symlink_metadata(path).map_err(|_| fail())?;
    if bytes.len() as u64 != before.len()
        || !same(&before, &after)
        || !same(&before, &file.metadata().map_err(|_| fail())?)
    {
        return Err(fail());
    }
    Ok((bytes, before))
}
fn walk(
    root: &Path,
    dir: &Path,
    files: &mut Vec<SourceFile>,
    snapshots: &mut Vec<(PathBuf, Metadata)>,
    total: &mut usize,
) -> Result<(), Error> {
    let before = fs::symlink_metadata(dir).map_err(|_| fail())?;
    if !before.is_dir() || before.file_type().is_symlink() {
        return Err(fail());
    }
    let mut children = fs::read_dir(dir)
        .map_err(|_| fail())?
        .map(|e| e.map(|e| e.path()).map_err(|_| fail()))
        .collect::<Result<Vec<_>, _>>()?;
    children.sort();
    for path in children {
        let relative = path.strip_prefix(root).map_err(|_| fail())?;
        let relative = relative.to_str().ok_or_else(fail)?;
        if relative.len() > 1024 {
            return Err(fail());
        }
        let meta = fs::symlink_metadata(&path).map_err(|_| fail())?;
        if meta.file_type().is_symlink() {
            return Err(fail());
        }
        if meta.is_dir() {
            walk(root, &path, files, snapshots, total)?;
        } else if meta.is_file() {
            if files.len() >= 4096 {
                return Err(fail());
            }
            let (bytes, metadata) = stable_file(&path, ENTRY)?;
            *total = total.checked_add(bytes.len()).ok_or_else(fail)?;
            if *total > TOTAL {
                return Err(fail());
            }
            let relative = relative.replace(std::path::MAIN_SEPARATOR, "/");
            files.push(SourceFile {
                path: relative,
                bytes,
            });
            snapshots.push((path, metadata));
        } else {
            return Err(fail());
        }
    }
    if !same(&before, &fs::symlink_metadata(dir).map_err(|_| fail())?) {
        return Err(fail());
    }
    snapshots.push((dir.to_owned(), before));
    Ok(())
}
pub(super) fn source(path: &Path) -> Result<(String, Vec<u8>), Error> {
    let metadata = fs::symlink_metadata(path).map_err(|_| fail())?;
    if metadata.file_type().is_symlink() {
        return Err(fail());
    }
    if metadata.is_file() {
        let name = path.file_name().and_then(|n| n.to_str()).ok_or_else(fail)?;
        if !name.to_ascii_lowercase().ends_with(".zip") {
            return Err(fail());
        }
        let (bytes, _) = stable_file(path, RAW)?;
        return Ok((name.to_owned(), bytes));
    }
    if !metadata.is_dir() {
        return Err(fail());
    }
    let mut files = vec![];
    let mut snapshots = vec![];
    let mut total = 0;
    walk(path, path, &mut files, &mut snapshots, &mut total)?;
    for (file, before) in snapshots {
        let after = fs::symlink_metadata(file).map_err(|_| fail())?;
        if after.file_type().is_symlink() || !same(&before, &after) {
            return Err(fail());
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let bytes = containers::package(&files)?;
    if bytes.len() as u64 > RAW {
        return Err(fail());
    }
    Ok(("dinero-directory-source.json".into(), bytes))
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "rm126-source-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn folder_snapshot_is_deterministic_and_preserves_unknown_unicode_binary_entries() {
        let temp = Temp::new();
        fs::create_dir(temp.0.join("2025")).unwrap();
        fs::create_dir(temp.0.join("Ikke-bogførte-bilag")).unwrap();
        fs::write(
            temp.0.join("2025/Kontoplan.csv"),
            b"\xef\xbb\xbfexact;CSV\r\n",
        )
        .unwrap();
        fs::write(
            temp.0.join("Ikke-bogførte-bilag/æ.pdf"),
            b"%PDF-1.4\0exact\xff",
        )
        .unwrap();
        fs::write(temp.0.join("unknown.bin"), [0, 255, 1, 2]).unwrap();
        let (name, bytes) = source(&temp.0).unwrap();
        assert_eq!(name, "dinero-directory-source.json");
        assert_eq!(bytes, source(&temp.0).unwrap().1);
        let decoded = containers::decode(&bytes, containers::Kind::Json).unwrap();
        assert_eq!(decoded.files.len(), 3);
        assert!(
            decoded
                .files
                .iter()
                .any(|f| f.path == "2025/Kontoplan.csv" && f.bytes == b"\xef\xbb\xbfexact;CSV\r\n")
        );
        assert!(decoded.files.iter().any(|f|f.path=="Ikke-bogførte-bilag/æ.pdf"&&f.bytes==b"%PDF-1.4\0exact\xff"));
        assert!(
            decoded
                .files
                .iter()
                .any(|f| f.path == "unknown.bin" && f.bytes == [0, 255, 1, 2])
        );
    }
    #[test]
    fn raw_zip_is_snapshotted_without_local_path_or_json_reinterpretation() {
        let temp = Temp::new();
        let path = temp.0.join("export.ZIP");
        let bytes = b"PK\x03\x04synthetic exact raw container";
        fs::write(&path, bytes).unwrap();
        assert_eq!(
            source(&path).unwrap(),
            ("export.ZIP".into(), bytes.to_vec())
        );
        let oversized = temp.0.join("oversized.zip");
        File::create(&oversized).unwrap().set_len(RAW + 1).unwrap();
        assert!(source(&oversized).is_err());
        fs::write(temp.0.join("source.json"), b"{}").unwrap();
        assert!(source(&temp.0.join("source.json")).is_err());
    }
    #[test]
    fn directory_refuses_case_collisions_and_oversized_entries_before_network() {
        let temp = Temp::new();
        fs::write(temp.0.join("A.csv"), b"A").unwrap();
        fs::write(temp.0.join("a.csv"), b"a").unwrap();
        // A case-insensitive filesystem may alias these files; only assert the
        // collision when both distinct entries can actually exist.
        if fs::read_dir(&temp.0).unwrap().count() == 2 {
            assert!(source(&temp.0).is_err());
        }
        let huge = Temp::new();
        File::create(huge.0.join("large.bin"))
            .unwrap()
            .set_len(ENTRY + 1)
            .unwrap();
        assert!(source(&huge.0).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn source_refuses_root_and_nested_symbolic_links() {
        use std::os::unix::fs::symlink;
        let temp = Temp::new();
        let outside = Temp::new();
        fs::write(outside.0.join("secret.zip"), b"PKexternal").unwrap();
        symlink(&outside.0, temp.0.join("linked-root")).unwrap();
        assert!(source(&temp.0.join("linked-root")).is_err());
        symlink(outside.0.join("secret.zip"), temp.0.join("nested.zip")).unwrap();
        assert!(source(&temp.0.join("nested.zip")).is_err());
        assert!(source(&temp.0).is_err());
    }
}
