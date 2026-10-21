//! The explicit dashboard artifact command owns local file publication only.
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
fn io_error() -> (u8, Error) {
    (
        1,
        Error::new(
            "artifact_write_failed",
            "could not atomically write dashboard artifact",
        ),
    )
}
fn atomic_write(out: &Path, bytes: &[u8]) -> Result<(), (u8, Error)> {
    let parent = out
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = out.file_name().ok_or_else(io_error)?;
    fs::create_dir_all(parent).map_err(|_| io_error())?;
    let temporary = parent.join(format!(
        ".{}.{}.{}.tmp",
        name.to_string_lossy(),
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary).map_err(|_| io_error())?;
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, out)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
        return Err(io_error());
    }
    Ok(())
}
fn open_artifact(out: &Path) {
    let Ok(out) = fs::canonicalize(out) else {
        return;
    };
    #[cfg(target_os = "macos")]
    let mut command = Command::new("open");
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut c = Command::new("cmd");
        c.args(["/c", "start", ""]);
        c
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut command = Command::new("xdg-open");
    let _ = command
        .arg(&out)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}
pub fn run(
    flags: &BTreeMap<&str, &str>,
    http: bool,
    confirm: bool,
    preview: bool,
    open: bool,
) -> Result<Value, (u8, Error)> {
    let invalid = || {
        (
            2,
            Error::new(
                "invalid_input",
                "dashboard requires explicit server/cookie/company/out, optional as-of/open only",
            ),
        )
    };
    if confirm
        || preview
        || flags
            .keys()
            .any(|k| !["--server", "--cookie-file", "--company", "--as-of", "--out"].contains(k))
    {
        return Err(invalid());
    }
    let required = |k| flags.get(k).copied().ok_or_else(invalid);
    let server = required("--server")?;
    required("--cookie-file")?;
    let company = required("--company")?;
    let out = required("--out")?;
    let as_of = flags.get("--as-of").copied();
    if as_of.is_some_and(|s| !rentemester_engine::calendar::valid_iso_date(s)) {
        return Err(invalid());
    }
    let api = crate::remote::credential_client(server, http, flags, false)?;
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|_| (1, Error::new("transport", "could not start HTTP runtime")))?;
    let value = runtime
        .block_on(api.dashboard_export(company, as_of))
        .map_err(crate::remote::client_error)?;
    if value["ok"] != true {
        return Ok(json!({"ok":false,"data":value}));
    }
    let bytes: Vec<u8> = serde_json::from_value(value["bytes"].clone())
        .map_err(|_| (1, Error::new("protocol", "invalid artifact bytes")))?;
    atomic_write(Path::new(out), &bytes)?;
    if open {
        open_artifact(Path::new(out));
    }
    Ok(
        json!({"ok":true,"outPath":out,"asOfDate":value["asOfDate"],"generatedAt":value["generatedAt"],"bytes":bytes.len(),"renderMs":value["renderMs"],"commitSha":value["commitSha"],"ruleBundleVersion":value["ruleBundleVersion"],"opened":open}),
    )
}
