//! Original schema CLI. Inspections use protected disposable snapshots; apply uses
//! only the explicitly configured, stopped common server's policy and namespace lease.
use crate::dated_reports::Output;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::IsTerminal, path::Path};
fn owns(a: &str, b: &str) -> bool {
    a == "system" && matches!(b, "migrate" | "repair-schema-views")
}
fn usage(message: &str) -> Output {
    Output {
        stdout: String::new(),
        stderr: format!("{message}\n"),
        exit: 2,
    }
}
pub fn prepare(args: &[String]) -> Option<Vec<String>> {
    let mut words = vec![];
    let mut i = 0;
    while i < args.len() {
        let (k, a) = crate::original_args::option(&args[i]);
        if k == "--" {
            break;
        }
        if k.starts_with("--") {
            i = if a.is_some() || matches!(k, "--json" | "--help") {
                i + 1
            } else {
                crate::original_args::value(args, i).map_or(i + 1, |(_, n)| n)
            };
        } else {
            words.push(i);
            i += 1;
        }
    }
    if words.len() < 2 || !owns(&args[words[0]], &args[words[1]]) {
        return None;
    }
    let mut out = vec![args[words[0]].clone(), args[words[1]].clone()];
    out.extend(
        args.iter()
            .enumerate()
            .filter(|(i, _)| !words[..2].contains(i))
            .map(|(_, v)| v.clone()),
    );
    Some(out)
}
fn parse(args: &[String]) -> Result<BTreeMap<&str, &str>, Output> {
    let mut f = BTreeMap::new();
    let mut i = 2;
    while i < args.len() {
        let (k, a) = crate::original_args::option(&args[i]);
        if !matches!(
            k,
            "--company"
                | "--apply"
                | "--reason"
                | "--server-bin"
                | "--config"
                | "--json"
                | "--format"
                | "--help"
        ) || (args[1] == "migrate" && k == "--reason")
        {
            return Err(usage(&format!("Unknown flag {k}")));
        }
        let (v, n) = if matches!(k, "--json" | "--help") {
            if a.is_some() {
                return Err(usage("Boolean flag takes no value"));
            }
            ("true", i + 1)
        } else {
            crate::original_args::value(args, i).ok_or_else(|| usage("Missing flag value"))?
        };
        if f.insert(k, v).is_some() {
            return Err(usage("Duplicate flag"));
        }
        i = n;
    }
    if f.get("--apply").is_some_and(|v| *v != "yes") {
        return Err(usage("--apply must be exactly yes"));
    }
    if f.get("--format")
        .is_some_and(|v| !matches!(*v, "json" | "human"))
    {
        return Err(usage("--format must be json or human"));
    }
    if args[1] == "repair-schema-views"
        && f.contains_key("--apply")
        && f.get("--reason")
            .is_none_or(|v| v.trim().is_empty() || v.trim().chars().count() > 1000)
    {
        return Err(usage(
            "--reason is required and must contain 1 through 1000 characters",
        ));
    }
    Ok(f)
}
fn emit(value: Value, repair: bool, f: &BTreeMap<&str, &str>) -> Output {
    let ok = value["ok"] == true;
    let machine = f.contains_key("--json")
        || f.get("--format") == Some(&"json")
        || !f.contains_key("--format") && !std::io::stdout().is_terminal();
    let label = if repair {
        "Viser eller reparerer drift i kanoniske SQL-views"
    } else {
        "Viser skemastatus read-only; kun --apply yes udfører en l…"
    };
    if machine {
        return Output {
            stdout: format!("{}\n", serde_json::to_string_pretty(&value).unwrap()),
            stderr: String::new(),
            exit: if ok { 0 } else { 1 },
        };
    }
    if ok {
        Output {
            stdout: format!("✔ {label}\n"),
            stderr: String::new(),
            exit: 0,
        }
    } else {
        Output {
            stdout: String::new(),
            stderr: format!(
                "✘ {label} failed\n{}",
                value["errors"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(|e| format!("  → {e}\n"))
                    .collect::<String>()
            ),
            exit: 1,
        }
    }
}
pub async fn execute(args: &[String]) -> Option<Output> {
    if !owns(args.first()?.as_str(), args.get(1)?.as_str()) {
        return None;
    }
    let f = match parse(args) {
        Ok(v) => v,
        Err(o) => return Some(o),
    };
    let repair = args[1] == "repair-schema-views";
    if f.contains_key("--help") {
        return Some(Output {
            stdout: format!(
                "system {} --company <slug|path> [--apply yes]{}\nInspect is read-only using a protected snapshot. Apply requires --server-bin /absolute/rentemester-server --config /absolute/PRIVATE.json; stop the server first. The private config chooses company and allowlisted operator; --company is not write authority. No HTTP schema route, implicit upgrade, actor override or retry.\n",
                args[1],
                if repair { " --reason <text>" } else { "" }
            ),
            stderr: String::new(),
            exit: 0,
        });
    }
    let value = if f.contains_key("--apply") {
        // Config is authoritative; --company is an exact assertion checked by the server.
        let Some(company) = f.get("--company") else {
            return Some(usage("Missing required --company <slug|path>"));
        };
        let (Some(binary), Some(config)) = (f.get("--server-bin"), f.get("--config")) else {
            return Some(usage(
                "apply requires --server-bin and --config for the stopped common server",
            ));
        };
        rentemester_client::original716::schema::execute_private_schema(
            Path::new(binary),
            Path::new(config),
            company,
            repair,
            true,
            f.get("--reason").copied(),
        )
        .unwrap_or_else(|e| json!({"ok":false,"errors":[e]}))
    } else {
        if f.contains_key("--server-bin") || f.contains_key("--config") {
            return Some(usage("inspect requires only an explicit --company path"));
        }
        let Some(company) = f.get("--company") else {
            return Some(usage("Missing required --company <path>"));
        };
        rentemester_storage::original716::schema::inspect_company(Path::new(company), repair)
            .unwrap_or_else(|e| json!({"ok":false,"errors":[format!("{}: {}",e.code,e.message)]}))
    };
    Some(emit(value, repair, &f))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }
    #[test]
    fn original716_literal_prepare_and_invalid_apply() {
        assert_eq!(
            prepare(&args(&["--company=/synthetic/a=b", "system", "migrate"])),
            Some(args(&["system", "migrate", "--company=/synthetic/a=b"]))
        );
        assert!(prepare(&args(&["--company", "system", "migrate"])).is_none());
        assert!(parse(&args(&["system", "migrate", "--apply", "Yes"])).is_err());
        assert!(
            parse(&args(&[
                "system",
                "repair-schema-views",
                "--apply",
                "yes",
                "--reason",
                " "
            ]))
            .is_err()
        );
    }
}
