//! Source operator family. Only implemented keys are claimed; no registered-name parity.
use crate::dated_reports::Output;
use rentemester_client::LocalRuntime;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::IsTerminal, path::Path};
fn usage(message: &str) -> Output {
    Output { stdout: String::new(), stderr: format!("{message}\n"), exit: 2 }
}
/// Normalize command words only. Attached values and consumed separated literals
/// are never searched for command names, split again, trimmed or reordered.
pub fn prepare(args: &[String]) -> Option<Vec<String>> {
    let mut words = vec![];
    let mut i = 0;
    while i < args.len() {
        let (key, attached) = crate::original_args::option(&args[i]);
        if key.starts_with("--") {
            i = if attached.is_some() || matches!(key, "--json" | "--help" | "--allow-loopback-http") {
                i + 1
            } else {
                crate::original_args::value(args, i).map_or(i + 1, |(_, next)| next)
            };
        } else { words.push(i); i += 1; }
    }
    if words.len() < 2 || args[words[0]] != "group" || args[words[1]] != "overview" { return None; }
    let mut out = vec!["group".into(), "overview".into()];
    out.extend(args.iter().enumerate().filter(|(i, _)| !words[..2].contains(i)).map(|(_, v)| v.clone()));
    Some(out)
}
fn parse(args: &[String]) -> Result<BTreeMap<&str, &str>, Output> {
    let mut flags = BTreeMap::new();
    let mut i = 2;
    while i < args.len() {
        let (key, attached) = crate::original_args::option(&args[i]);
        if !matches!(key, "--workspace" | "--as-of" | "--company" | "--server" | "--local-profile" | "--cookie-file" | "--json" | "--format" | "--help" | "--allow-loopback-http") {
            return Err(usage(&format!("Unknown group overview flag {key}")));
        }
        let (value, next) = if matches!(key, "--json" | "--help" | "--allow-loopback-http") {
            if attached.is_some() { return Err(usage("Boolean flag takes no value")); }
            ("true", i + 1)
        } else { crate::original_args::value(args, i).ok_or_else(|| usage("Missing flag value"))? };
        if flags.insert(key, value).is_some() && key != "--format" && key != "--json" {
            return Err(usage("Duplicate flag"));
        }
        if key == "--format" && !matches!(value, "json" | "human") { return Err(usage("--format must be either json or human")); }
        i = next;
    }
    if flags.contains_key("--local-profile") && (flags.contains_key("--server") || flags.contains_key("--allow-loopback-http")) {
        return Err(usage("--local-profile conflicts with --server/--allow-loopback-http"));
    }
    if !flags.contains_key("--help") {
        if !flags.contains_key("--as-of") { return Err(usage("--as-of is required")); }
        if rentemester_engine::fiscal_year::Date::parse(flags["--as-of"]).is_err() { return Err(usage("asOf must be a real ISO date")); }
    }
    Ok(flags)
}
fn emit(value: Value, flags: &BTreeMap<&str, &str>) -> Output {
    let ok = value["ok"] == true;
    let machine = flags.contains_key("--json") || flags.get("--format") == Some(&"json")
        || !flags.contains_key("--format") && !std::io::stdout().is_terminal();
    if machine {
        return Output { stdout: format!("{}\n", serde_json::to_string_pretty(&value).unwrap()), stderr: String::new(), exit: if ok { 0 } else { 1 } };
    }
    if ok {
        Output { stdout: "✔ Viser kun datoafgrænset koncernstruktur og parathed; inge…\n".into(), stderr:String::new(), exit:0 }
    } else {
        let errors = value["errors"].as_array().map(|a| a.iter().filter_map(Value::as_str).map(|s| format!("  → {s}\n")).collect::<String>()).unwrap_or_default();
        Output { stdout:String::new(), stderr:format!("✘ Viser kun datoafgrænset koncernstruktur og parathed; ingen konsoliderede tal. failed\n{errors}"), exit:1 }
    }
}
/// Positive source group overview. All other original operator keys remain unclaimed
/// until their source artifact/custody adapters exist; never substitute native backups.
pub async fn execute(args: &[String]) -> Option<Output> {
    if args.first().map(String::as_str) != Some("group") || args.get(1).map(String::as_str) != Some("overview") { return None; }
    Some(execute_overview(args).await)
}
async fn execute_overview(args: &[String]) -> Output {
    let flags = match parse(args) { Ok(v) => v, Err(o) => return o };
    if flags.contains_key("--help") {
        return Output { stdout:format!("{}\n", json!({"usage":"group overview --workspace <dir> --as-of <YYYY-MM-DD>","connection":"Explicit --local-profile and --cookie-file; --workspace exactly asserts its root. Remote --server and --cookie-file may omit the local path assertion. Current credential determines visibility; no consolidated figures."})), stderr:String::new(), exit:0 };
    }
    let local_origin;
    let server = if let Some(profile) = flags.get("--local-profile") {
        let local = match LocalRuntime::load(Path::new(profile)) { Ok(v) => v, Err(e) => return usage(&e.to_string()) };
        let Some(workspace) = flags.get("--workspace") else { return usage("--workspace is required"); };
        if local.workspace_root().to_str() != Some(*workspace) { return usage("workspace differs from explicit verified local runtime root"); }
        local_origin = local.origin().to_owned();
        if let Some(company) = flags.get("--company") {
            if let Err(e) = rentemester_client::knowledge_company_selector(Some(&local), company) { return usage(&e.to_string()); }
        }
        if let Err(e) = local.install_process() { return usage(&e.to_string()); }
        local_origin.as_str()
    } else {
        if flags.contains_key("--workspace") { return usage("--workspace requires an explicit verified --local-profile; no remote path access"); }
        if flags.get("--company").is_some_and(|v| Path::new(v).is_absolute()) { return usage("remote company path is not permitted"); }
        match flags.get("--server") { Some(v) => v, None => return usage("--server or --local-profile required") }
    };
    let client = match crate::remote::credential_client(server, flags.contains_key("--local-profile") || flags.contains_key("--allow-loopback-http"), &flags, false) {
        Ok(v) => v,
        Err((exit, e)) => return Output { stdout:String::new(), stderr:format!("{}: {}\n", e.code, e.message), exit },
    };
    let value = match client.group_overview_original716(flags["--as-of"]).await {
        Ok(v) => v,
        Err(e) => json!({"ok":false,"errors":[e.to_string()]}),
    };
    emit(value, &flags)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn args(a: &[&str]) -> Vec<String> { a.iter().map(|s| s.to_string()).collect() }
    #[test]
    fn normalize_only_words_and_own_only_overview() {
        assert_eq!(prepare(&args(&["--workspace=/synthetic/a=b", "group", "--as-of", "2026-01-01", "overview"])), Some(args(&["group", "overview", "--workspace=/synthetic/a=b", "--as-of", "2026-01-01"])));
        assert!(prepare(&args(&["--workspace=group", "report", "analytics"])).is_none());
        assert!(prepare(&args(&["group", "reconcile"])).is_none());
        assert!(prepare(&args(&["--workspace", "group", "overview"])).is_none());
    }
}
