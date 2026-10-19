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
