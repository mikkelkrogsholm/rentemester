//! Private backend child entry. No HTTP endpoint, caller actor or path grants access.
use super::{Config, original717};
use rentemester_runtime::{
    auth::{CookieVerifier, SystemClock},
    original716::transfer as source,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::{IsTerminal, Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
};
use zeroize::Zeroizing;
type Error = Box<dyn std::error::Error>;
fn read(path: &Path, uid: u32, max: u64, private: bool) -> Result<Vec<u8>, Error> {
    #[cfg(target_os = "macos")]
    let nofollow = 0x100;
    #[cfg(target_os = "linux")]
    let nofollow = 0x20000;
    let f = OpenOptions::new()
        .read(true)
        .custom_flags(nofollow)
        .open(path)?;
    let m = f.metadata()?;
    if !m.is_file()
        || m.nlink() != 1
        || m.uid() != uid
        || m.len() > max
        || (private && m.mode() & 0o7777 != 0o600)
    {
        return Err(
            "bounded current-user single-link regular file required (private files0600)".into(),
        );
    }
    let mut b = vec![];
    f.take(max + 1).read_to_end(&mut b)?;
    if b.len() as u64 > max {
        return Err("input bound".into());
    }
    Ok(b)
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    File::open(path.parent().ok_or("output parent required")?)?.sync_all()?;
    Ok(())
}
fn required<'a>(f: &BTreeMap<&str, &'a str>, key: &str) -> Result<&'a str, Error> {
    f.get(key)
        .copied()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| format!("Missing required {key}").into())
}
pub fn execute(args: &[String]) -> Option<i32> {
    if args.first().map(String::as_str) != Some("--original716-transfer") {
        return None;
    }
    Some(match run(&args[1..]) {
        Ok((v, machine)) => {
            let ok = v["ok"] == true;
            if machine {
                println!("{}", serde_json::to_string_pretty(&v).unwrap());
            } else if ok {
                println!(
                    "✔ {}",
                    if v["snapshotPath"].is_string() {
                        "Workspace snapshot created"
                    } else {
                        "Workspace restored; bootstrap-owner-then-reinvite"
                    }
                );
            } else {
                eprintln!("✘ {}", v["errors"]);
            }
            if ok { 0 } else { 1 }
        }
        Err(_) => {
            let message = "workspace transfer refused or outcome unknown; reconcile retained files/private stage before another deliberate attempt";
            let machine = args.iter().any(|s| s == "--json" || s == "--format=json")
                || args
                    .windows(2)
                    .any(|a| a[0] == "--format" && a[1] == "json")
                || (!args.iter().any(|s| s == "--format=human")
                    && !args
                        .windows(2)
                        .any(|a| a[0] == "--format" && a[1] == "human")
                    && !std::io::stdout().is_terminal());
            if machine {
                println!(
                    "{}",
                    json!({"ok":false,"appliedRules":[source::RULE],"errors":[message],"outcome":"reconciliation-required"})
                );
            } else {
                eprintln!("{message}");
            }
            1
        }
    })
}
fn run(args: &[String]) -> Result<(Value, bool), Error> {
    if args.first().map(String::as_str) != Some("workspace")
        || !args
            .get(1)
            .is_some_and(|s| matches!(s.as_str(), "snapshot" | "restore"))
    {
        return Err("workspace snapshot/restore required".into());
    }
    let mut f = BTreeMap::new();
    let mut i = 2;
    while i < args.len() {
        let (k, a) = args[i]
            .split_once('=')
            .map_or((args[i].as_str(), None), |(k, v)| (k, Some(v)));
        if !matches!(
            k,
            "--workspace"
                | "--snapshot"
                | "--out"
                | "--target"
                | "--target-workspace"
                | "--confirm"
                | "--at"
                | "--actor"
                | "--json"
                | "--format"
                | "--config"
                | "--server-bin"
                | "--cookie-file"
                | "--origin"
                | "--trust-keys"
                | "--source-unavailable"
        ) {
            return Err("unknown flag".into());
        }
        let v = if k == "--json" {
            if a.is_some() {
                return Err("boolean flag".into());
            }
            "true"
        } else if let Some(v) = a {
            v
        } else {
            i += 1;
            args.get(i)
                .filter(|v| !v.starts_with("--"))
                .ok_or("missing value")?
        };
        if f.insert(k, v).is_some() {
            return Err("duplicate flag".into());
        }
        i += 1;
    }
    let machine = f.contains_key("--json")
        || f.get("--format") == Some(&"json")
        || (f.get("--format") != Some(&"human") && !std::io::stdout().is_terminal());
    if f.get("--confirm")
        .is_none_or(|v| !v.trim().eq_ignore_ascii_case("yes"))
    {
        return Ok((
            json!({"ok":false,"errors":["--confirm yes required"]}),
            machine,
        ));
    }
    let uid = rentemester_runtime::original716::bootstrap::private_process();
    let raw = Zeroizing::new(read(
        Path::new(required(&f, "--config")?),
        uid,
        1024 * 1024,
        true,
    )?);
    let config: Config = serde_json::from_slice(&raw).map_err(|_| "invalid private config")?;
    if config.broker_uid != uid || config.first_owner.is_some() {
        return Err(
            "explicit current-user operator config required, without firstOwner credentials".into(),
        );
    }
    let operator =
        original717::operator(&config).ok_or("current configured operator policy required")?;
    if f.get("--actor").is_some_and(|a| *a != operator.actor()) {
        return Err("caller actor cannot replace current operator".into());
    }
    if args[1] == "snapshot" {
        if Path::new(required(&f, "--workspace")?) != config.root {
            return Err("exact configured workspace required".into());
        }
        let cookies = CookieVerifier::new(
            &config.workspace,
            super::cookie_profile(required(&f, "--origin")?)?,
            &config.cookie_key,
        )?;
        let cookie = Zeroizing::new(read(
            Path::new(required(&f, "--cookie-file")?),
            uid,
            16384,
            true,
        )?);
        let header = std::str::from_utf8(&cookie)?.trim();
        let proof = cookies.verify(Some(header), None)?;
        let mut owner = rentemester_host::Owner::open_with_profile(
            &config.root,
            uid,
            &config.workspace,
            &config.company_slug,
            config.runtime_profile,
        )?;
        let outcome = owner
            .snapshot_original716(
                &cookies,
                &SystemClock,
                &proof,
                &operator,
                f.get("--at").copied(),
            )?
            .outcome;
        let wire = match outcome {
            Ok(w) => w,
            Err(_) => return Err("snapshot authority or effects uncertain".into()),
        };
        let checked = source::check_archive(&wire)?;
        let out = Path::new(required(&f, "--out")?);
        if out.starts_with(&config.root) {
            return Err("external snapshot output required".into());
        }
        let digest = rentemester_runtime::backup_artifact::original717::sha256(&wire);
        let sidecar = out.with_file_name(format!(
            "{}.sha256",
            out.file_name()
                .ok_or("output filename")?
                .to_str()
                .ok_or("output filename")?
        ));
        if out.symlink_metadata().is_ok() || sidecar.symlink_metadata().is_ok() {
            return Err("snapshot output already exists".into());
        }
        write_new(out, &wire)?;
        write_new(
            &sidecar,
            format!(
                "{digest}  {}\n",
                out.file_name().unwrap().to_str().ok_or("output filename")?
            )
            .as_bytes(),
        )?;
        let result = json!({"ok":true,"snapshotPath":out,"sha256":digest,"sha256Path":sidecar,"companyCount":checked.manifest().companies.len(),"accessIdentityCount":checked.access_plan().users.len(),"appliedRules":[source::RULE],"errors":[]});
        owner
            .try_close()
            .map_err(|_| "snapshot owner close failed")?;
        Ok((result, machine))
    } else {
        let target = Path::new(
            f.get("--target-workspace")
                .or(f.get("--target"))
                .copied()
                .ok_or("target workspace required")?,
        );
        if f.contains_key("--target") && f.contains_key("--target-workspace") {
            return Err("duplicate target".into());
        }
        if target != config.root
            || !target.is_absolute()
            || f.get("--source-unavailable") != Some(&"yes")
        {
            return Err(
                "exact fresh configured target and --source-unavailable yes required".into(),
            );
        }
        // Destination must start with no delivery/collection providers. Fresh config
        // contains local control secrets, never any transported source credentials.
        if config.imap_transport.is_some()
            || !config.public_einvoice.is_empty()
            || !config.invoice_mail.is_empty()
        {
            return Err("fresh destination requires disabled providers".into());
        }
        let wire = read(
            Path::new(required(&f, "--snapshot")?),
            uid,
            rentemester_runtime::backup_artifact::MAX_TOTAL_BYTES as u64,
            false,
        )?;
        let keys: BTreeMap<String, String> = serde_json::from_slice(&read(
            Path::new(required(&f, "--trust-keys")?),
            uid,
            1024 * 1024,
            false,
        )?)?;
        let keys = keys.into_iter().map(|(s, p)| (s, p.into_bytes())).collect();
        let checked = source::trust_archive(&wire, &keys)?;
        let owner = rentemester_host::Owner::restore_source_original716(
            target,
            uid,
            &config.workspace,
            &config.company_slug,
            config.runtime_profile,
            &checked,
            &operator,
            true,
        )?;
        owner
            .try_close()
            .map_err(|_| "published restore close failed")?;
        Ok((
            json!({"ok":true,"targetWorkspaceRoot":target,"companyCount":checked.archive().manifest().companies.len(),"accessRecoveryPlanPath":target.join(".rentemester/restored-access-plan.json"),"nextStep":"bootstrap-owner-then-reinvite","appliedRules":[source::RULE],"errors":[]}),
            machine,
        ))
    }
}
