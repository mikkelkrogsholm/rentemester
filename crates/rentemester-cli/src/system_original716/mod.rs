//! Source CLI backup boundary. No ledger access, native-artifact substitution or retries.
use crate::dated_reports::Output;
use rentemester_client::{ApiClient, LocalRuntime};
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::IsTerminal, path::Path};
mod presentation;
type Flags<'a> = BTreeMap<&'a str, &'a str>;
fn specs() -> Vec<Value> {
    serde_json::from_str(include_str!("specs.json")).expect("frozen source specs")
}
fn spec(command: &str) -> Option<Value> {
    specs()
        .into_iter()
        .find(|s| s["key"] == format!("system {command}"))
}
fn usage(s: impl ToString) -> Output {
    Output {
        stdout: String::new(),
        stderr: format!("{}\n", s.to_string()),
        exit: 2,
    }
}
fn boolean(k: &str) -> bool {
    matches!(
        k,
        "--json" | "--help" | "--allow-loopback-http" | "--archive" | "--sign-with-ed25519"
    )
}
/// Move only the two command words, preserving every literal option/value byte.
pub fn prepare(args: &[String]) -> Option<Vec<String>> {
    let (mut words, mut i) = (vec![], 0);
    while i < args.len() {
        let (k, a) = crate::original_args::option(&args[i]);
        if k.starts_with("--") {
            i = if a.is_some() || boolean(k) {
                i + 1
            } else {
                crate::original_args::value(args, i).map_or(i + 1, |(_, n)| n)
            };
        } else {
            words.push(i);
            i += 1;
        }
    }
    if words.len() < 2 || args[words[0]] != "system" || spec(&args[words[1]]).is_none() {
        return None;
    }
    let mut out = vec!["system".into(), args[words[1]].clone()];
    out.extend(
        args.iter()
            .enumerate()
            .filter(|(i, _)| !words[..2].contains(i))
            .map(|(_, v)| v.clone()),
    );
    Some(out)
}
fn parse<'a>(args: &'a [String], s: &Value) -> Result<Flags<'a>, Output> {
    let mut f = BTreeMap::new();
    let mut i = 2;
    while i < args.len() {
        let (k, a) = crate::original_args::option(&args[i]);
        let global = matches!(
            k,
            "--server"
                | "--local-profile"
                | "--cookie-file"
                | "--service-key-file"
                | "--allow-loopback-http"
                | "--actor"
                | "--json"
                | "--format"
                | "--help"
        );
        if !global && !s["allowedFlags"].as_array().unwrap().iter().any(|v| v == k) {
            return Err(usage(format!("Unknown flag {k}")));
        }
        let (v, n) = if boolean(k) {
            if a.is_some() {
                return Err(usage(format!("{k} does not take a value")));
            }
            ("true", i + 1)
        } else {
            crate::original_args::value(args, i)
                .ok_or_else(|| usage(format!("Missing value for {k}")))?
        };
        if k == "--format" && !matches!(v, "json" | "human") {
            return Err(usage("--format must be json or human"));
        }
        f.insert(k, v);
        i = n;
    }
    if f.contains_key("--local-profile")
        && (f.contains_key("--server") || f.contains_key("--allow-loopback-http"))
    {
        return Err(usage(
            "--local-profile conflicts with --server/--allow-loopback-http",
        ));
    }
    Ok(f)
}
fn required(f: &Flags<'_>, keys: &[&str], message: &str) -> Result<(), Output> {
    if keys.iter().any(|k| f.get(k).is_none_or(|v| v.is_empty())) {
        Err(usage(message))
    } else {
        Ok(())
    }
}
fn number(raw: &str) -> Option<f64> {
    let v = raw.trim();
    if v.is_empty() {
        return Some(0.);
    }
    for (p, r) in [
        ("0x", 16),
        ("0X", 16),
        ("0b", 2),
        ("0B", 2),
        ("0o", 8),
        ("0O", 8),
    ] {
        if let Some(v) = v.strip_prefix(p) {
            return u64::from_str_radix(v, r).ok().map(|n| n as f64);
        }
    }
    v.parse::<f64>().ok().filter(|n| !n.is_nan())
}
fn request(command: &str, f: &Flags<'_>) -> Result<Value, Output> {
    let mut b = json!({});
    for (flag, key) in [
        ("--at", "at"),
        ("--as-of", "asOf"),
        ("--backup-id", "backupId"),
        ("--out", "out"),
        ("--label", "label"),
        ("--kind", "kind"),
        ("--location", "location"),
        ("--attested-by", "attestedBy"),
        ("--region-country", "regionCountry"),
        ("--region-note", "regionNote"),
        ("--it-security-note", "itSecurityNote"),
        ("--id", "id"),
        ("--archive-file", "archivePath"),
        ("--destination", "destinationId"),
        ("--actor-kind", "actorKind"),
        ("--note", "note"),
        ("--archive-sha256", "archiveSha256"),
        ("--remote-object-id", "remoteObjectId"),
        ("--backup-dir", "backupDir"),
        ("--target-company", "targetCompany"),
        ("--verify-key", "verifyKey"),
        ("--public-key", "publicKey"),
    ] {
        if let Some(v) = f.get(flag) {
            b[key] = json!(v);
        }
    }
    for (flag, key) in [
        ("--enforce", "enforced"),
        ("--region-eu", "inEeaOrEu"),
        ("--non-related", "nonRelatedParty"),
        ("--it-security", "itSecurityMeetsStandards"),
    ] {
        if let Some(v) = f.get(flag) {
            b[key] = match *v {
                "true" => json!(true),
                "false" => json!(false),
                _ => return Err(usage(format!("{flag} must be true or false"))),
            };
        }
    }
    if command == "backup-add-destination" && !f.contains_key("--region-eu") {
        return Err(usage("--region-eu is required and must be true or false"));
    }
    for (flag, key) in [
        ("--grace-days", "graceDays"),
        ("--archive-size", "archiveSizeBytes"),
        ("--max-metadata-age-ms", "maxMetadataAgeMs"),
    ] {
        if let Some(v) = f.get(flag) {
            let n =
                number(v).ok_or_else(|| usage(format!("{flag} must be numeric when present")))?;
            b[key] = serde_json::Number::from_f64(n)
                .map(Value::Number)
                .unwrap_or(Value::Null);
        }
    }
    if f.get("--actor-kind")
        .is_some_and(|v| !matches!(*v, "human" | "agent"))
    {
        return Err(usage("--actor-kind must be human or agent"));
    }
    match command {
        "backup-remove-destination" => required(f, &["--id"], "Missing required --id <dest-id>")?,
        "backup-place" => required(
            f,
            &["--archive-file", "--destination"],
            "Missing required --archive-file <file.tar> or --destination <dest-id>",
        )?,
        "backup-confirm-placement" => required(
            f,
            &["--destination", "--backup-id", "--archive-sha256"],
            "Missing required --destination <dest-id>, --backup-id <id> or --archive-sha256 <hex>",
        )?,
        "backup-verify-remote-placement" => required(
            f,
            &["--destination", "--backup-id", "--remote-object-id"],
            "Missing required --destination, --backup-id, or --remote-object-id",
        )?,
        "backup-lock" if !f.contains_key("--enforce") && !f.contains_key("--grace-days") => {
            return Err(usage(
                "Missing change: specify --enforce true|false and/or --grace-days <n>",
            ));
        }
        "restore-backup" => required(
            f,
            &["--backup-dir", "--target-company"],
            "Missing required --backup-dir <dir> or --target-company <path>",
        )?,
        "backup-guide" => required(f, &["--out"], "Missing required --out <file.html>")?,
        "export-public-key" => required(f, &["--out"], "Missing required --out <file>")?,
        _ => {}
    }
    if command == "backup" && f.contains_key("--archive") {
        b["archive"] = json!(true);
    }
    Ok(b)
}
fn fail(command: &str, f: &Flags<'_>, message: impl ToString) -> Output {
    presentation::output(
        command,
        json!({"ok":false,"errors":[message.to_string()]}),
        f,
    )
}
/// Dispatch only this family's fourteen keys. All mutations are single-shot SDK calls.
pub async fn execute(args: &[String]) -> Option<Output> {
    let normalized = prepare(args)?;
    Some(run(&normalized).await)
}
async fn run(args: &[String]) -> Output {
    let command = &args[1];
    let s = spec(command).unwrap();
    let f = match parse(args, &s) {
        Ok(f) => f,
        Err(o) => return o,
    };
    if f.contains_key("--help") {
        return Output {
            stdout: format!("{}\n", s["usage"].as_str().unwrap()),
            stderr: String::new(),
            exit: 0,
        };
    }
    let mut body = match request(command, &f) {
        Ok(b) => b,
        Err(o) => return o,
    };
    // Source optional valued confirmation: absence/no => domain refusal, bare flag => parse failure.
    if command == "restore-backup"
        && f.get("--confirm")
            .is_none_or(|v| v.trim().to_lowercase() != "yes")
    {
        return fail(
            command,
            &f,
            "system restore-backup is destructive: it can overwrite files in --target-company. Re-run with --confirm yes to proceed.",
        );
    }
    let mut local = None;
    if let Some(profile) = f.get("--local-profile") {
        local = match LocalRuntime::load(Path::new(profile)) {
            Ok(l) => Some(l),
            Err(e) => return fail(command, &f, e),
        };
    }
    let origin = local
        .as_ref()
        .map(LocalRuntime::origin)
        .or_else(|| f.get("--server").copied());
    let Some(origin) = origin else {
        return usage("--server or --local-profile required");
    };
    if let Some(l) = local.clone() {
        if let Err(e) = l.install_process() {
            return fail(command, &f, e);
        }
    }
    let api = match crate::remote::credential_client(
        origin,
        local.is_some() || f.contains_key("--allow-loopback-http"),
        &f,
        false,
    ) {
        Ok(a) => a,
        Err((exit, e)) => {
            return Output {
                stdout: String::new(),
                stderr: format!("{}: {}\n", e.code, e.message),
                exit,
            };
        }
    };
    let company = f
        .get("--company")
        .map(|s| s.to_string())
        .or_else(|| std::env::var("RENTEMESTER_COMPANY").ok());
    if let Some(actor) = f.get("--actor") {
        if f.contains_key("--service-key-file") {
            return usage(
                "--actor assertion requires a current human cookie; service credentials determine their own principal",
            );
        }
        let me = match api.me().await {
            Ok(v) => v,
            Err(e) => return fail(command, &f, e),
        };
        if me["user"]["id"]
            .as_str()
            .is_none_or(|id| *actor != format!("user:{id}"))
        {
            return fail(command, &f, "actor differs from current cookie user");
        }
    }
    let company = if command == "restore-backup" {
        String::new()
    } else {
        let Some(c) = company else {
            return usage("Missing required --company <slug|path>");
        };
        match api.original717_company_selector(&c) {
            Ok(c) => c,
            Err(e) => return fail(command, &f, e),
        }
    };
    if matches!(
        command.as_str(),
        "backup-place" | "backup-confirm-placement" | "backup-verify-remote-placement"
    ) && !f.contains_key("--actor-kind")
    {
        // Without a caller assertion the source CLI defaults to a human operator. Server owns actual actor.
        body["actorKind"] = json!("human");
    }
    let result = match command.as_str() {
        "export-public-key" => {
            body["company"] = json!(company);
            api.original716_system(command, &body).await
        }
        "backup" if f.contains_key("--sign-with-ed25519") => {
            body["company"] = json!(company);
            body["confirm"] = json!(true);
            body["signWithEd25519"] = json!(true);
            api.original716_system(command, &body).await
        }
        "backup-status" => {
            api.system_backup_status_original122(&company, f.get("--as-of").copied())
                .await
        }
        "backup-lock" => {
            body["confirm"] = json!(true);
            api.system_backup_lock_original122(&company, serde_json::to_vec(&body).unwrap(), true)
                .await
        }
        "backup-governance"
        | "backup-destinations"
        | "backup-add-destination"
        | "backup-remove-destination"
        | "backup-confirm-placement" => {
            let name = match command.as_str() {
                "backup-governance" => "system_backup_governance",
                "backup-destinations" => "system_backup_destination_list",
                "backup-add-destination" => "system_backup_destination_add",
                "backup-remove-destination" => "system_backup_destination_remove",
                _ => "system_backup_confirm_placement",
            };
            if !matches!(
                command.as_str(),
                "backup-governance" | "backup-destinations"
            ) {
                body["confirm"] = json!(true);
            }
            api.backup_views_original717(name, &company, body).await
        }
        "backup-guide" => return guide(&api, &company, &f, local.as_ref()).await,
        _ => {
            body["confirm"] = json!(true);
            let name = if command == "restore-backup" {
                body["confirmText"] = json!(format!("RESTORE {}", f["--target-company"]));
                "system_restore_backup".to_owned()
            } else {
                body["company"] = json!(company);
                format!("system_{}", command.replace('-', "_"))
            };
            api.original717(&name, &body).await
        }
    };
    match result {
        Ok(v) => presentation::output(command, v, &f),
        Err(e) => {
            let (exit, e) = crate::remote::client_error(e);
            Output {
                stdout: String::new(),
                stderr: format!("{}: {}\n", e.code, e.message),
                exit,
            }
        }
    }
}
async fn guide(
    api: &ApiClient,
    company: &str,
    f: &Flags<'_>,
    local: Option<&LocalRuntime>,
) -> Output {
    let Some(local) = local else {
        return fail(
            "backup-guide",
            f,
            "backup-guide output requires an explicit verified --local-profile",
        );
    };
    let mut q = json!({});
    if let Some(v) = f.get("--as-of") {
        q["asOf"] = json!(v);
    }
    let governance = match api
        .backup_views_original717("system_backup_governance", company, q)
        .await
    {
        Ok(v) => v,
        Err(e) => return fail("backup-guide", f, e),
    };
    let profile = match api.company_profile(company).await {
        Ok(v) => v,
        Err(e) => return fail("backup-guide", f, e),
    };
    if !profile["company"]["name"].is_string() {
        return fail("backup-guide", f, "Malformed canonical company profile");
    }
    let html = presentation::guide(profile["company"]["name"].as_str().unwrap(), &governance);
    // No company/ledger filesystem access. Private operator-selected presentation output only.
    if let Err(e) = presentation::write_private(
        Path::new(f["--out"]),
        html.as_bytes(),
        local
            .workspace_root()
            .parent()
            .unwrap_or(local.workspace_root()),
    ) {
        return fail("backup-guide", f, e);
    }
    presentation::output(
        "backup-guide",
        json!({"ok":true,"out":f["--out"],"checkedAt":governance["checkedAt"]}),
        f,
    )
}
