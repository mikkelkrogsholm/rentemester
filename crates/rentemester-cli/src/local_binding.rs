//! One explicit transport join before every existing CLI dispatcher; no server lifecycle.
use rentemester_client::LocalRuntime;
use rentemester_engine::Error;
fn usage(message: &str) -> (u8, Error) {
    (2, Error::new("invalid_input", message))
}
pub fn prepare(args: Vec<String>) -> Result<Vec<String>, (u8, Error)> {
    let registry = crate::corporate_records_original::accepts(&args) || crate::ownership_original::accepts(&args);
    let mut workspace_assertion = None;
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
        if registry && normal && (arg == "--workspace" || arg.starts_with("--workspace=")) {
            if workspace_assertion.is_some() { return Err(usage("duplicate --workspace assertion")); }
            let value = if let Some(value) = arg.strip_prefix("--workspace=") { value.to_owned() } else {
                cursor += 1;
                args.get(cursor).filter(|v| !v.starts_with("--")).ok_or_else(|| usage("--workspace requires an exact verified local workspace"))?.clone()
            };
            if value.is_empty() { return Err(usage("empty --workspace assertion")); }
            workspace_assertion = Some(value); cursor += 1; continue;
        }
        remaining.push(arg.clone());
        cursor += 1;
    }
    // Native and experimental dialects retain their own company semantics.
    let knowledge = remaining.first().is_some_and(|s| s == "remote")
        && remaining.get(1).is_some_and(|s| crate::company_knowledge::accepts(s));
    let company_bound = (knowledge || crate::original_args::requires_company(&remaining))
        && !remaining
            .iter()
            .take_while(|s| s.as_str() != "--")
            .any(|s| s == "--help");
    let company = if company_bound {
        select_company(&mut remaining)?
    } else {
        None
    };
    if let Some(company) = &company {
        if !slug(company) && profile.is_none() {
            return Err(usage(
                "company paths require explicit --local-profile; otherwise use an ASCII company slug",
            ));
        }
        check_path_syntax(company)?;
    }
    let Some(profile) = profile else {
        if workspace_assertion.is_some() { return Err(usage("--workspace assertion requires explicit --local-profile")); }
        if let Some(company) = company {
            insert_company(&mut remaining, company);
        }
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
    if workspace_assertion.as_ref().is_some_and(|w| std::path::Path::new(w) != local.workspace_root()) {
        return Err(usage("--workspace differs from verified local workspace"));
    }
    if let Some(company) = company {
        assert_workspace(&local)?;
        let company = if knowledge {
            rentemester_client::knowledge_company_selector(Some(&local), &company)
                .map_err(|e| (2, Error::new("invalid_input", e.to_string())))?
        } else if slug(&company) {
            company
        } else {
            local_company(&local, &company)?
        };
        insert_company(&mut remaining, company);
    }
    let origin = local.origin().to_owned();
    local
        .install_process()
        .map_err(|e| (2, Error::new("invalid_input", e.to_string())))?;
    remaining.extend(["--server".into(), origin, "--allow-loopback-http".into()]);
    Ok(remaining)
}

fn slug(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}

fn select_company(args: &mut Vec<String>) -> Result<Option<String>, (u8, Error)> {
    let selectors: Vec<_> = args
        .iter()
        .take_while(|arg| arg.as_str() != "--")
        .enumerate()
        .filter(|(_, arg)| crate::original_args::option(arg).0 == "--company")
        .map(|(index, _)| index)
        .collect();
    // Existing family parsers own duplicate/missing-company diagnostics and
    // their output streams. Do not partially consume an invalid invocation.
    if selectors.len() > 1 {
        return Ok(None);
    }
    let explicit = selectors
        .first()
        .map(|&index| {
            crate::original_args::value(args, index)
                .map(|(value, next)| (index, next, value.trim().to_owned()))
                .ok_or_else(|| usage("--company requires a company slug or verified local path"))
        })
        .transpose()?;
    let selected = explicit
        .as_ref()
        .map(|(_, _, value)| value.clone())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            std::env::var("RENTEMESTER_COMPANY")
                .ok()
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
        });
    if selected.is_some() && let Some((index, next, _)) = explicit {
        args.drain(index..next);
    }
    Ok(selected)
}

fn insert_company(args: &mut Vec<String>, company: String) {
    let end = args.iter().position(|s| s == "--").unwrap_or(args.len());
    args.splice(end..end, ["--company".into(), company]);
}

fn check_path_syntax(value: &str) -> Result<(), (u8, Error)> {
    use std::path::{Component, Path};
    if slug(value) {
        return Ok(());
    }
    if !value.contains('/')
        || value.contains('\\')
        || value.chars().any(char::is_control)
        || Path::new(value)
            .components()
            .any(|c| matches!(c, Component::ParentDir))
    {
        return Err(usage(
            "company must be an ASCII slug or a local path without parent traversal or backslashes",
        ));
    }
    Ok(())
}

fn assert_workspace(local: &LocalRuntime) -> Result<(), (u8, Error)> {
    if let Some(asserted) = std::env::var("RENTEMESTER_WORKSPACE")
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
    {
        // Assertion only: never discover or open a different workspace.
        if std::path::Path::new(&asserted) != local.workspace_root() {
            return Err(usage(
                "RENTEMESTER_WORKSPACE differs from the verified local workspace",
            ));
        }
    }
    Ok(())
}

fn local_company(local: &LocalRuntime, value: &str) -> Result<String, (u8, Error)> {
    use std::path::{Component, Path, PathBuf};
    let invalid = || {
        usage(
            "company path must be an existing non-symlink direct child of the verified local workspace",
        )
    };
    let path = Path::new(value);
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir().map_err(|_| invalid())?.join(path)
    };
    // Check every component before canonicalization: canonicalize alone would
    // silently admit a symlink alias (including one inside the workspace).
    let mut checked = PathBuf::new();
    for component in absolute.components() {
        if matches!(component, Component::CurDir) {
            continue;
        }
        checked.push(component.as_os_str());
        let metadata = std::fs::symlink_metadata(&checked).map_err(|_| invalid())?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(invalid());
        }
    }
    let canonical = std::fs::canonicalize(&checked).map_err(|_| invalid())?;
    if canonical.parent() != Some(local.workspace_root()) {
        return Err(invalid());
    }
    let name = canonical
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(invalid)?;
    if !slug(name) {
        return Err(invalid());
    }
    Ok(name.to_owned())
}
