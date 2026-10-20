//! Shared credential-file policy for local HTTP adapters; errors never contain secrets or paths.
use crate::Error;
use std::{io::Read, path::Path};

/// Read one Cookie request-header value from a bounded private regular file.
/// Unix group/other permissions are refused. Windows callers must provision private ACLs.
pub fn read_cookie_file(path: &Path) -> Result<String, Error> {
    let invalid = Error::InvalidInput;
    // Inspect the opened descriptor, avoiding a metadata/open path-replacement race.
    let mut file = std::fs::File::open(path).map_err(|_| invalid("could not open cookie file"))?;
    let metadata = file
        .metadata()
        .map_err(|_| invalid("could not inspect cookie file"))?;
    if !metadata.is_file() || metadata.len() > 8192 {
        return Err(invalid(
            "cookie must be a regular file of at most 8192 bytes",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.mode() & 0o077 != 0 {
            return Err(invalid(
                "cookie file must have no group or other permissions (chmod 600)",
            ));
        }
    }
    let mut bytes = Vec::new();
    file.by_ref()
        .take(8193)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid("could not read cookie file"))?;
    if bytes.len() > 8192 {
        return Err(invalid("cookie file exceeds limit"));
    }
    let value = String::from_utf8(bytes).map_err(|_| invalid("cookie file must be UTF-8"))?;
    let value = value.trim_end_matches(['\r', '\n']);
    if value.is_empty() || value.chars().any(char::is_control) {
        return Err(invalid("invalid Cookie header"));
    }
    Ok(value.to_owned())
}

/// Read one exact service key from a bounded private regular file, allowing a final newline.
/// Symlinks and Unix hard-linked/group-readable files are refused. No key/path enters errors.
/// Windows callers must provision private ACLs, as for Cookie files.
pub fn read_service_key_file(path: &Path) -> Result<String, Error> {
    let invalid = || {
        Error::InvalidInput(
            "service key requires a private regular file (chmod 600), at most 4098 bytes",
        )
    };
    let before = std::fs::symlink_metadata(path).map_err(|_| invalid())?;
    if !before.is_file() || before.len() > 4098 {
        return Err(invalid());
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    // O_NOFOLLOW | O_NONBLOCK: a replacement symlink/FIFO must not disclose or block.
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(0x20000 | 0x800);
    }
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(0x100 | 0x4);
    }
    let mut file = options.open(path).map_err(|_| invalid())?;
    let opened = file.metadata().map_err(|_| invalid())?;
    if !opened.is_file() || opened.len() > 4098 {
        return Err(invalid());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if opened.mode() & 0o077 != 0
            || opened.nlink() != 1
            || before.dev() != opened.dev()
            || before.ino() != opened.ino()
        {
            return Err(invalid());
        }
    }
    let mut bytes = zeroize::Zeroizing::new(Vec::new());
    file.by_ref()
        .take(4099)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid())?;
    if bytes.len() > 4098 {
        return Err(invalid());
    }
    let value = std::str::from_utf8(&bytes).map_err(|_| invalid())?;
    let value = value
        .strip_suffix("\r\n")
        .or_else(|| value.strip_suffix('\n'))
        .unwrap_or(value);
    if value.is_empty()
        || value.len() > 4096
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(invalid());
    }
    Ok(value.to_owned())
}

#[cfg(all(test, unix))]
mod service_file_tests {
    use super::*;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt, symlink};
    #[test]
    fn service_file_refuses_public_symlink_hardlink_and_invalid_values() {
        let path = std::env::temp_dir().join(format!("rm-service-file-{}", std::process::id()));
        let link = path.with_extension("link");
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .unwrap();
        use std::io::Write;
        file.write_all(b"synthetic-private-key\r\n").unwrap();
        assert_eq!(
            read_service_key_file(&path).unwrap(),
            "synthetic-private-key"
        );
        symlink(&path, &link).unwrap();
        assert!(read_service_key_file(&link).is_err());
        std::fs::remove_file(&link).unwrap();
        std::fs::hard_link(&path, &link).unwrap();
        assert!(read_service_key_file(&path).is_err());
        std::fs::remove_file(&link).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let error = read_service_key_file(&path).unwrap_err();
        assert!(!format!("{error:?}").contains("synthetic-private-key"));
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        for value in ["", " leading", "trailing ", "two\nlines", "key\n\n"] {
            std::fs::write(&path, value).unwrap();
            assert!(read_service_key_file(&path).is_err());
        }
        std::fs::write(&path, vec![b'x'; 4099]).unwrap();
        assert!(read_service_key_file(&path).is_err());
        std::fs::remove_file(path).unwrap();
    }
}
