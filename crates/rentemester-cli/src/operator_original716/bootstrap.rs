//! Original four onboarding commands. The CLI never owns runtime connections.
//! An explicit packaged/sibling native server runs the same existing Owner engine.
use crate::dated_reports::Output;
use std::{
    collections::BTreeMap,
    path::PathBuf,
    process::{Command, Stdio},
};
fn usage(message: impl ToString) -> Output {
    Output {
        stdout: String::new(),
        stderr: format!("{}\n", message.to_string()),
        exit: 2,
    }
}
fn boolean(k: &str) -> bool {
    matches!(k, "--json" | "--help" | "--no-vat" | "--no-open")
}
fn count(args: &[String]) -> Option<usize> {
    match args.first()?.as_str() {
        "init" | "serve" => Some(1),
        "local" if args.get(1).map(String::as_str) == Some("start") => Some(2),
        "workspace-access" if args.get(1).map(String::as_str) == Some("bootstrap-first") => Some(2),
        _ => None,
    }
}
/// Only command words move; consumed values are never reinterpreted as commands.
pub fn prepare(args: &[String]) -> Option<Vec<String>> {
    let (mut words, mut i) = (Vec::new(), 0);
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
    let selected: Vec<String> = words.iter().map(|i| args[*i].clone()).collect();
    let n = count(&selected)?;
    let mut out = selected[..n].to_vec();
    out.extend(
        args.iter()
            .enumerate()
            .filter(|(i, _)| !words[..n].contains(i))
            .map(|(_, v)| v.clone()),
    );
    Some(out)
}
fn parse(args: &[String], n: usize) -> Result<BTreeMap<&str, &str>, Output> {
    let mut f = BTreeMap::new();
    let mut i = n;
    while i < args.len() {
        let (k, a) = crate::original_args::option(&args[i]);
        let common = matches!(
            k,
            "--workspace" | "--runtime-config" | "--actor" | "--json" | "--format" | "--help"
        );
        let allowed = match args[0].as_str() {
            "init" => matches!(
                k,
                "--company"
                    | "--name"
                    | "--cvr"
                    | "--address"
                    | "--postal-code"
                    | "--city"
                    | "--payment-terms"
                    | "--bank-name"
                    | "--bank-reg"
                    | "--bank-account"
                    | "--iban"
                    | "--fiscal-year-start-month"
                    | "--fiscal-year-label-strategy"
                    | "--vat-period"
                    | "--no-vat"
            ),
            "serve" => matches!(k, "--host" | "--port" | "--origin" | "--web-dir"),
            "local" => matches!(
                k,
                "--web-dir" | "--port" | "--company-name" | "--company" | "--confirm" | "--no-open"
            ),
            _ => matches!(
                k,
                "--company" | "--name" | "--email" | "--password-file" | "--confirm" | "--origin"
            ),
        };
        if !common && !allowed {
            return Err(usage(format!("Unknown flag {k}")));
        }
        let (v, next) = if boolean(k) {
            if a.is_some() {
                return Err(usage("Boolean flag takes no value"));
            }
            ("true", i + 1)
        } else {
            crate::original_args::value(args, i)
                .ok_or_else(|| usage(format!("Missing value for {k}")))?
        };
        if f.insert(k, v).is_some() {
            return Err(usage(format!("Duplicate flag {k}")));
        }
        if k == "--format" && !matches!(v, "json" | "human") {
            return Err(usage("--format must be either json or human"));
        }
        i = next;
    }
    if f.contains_key("--help") {
        return Ok(f);
    }
    if args[0] == "local" && !f.contains_key("--workspace") {
        return Err(usage("local start requires --workspace <dir>"));
    }
    if args[0] == "init"
        && f.contains_key("--no-vat")
        && f.get("--vat-period").is_some_and(|v| *v != "none")
    {
        return Err(usage(
            "--no-vat cannot be combined with --vat-period <cadence>",
        ));
    }
    if let Some(port) = f.get("--port") {
        if port.parse::<u16>().ok().is_none_or(|p| p == 0) {
            return Err(usage("--port must be an integer from 1 to 65535"));
        }
    }
    Ok(f)
}
fn binary() -> Result<PathBuf, Output> {
    if let Some(path) = std::env::var_os("RENTEMESTER_SERVER_BIN") {
        let p = PathBuf::from(path);
        if !p.is_absolute() {
            return Err(usage(
                "RENTEMESTER_SERVER_BIN must be an absolute executable path",
            ));
        }
        return Ok(p);
    }
    Ok(std::env::current_exe()
        .map_err(usage)?
        .with_file_name("rentemester-server"))
}
/// Same dispatcher signature as provider/system. Spawns once, no shell/cargo/Bun,
/// inherits cancellation/signals in the foreground and never retries unknown effects.
pub async fn execute(args: &[String]) -> Option<Output> {
    let n = count(args)?;
    let f = match parse(args, n) {
        Ok(f) => f,
        Err(e) => return Some(e),
    };
    if f.contains_key("--help") {
        let help = match args[0].as_str() {
            "init" => {
                "init --company <slug> --workspace <dir> [--name <text>] [--cvr <DK########>] [--address <text>] [--postal-code <text>] [--city <text>] [--payment-terms <days>] [--bank-name <text>] [--bank-reg <text>] [--bank-account <text>] [--iban <text>] [--fiscal-year-start-month <1..12>] [--fiscal-year-label-strategy <end-year|start-year|span>] [--vat-period <month|quarter|half-year|none> | --no-vat] --actor <id>"
            }
            "serve" => "serve --workspace <dir> [--host <host>] [--port <port>]",
            "local" => {
                "local start --workspace <dir> [--company-name <text> --actor <id> --confirm yes] [--port <port>] [--no-open]"
            }
            _ => {
                "workspace-access bootstrap-first --workspace <dir> --company <slug> --name <text> --email <email> --password-file <private0600-file> --confirm yes"
            }
        };
        return Some(Output {
            stdout: format!(
                "{help}\n\nOptional --runtime-config <PRIVATE.json> selects existing operator policy. Fresh init generates a private sibling runtime config; existing workspaces are never adopted.\n"
            ),
            stderr: String::new(),
            exit: 0,
        });
    }
    if args[0] == "workspace-access"
        && f.get("--confirm")
            .is_none_or(|s| !s.trim().eq_ignore_ascii_case("yes"))
    {
        return Some(Output{stdout:"{\"ok\":false,\"errors\":[\"--confirm yes required to bootstrap the first workspace identity\"]}\n".into(),stderr:String::new(),exit:1});
    }
    if args[0] == "local" && f.contains_key("--company-name") && f.get("--confirm") != Some(&"yes")
    {
        return Some(Output{stdout:"{\"ok\":false,\"errors\":[\"local start creates a workspace and company; pass --confirm yes to continue\"]}\n".into(),stderr:String::new(),exit:1});
    }
    let bin = match binary() {
        Ok(v) => v,
        Err(o) => return Some(o),
    };
    let mut command = Command::new(bin);
    command
        .arg("--original716-operator")
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    if args[0] == "local" {
        for (key, _) in std::env::vars() {
            if key.starts_with("RENTEMESTER_AUTH_")
                || key.starts_with("RENTEMESTER_DOCUMENT_SCANNER_")
                || matches!(
                    key.as_str(),
                    "RENTEMESTER_APP_AUTH"
                        | "RENTEMESTER_APP_TOKEN"
                        | "RENTEMESTER_DEPLOYMENT_PROFILE"
                )
            {
                command.env_remove(key);
            }
        }
    }
    Some(match command.status() {
        Ok(status) => Output {
            stdout: String::new(),
            stderr: String::new(),
            exit: status.code().unwrap_or(1),
        },
        Err(_) => usage(
            "native rentemester-server could not be launched; install the sibling binary or set explicit RENTEMESTER_SERVER_BIN (no fallback)",
        ),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }
    #[test]
    fn original716_literals_not_commands() {
        assert!(prepare(&args(&["--name", "init", "report", "analytics"])).is_none());
        assert_eq!(
            prepare(&args(&["--name=serve=a", "init", "--company=synthetic"])),
            Some(args(&["init", "--name=serve=a", "--company=synthetic"]))
        );
        assert_eq!(
            prepare(&args(&[
                "--workspace=/synthetic/a=b",
                "local",
                "--no-open",
                "start"
            ])),
            Some(args(&[
                "local",
                "start",
                "--workspace=/synthetic/a=b",
                "--no-open"
            ]))
        );
    }
    #[test]
    fn original716_invalid_flags_before_effects() {
        for a in [
            args(&["init", "--no-vat", "--vat-period", "month"]),
            args(&["serve", "--port=0"]),
            args(&["local", "start", "--port=4319"]),
            args(&["init", "--name=x", "--name=y"]),
        ] {
            assert_eq!(parse(&a, count(&a).unwrap()).err().unwrap().exit, 2);
        }
    }
}
