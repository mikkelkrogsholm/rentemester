//! One explicit transport join before every existing CLI dispatcher; no server lifecycle.
use rentemester_client::LocalRuntime;
use rentemester_engine::Error;
fn usage(message: &str) -> (u8, Error) {
    (2, Error::new("invalid_input", message))
}
pub fn prepare(args: Vec<String>) -> Result<Vec<String>, (u8, Error)> {
    let mut remaining = Vec::new();
    let mut profile = None;
    let mut cursor = 0;
    let mut normal = true;
    while cursor < args.len() {
        let arg = &args[cursor];
        if normal && arg == "--" {
            normal = false;
            remaining.push(arg.clone());
            cursor += 1;
            continue;
        }
        let attached = normal
            .then(|| arg.strip_prefix("--local-profile="))
            .flatten();
        if normal && (arg == "--local-profile" || attached.is_some()) {
            if profile.is_some() {
                return Err(usage("duplicate --local-profile"));
            }
            let value = if let Some(value) = attached {
                value.to_owned()
            } else {
                cursor += 1;
                args.get(cursor)
                    .filter(|v| !v.starts_with("--"))
                    .ok_or_else(|| {
                        usage("--local-profile requires an absolute private profile path")
                    })?
                    .clone()
            };
            if value.is_empty() {
                return Err(usage(
                    "--local-profile requires an absolute private profile path",
                ));
            }
            profile = Some(value);
            cursor += 1;
            continue;
        }
        remaining.push(arg.clone());
        cursor += 1;
    }
    let Some(profile) = profile else {
        return Ok(remaining);
    };
    let flags: Vec<_> = remaining
        .iter()
        .take_while(|v| v.as_str() != "--")
        .collect();
    if flags.iter().any(|v| {
        v.as_str() == "--server"
            || v.starts_with("--server=")
            || v.as_str() == "--allow-loopback-http"
            || v.starts_with("--allow-loopback-http=")
    }) {
        return Err(usage(
            "--local-profile is mutually exclusive with --server and --allow-loopback-http",
        ));
    }
    if flags.iter().any(|v| v.as_str() == "--help") {
        return Ok(remaining);
    }
    let local = LocalRuntime::load(std::path::Path::new(&profile))
        .map_err(|e| (2, Error::new("invalid_input", e.to_string())))?;
    let origin = local.origin().to_owned();
    local
        .install_process()
        .map_err(|e| (2, Error::new("invalid_input", e.to_string())))?;
    remaining.extend(["--server".into(), origin, "--allow-loopback-http".into()]);
    Ok(remaining)
}
