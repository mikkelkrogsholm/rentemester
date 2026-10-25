//! Typed private CLI entry point. Mount no HTTP route for schema changes.
//! Declare this as a child of backend so Config remains private to its authority boundary.
use super::{Config, original717};
use serde_json::{Value, json};
use std::path::Path;
use zeroize::Zeroizing;
/// Final wire calls this for --original716-schema migrate|repair-schema-views
/// with --config PRIVATE.json, optional --apply yes and --reason TEXT.
/// Config chooses the only namespace/company and current allowlisted operator.
pub fn execute(
    config_path: &Path,
    company: &str,
    repair: bool,
    apply: bool,
    reason: Option<&str>,
) -> Result<Value, Box<dyn std::error::Error>> {
    let text = private_config(config_path)?;
    let config: Config = serde_json::from_str(&text).map_err(|_| "invalid server configuration")?;
    if company != config.company_slug
        && Path::new(company) != config.root.join(&config.company_slug)
    {
        return Err("company differs from the configured schema target".into());
    }
    let operator = original717::operator(&config).ok_or("configured schema operator required")?;
    let mut owner = rentemester_host::Owner::open_with_profile(
        &config.root,
        config.broker_uid,
        &config.workspace,
        &config.company_slug,
        config.runtime_profile,
    )?;
    let attempt = owner.original716_schema(&operator, repair, apply, reason)?;
    // Preserve committed outcome rather than transforming close/observation trouble into a retry.
    let mut result = attempt
        .outcome
        .unwrap_or_else(|e| json!({"ok":false,"errors":[format!("{}: {}",e.code,e.message)]}));
    let close_failed = owner.try_close().is_err();
    if attempt.retired || close_failed {
        result["ownerRetired"] = json!(true);
    }
    Ok(result)
}

// The operator allowlist is authority: admit its file before reading any secrets.
fn private_config(path: &Path) -> Result<Zeroizing<String>, Box<dyn std::error::Error>> {
    use std::io::Read;
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    if !path.is_absolute() {
        return Err("schema config must be an absolute private file".into());
    }
    #[cfg(target_os = "macos")]
    let flags = 0x100 | 4; // O_NOFOLLOW | O_NONBLOCK
    #[cfg(not(target_os = "macos"))]
    let flags = 0x20000 | 0x800;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(flags)
        .open(path)?;
    let meta = file.metadata()?;
    let uid = rentemester_runtime::original716::bootstrap::private_process();
    const MAX: u64 = 1024 * 1024;
    if !meta.is_file()
        || meta.uid() != uid
        || meta.nlink() != 1
        || meta.mode() & 0o7777 != 0o600
        || meta.len() > MAX
    {
        return Err(
            "schema config requires a current-user, single-link, regular 0600 file within 1 MiB"
                .into(),
        );
    }
    let mut text = Zeroizing::new(String::new());
    file.take(MAX + 1).read_to_string(&mut text)?;
    if text.len() as u64 > MAX {
        return Err("schema config exceeds 1 MiB".into());
    }
    Ok(text)
}
