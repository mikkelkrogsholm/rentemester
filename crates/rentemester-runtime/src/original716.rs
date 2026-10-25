//! Private operator input, never an HTTP authority producer.
use std::path::Path;
use zeroize::Zeroizing;
const PASSWORD_ERROR: &str = "password file must be a regular 0600 file up to 4 KiB containing one logical line";
/// Source password-file semantics: exact 0600, bounded regular no-follow file;
/// only a final CRLF is removed. A final LF alone remains invalid. Neither the
/// path, contents nor underlying OS error is ever included in the refusal.
#[cfg(unix)]
pub fn read_bootstrap_password(path: &Path) -> Result<Zeroizing<String>, &'static str> {
    use rustix::fs::{open, fstat, FileType, Mode, OFlags};
    use std::io::Read;
    let fd = open(path, OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC, Mode::empty()).map_err(|_| PASSWORD_ERROR)?;
    let stat = fstat(&fd).map_err(|_| PASSWORD_ERROR)?;
    if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile
        || stat.st_mode & 0o777 != 0o600 || stat.st_size < 0 || stat.st_size > 4096 {
        return Err(PASSWORD_ERROR);
    }
    let mut bytes = Zeroizing::new(Vec::new());
    std::fs::File::from(fd).take(4097).read_to_end(&mut bytes).map_err(|_| PASSWORD_ERROR)?;
    if bytes.len() > 4096 { return Err(PASSWORD_ERROR); }
    let raw = std::str::from_utf8(&bytes).map_err(|_| PASSWORD_ERROR)?;
    let value = raw.strip_suffix("\r\n").unwrap_or(raw);
    if value.is_empty() || value.contains(['\r', '\n']) { return Err(PASSWORD_ERROR); }
    Ok(Zeroizing::new(value.to_owned()))
}
#[cfg(not(unix))]
pub fn read_bootstrap_password(_: &Path) -> Result<Zeroizing<String>, &'static str> { Err(PASSWORD_ERROR) }

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn source_logical_line_modes_and_symlink_refusal() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("private");
        std::fs::write(&path, b"synthetic-secret\r\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(read_bootstrap_password(&path).unwrap().as_str(), "synthetic-secret");
        std::fs::write(&path, b"synthetic-secret\n").unwrap();
        assert_eq!(read_bootstrap_password(&path).unwrap_err(), PASSWORD_ERROR);
        std::fs::write(&path, b"synthetic-secret").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(read_bootstrap_password(&path).unwrap_err(), PASSWORD_ERROR);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let alias = root.path().join("alias");
        std::os::unix::fs::symlink(&path, &alias).unwrap();
        assert_eq!(read_bootstrap_password(&alias).unwrap_err(), PASSWORD_ERROR);
    }
}

pub mod bootstrap;
pub mod custody;
pub mod report;

pub mod transfer;
