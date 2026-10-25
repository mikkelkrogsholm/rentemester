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
    let text = Zeroizing::new(std::fs::read_to_string(config_path)?);
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
