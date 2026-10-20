//! Offline application-only updater. Explicit trust anchor, no network or workspace.
use rentemester_runtime::package_update::{self as update, Error};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

fn run() -> Result<Value, (u8, Error)> {
    let mut args = std::env::args().skip(1);
    let command = args.next().ok_or((2, Error("update_command_required")))?;
    let mut flags = BTreeMap::new();
    let mut confirm = false;
    // Parse separately so boolean confirmation cannot consume a following flag.
    let mut args = std::env::args().skip(2);
    while let Some(flag) = args.next() {
        if flag == "--confirm" {
            if confirm {
                return Err((2, Error("duplicate_update_flag")));
            }
            confirm = true;
            continue;
        }
        if !matches!(flag.as_str(), "--package" | "--installed" | "--trusted-key") {
            return Err((2, Error("unknown_update_flag")));
        }
        let value = args
            .next()
            .filter(|v| !v.starts_with("--"))
            .ok_or((2, Error("update_flag_value_required")))?;
        if flags.insert(flag, value).is_some() {
            return Err((2, Error("duplicate_update_flag")));
        }
    }
    let mutation = matches!(command.as_str(), "apply" | "recover");
    if !matches!(command.as_str(), "verify" | "status" | "apply" | "recover") || mutation != confirm
    {
        return Err((
            2,
            Error("explicit_update_command_and_confirmation_required"),
        ));
    }
    let trusted = update::read_trusted_key(Path::new(
        flags
            .get("--trusted-key")
            .ok_or((2, Error("independent_update_trust_key_required")))?,
    ))
    .map_err(|e| (1, e))?;
    let policy = update::current_policy();
    if command == "verify" {
        if flags.contains_key("--installed") {
            return Err((2, Error("unexpected_update_selector")));
        }
        let root = PathBuf::from(
            flags
                .get("--package")
                .ok_or((2, Error("update_package_required")))?,
        );
        let verified = update::verify(&root, &trusted, &policy).map_err(|e| (1, e))?;
        return Ok(
            json!({"ok":true,"version":verified.manifest().version,"sequence":verified.manifest().sequence,"manifestSha256":verified.manifest_sha256(),"applicationSigningQualified":false,"workspaceOpened":false}),
        );
    }
    let target = PathBuf::from(
        flags
            .get("--installed")
            .ok_or((2, Error("installed_package_required")))?,
    );
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    let result = match command.as_str() {
        "status" | "recover" => {
            if flags.contains_key("--package") {
                return Err((2, Error("unexpected_update_selector")));
            }
            if command == "status" {
                update::status(&target, &trusted, &policy)
            } else {
                update::recover(&target, &trusted, &policy)
            }
        }
        "apply" => {
            let candidate = PathBuf::from(
                flags
                    .get("--package")
                    .ok_or((2, Error("update_package_required")))?,
            );
            update::install(&candidate, &target, &trusted, &policy)
        }
        _ => Err(Error("unknown_update_command")),
    };
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let result = {
        let _ = target;
        Err(Error("update_platform_unqualified"))
    };
    result.map_err(|e| (1, e))
}
fn main() {
    if std::env::args().any(|arg| arg == "--help") {
        println!(
            "rentemester-update verify --package CANDIDATE.app --trusted-key INDEPENDENT_RAW32_PUBLIC_KEY\nrentemester-update status --installed INSTALLED.app --trusted-key PUBLIC_KEY\nrentemester-update apply --package CANDIDATE.app --installed INSTALLED.app --trusted-key PUBLIC_KEY --confirm\nrentemester-update recover --installed INSTALLED.app --trusted-key PUBLIC_KEY --confirm\nClose the app first. Existing installation must already have an authenticated manifest. No network, workspace access, schema migration or automatic rollback. After an interrupted operation inspect status, then explicitly recover; never blindly replay apply. Apple signing/notarization remains a separate release gate."
        );
        return;
    }
    match run() {
        Ok(value) => println!("{value}"),
        Err((exit, error)) => {
            println!(
                "{}",
                json!({"ok":false,"code":error.0,"workspaceOpened":false})
            );
            std::process::exit(i32::from(exit));
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn update_policy_matches_the_exact_fresh_database_schemas() {
        let mut control = rusqlite::Connection::open_in_memory().unwrap();
        let mut company = rusqlite::Connection::open_in_memory().unwrap();
        let initialized = rentemester_storage::fresh::initialize_empty_pair(
            &mut control,
            &mut company,
            "Synthetic package update",
        )
        .unwrap();
        let policy = rentemester_runtime::package_update::current_policy();
        assert_eq!(initialized.control_version, policy.control_schema);
        assert_eq!(initialized.company_version, policy.company_schema);
    }
}
