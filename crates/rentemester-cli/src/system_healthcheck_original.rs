//! Original local healthcheck: deliberately bypasses all transport/profile binding.
use crate::dated_reports::Output;
use std::{collections::BTreeMap, path::Path};
pub fn accepts(args: &[String]) -> bool {
    args.first().is_some_and(|s| s == "system") && args.get(1).is_some_and(|s| s == "healthcheck")
}
fn usage() -> Output {
    Output {
        stdout: String::new(),
        stderr:
            "Usage: system healthcheck --company PATH [--json | --format json|human]; see --help\n"
                .into(),
        exit: 2,
    }
}
pub fn execute(args: &[String]) -> Output {
    let mut flags = BTreeMap::new();
    let mut cursor = 2;
    while cursor < args.len() {
        let (key, attached) = crate::original_args::option(&args[cursor]);
        if !matches!(key, "--company" | "--json" | "--format" | "--help") {
            return usage();
        }
        let (value, next) = if matches!(key, "--json" | "--help") {
            if attached.is_some() {
                return usage();
            }
            ("true", cursor + 1)
        } else {
            match crate::original_args::value(args, cursor) {
                Some((v, n)) if !v.is_empty() => (v, n),
                _ => return usage(),
            }
        };
        if flags.insert(key, value).is_some() {
            return usage();
        }
        cursor = next;
    }
    if flags
        .get("--format")
        .is_some_and(|v| !matches!(*v, "json" | "human"))
    {
        return usage();
    }
    if flags.contains_key("--help") {
        return Output { stdout: "system healthcheck --company PATH [--json | --format json|human]\nOffline read-only company diagnostic: company_root, data_dir, ledger, documents, config and protected snapshot schema inspection. Explicit company path required; no environment/default company. No server, cookie, local-profile, actor, stdin, repair or mutation inputs. Remote health is aggregate-only and cannot substitute this report. Human diagnostics (including failures) go to stdout; explicit JSON is pretty-printed, flat and unwrapped. Exit 0 clean, 1 diagnostic failure, 2 usage.\n".into(), stderr: String::new(), exit: 0 };
    }
    let Some(company) = flags.get("--company") else {
        return usage();
    };
    let report = rentemester_storage::healthcheck::inspect_company(Path::new(company));
    let exit = if report["ok"] == true { 0 } else { 1 };
    let stdout = if flags.contains_key("--json") || flags.get("--format") == Some(&"json") {
        format!("{}\n", serde_json::to_string_pretty(&report).unwrap())
    } else {
        let mut lines = String::new();
        for check in report["checks"].as_array().unwrap() {
            lines.push_str(&format!(
                "{} {}\n",
                if check["ok"] == true { "OK" } else { "FAIL" },
                check["name"].as_str().unwrap()
            ));
        }
        if let Some(schema) = report.get("schema") {
            lines.push_str(&format!(
                "{} schema_{} current={} required={}\n",
                if schema["status"] == "current" {
                    "OK"
                } else {
                    "FAIL"
                },
                schema["status"].as_str().unwrap(),
                schema["currentVersion"],
                schema["requiredVersion"]
            ));
        }
        lines
    };
    Output {
        stdout,
        stderr: String::new(),
        exit,
    }
}
