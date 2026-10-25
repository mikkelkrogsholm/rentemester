//! Only the operator publishes output; server supplies evidence and HTML.
use super::*;
pub(super) async fn execute(api: &ApiClient, company: &str, f: &Flags<'_>) -> Output {
    let mut input = json!({"commitSha":short_sha()});
    for (flag, key) in [
        ("--as-of", "asOf"),
        ("--as-of-instant", "generatedAt"),
        ("--actor", "expectedActor"),
    ] {
        if let Some(v) = f.get(flag) {
            input[key] = json!(v);
        }
    }
    let mut v = match api.original716_compliance(company, &input).await {
        Ok(v) => v,
        Err(e) => return fail(e, f, "compliance"),
    };
    if v["ok"] != true {
        return result(v, f, "compliance");
    }
    let Some(html) = v["html"].as_str() else {
        return fail("Malformed report HTML", f, "compliance");
    };
    let hash = format!(
        "sha256:{}",
        rentemester_engine::compliance_report::fingerprint(html)
    );
    if v["fingerprint"] != hash || v["bytes"].as_u64() != Some(html.encode_utf16().count() as u64) {
        return fail("Report HTML fingerprint/length mismatch", f, "compliance");
    }
    if let Err(e) = private_output(Path::new(f["--out"]), html.as_bytes()) {
        return fail(e, f, "compliance");
    }
    v.as_object_mut().unwrap().remove("html");
    v["out"] = json!(f["--out"]);
    result(v, f, "compliance")
}
fn short_sha() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--short=7", "HEAD"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()
        .filter(|r| r.status.success())
        .and_then(|r| String::from_utf8(r.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| s.len() >= 7 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        .map(|s| s[..7].to_string())
        .unwrap_or_else(|| "unknown".into())
}
/// Private export publication follows the existing private-guide pattern: new 0600 file,
/// private directory and no symlink ancestors. Never truncate, follow or replace a target.
pub(super) fn private_output(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let absolute = if parent.is_absolute() {
        parent.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(parent)
    };
    let mut seen = std::path::PathBuf::new();
    for component in absolute.components() {
        seen.push(component);
        match std::fs::symlink_metadata(&seen) {
            Ok(m) if m.file_type().is_symlink() => {
                return Err("Private output ancestor must not be a symlink".into());
            }
            Ok(m) if !m.is_dir() => return Err("Output ancestor is not a directory".into()),
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let mut b = std::fs::DirBuilder::new();
                #[cfg(unix)]
                {
                    use std::os::unix::fs::DirBuilderExt;
                    b.mode(0o700);
                }
                b.create(&seen).map_err(|e| e.to_string())?;
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let m = std::fs::metadata(&absolute).map_err(|e| e.to_string())?;
        if m.mode() & 0o777 != 0o700 {
            return Err("Private export output directory must be 0700".into());
        }
    }
    let target = absolute.join(path.file_name().ok_or("Output requires filename")?);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&target)
        .map_err(|_| "Private output refused: existing or unsafe target".to_string())?;
    if file.write_all(bytes).and_then(|_| file.sync_all()).is_err() {
        drop(file);
        let _ = std::fs::remove_file(target);
        return Err("Private output write failed".into());
    }
    Ok(())
}
