//! Original standalone auditor verification and operator rotation through the common Owner.
use crate::dated_reports::Output;
use rentemester_client::LocalRuntime;
use serde_json::json;
use std::{collections::BTreeMap, path::Path};
fn usage(message: &str) -> Output {
    Output {
        stdout: String::new(),
        stderr: format!("{message}\n"),
        exit: 2,
    }
}
fn accepts(command: &str) -> bool {
    matches!(command, "verify-backup-signature" | "rotate-backup-keypair")
}
pub fn prepare(args: &[String]) -> Option<Vec<String>> {
    let (mut words, mut i) = (vec![], 0);
    while i < args.len() {
        let (k, a) = crate::original_args::option(&args[i]);
        if k.starts_with("--") {
            i = if a.is_some() || matches!(k, "--json" | "--help" | "--allow-loopback-http") {
                i + 1
            } else {
                crate::original_args::value(args, i).map_or(i + 1, |(_, n)| n)
            };
        } else {
            words.push(i);
            i += 1;
        }
    }
    if words.len() < 2 || args[words[0]] != "system" || !accepts(&args[words[1]]) {
        return None;
    }
    let mut out = vec!["system".into(), args[words[1]].clone()];
    out.extend(
        args.iter()
            .enumerate()
            .filter(|(i, _)| !words[..2].contains(i))
            .map(|(_, s)| s.clone()),
    );
    Some(out)
}
fn parse(args: &[String]) -> Result<BTreeMap<&str, &str>, Output> {
    let rotate = args[1] == "rotate-backup-keypair";
    let mut flags = BTreeMap::new();
    let mut i = 2;
    while i < args.len() {
        let (k, a) = crate::original_args::option(&args[i]);
        let global = matches!(
            k,
            "--local-profile"
                | "--cookie-file"
                | "--server"
                | "--allow-loopback-http"
                | "--json"
                | "--format"
                | "--help"
                | "--actor"
        );
        if !global
            && !(if rotate {
                matches!(k, "--company" | "--reason" | "--at")
            } else {
                matches!(k, "--backup-dir" | "--public-key" | "--verify-key")
            })
        {
            return Err(usage(&format!("Unknown flag {k}")));
        }
        let (v, n) = if matches!(k, "--json" | "--help" | "--allow-loopback-http") {
            if a.is_some() {
                return Err(usage("Boolean flag takes no value"));
            }
            ("true", i + 1)
        } else {
            crate::original_args::value(args, i).ok_or_else(|| usage("Missing flag value"))?
        };
        if k == "--format" && !matches!(v, "json" | "human") {
            return Err(usage("--format must be json or human"));
        }
        flags.insert(k, v);
        i = n;
    }
    if flags.contains_key("--help") {
        return Ok(flags);
    }
    if rotate {
        if flags.get("--reason").is_none_or(|v| v.trim().is_empty()) {
            return Err(usage("Missing required --reason \"<text>\""));
        }
        if !flags.contains_key("--company") {
            return Err(usage("Missing required --company <slug|path>"));
        }
    } else if !flags.contains_key("--backup-dir") {
        return Err(usage("Missing required --backup-dir <dir>"));
    }
    if !flags.contains_key("--local-profile") {
        return Err(usage(
            "Trusted local --local-profile required for operator custody paths",
        ));
    }
    if flags.contains_key("--server") || flags.contains_key("--allow-loopback-http") {
        return Err(usage(
            "--local-profile conflicts with --server/--allow-loopback-http",
        ));
    }
    Ok(flags)
}
pub async fn execute(args: &[String]) -> Option<Output> {
    if args.first().map(String::as_str) != Some("system")
        || !args.get(1).is_some_and(|s| accepts(s))
    {
        return None;
    }
    Some(run(args).await)
}
async fn run(args: &[String]) -> Output {
    let flags = match parse(args) {
        Ok(v) => v,
        Err(o) => return o,
    };
    if flags.contains_key("--help") {
        return Output {
            stdout: format!(
                "{}\n",
                if args[1] == "rotate-backup-keypair" {
                    "system rotate-backup-keypair --company <path> --reason \"<text>\" [--at <ISO-timestamp>]"
                } else {
                    "system verify-backup-signature --backup-dir <dir> [--public-key <path>] [--verify-key <path>]"
                }
            ),
            stderr: String::new(),
            exit: 0,
        };
    }
    let local = match LocalRuntime::load(Path::new(flags["--local-profile"])) {
        Ok(v) => v,
        Err(e) => return usage(&e.to_string()),
    };
    if let Err(e) = local.clone().install_process() {
        return usage(&e.to_string());
    }
    let api = match crate::remote::credential_client(local.origin(), true, &flags, false) {
        Ok(v) => v,
        Err((exit, e)) => {
            return Output {
                stdout: String::new(),
                stderr: format!("{}: {}\n", e.code, e.message),
                exit,
            };
        }
    };
    if let Some(actor) = flags.get("--actor") {
        match api.me().await {
            Ok(me)
                if me["user"]["id"]
                    .as_str()
                    .is_some_and(|id| *actor == format!("user:{id}")) => {}
            _ => return usage("actor differs from current cookie user"),
        }
    }
    let mut body = json!({});
    for (flag, key) in [
        ("--reason", "reason"),
        ("--at", "at"),
        ("--backup-dir", "backupDir"),
        ("--public-key", "publicKey"),
        ("--verify-key", "verifyKey"),
    ] {
        if let Some(v) = flags.get(flag) {
            body[key] = json!(v);
        }
    }
    if let Some(company) = flags.get("--company") {
        match api.original717_company_selector(company) {
            Ok(v) => body["company"] = json!(v),
            Err(e) => return usage(&e.to_string()),
        }
    }
    let result = match api.original716_custody(&args[1], &body).await {
        Ok(v) => v,
        Err(e) => json!({"ok":false,"errors":[e.to_string()]}),
    };
    let exit = if result["ok"] == true { 0 } else { 1 };
    Output {
        stdout: format!("{}\n", serde_json::to_string_pretty(&result).unwrap()),
        stderr: String::new(),
        exit,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn a(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }
    #[test]
    fn original716_custody_reason_and_literal_boundaries() {
        assert!(
            parse(&a(&[
                "system",
                "rotate-backup-keypair",
                "--company",
                "synthetic",
                "--reason",
                " ",
                "--local-profile",
                "/fixture"
            ]))
            .is_err()
        );
        assert!(parse(&a(&["system", "verify-backup-signature"])).is_err());
        assert!(prepare(&a(&["--company", "system", "rotate-backup-keypair"])).is_none());
        assert_eq!(
            prepare(&a(&["--reason=a=b", "system", "rotate-backup-keypair"])),
            Some(a(&["system", "rotate-backup-keypair", "--reason=a=b"]))
        );
    }
}
