//! Offline static commands share the embedded pure engine corpus.
use rentemester_engine::{Error, regulatory};
use serde_json::{Value, json};
use std::collections::BTreeMap;
fn invalid(s: &str) -> (u8, Error) {
    (2, Error::new("invalid_input", s))
}
pub fn run(args: &[String]) -> Result<Value, (u8, Error)> {
    let command = args
        .get(1)
        .map(String::as_str)
        .ok_or_else(|| invalid("reg coverage|citations [--out PATH] [--json]"))?;
    if !matches!(command, "coverage" | "citations") {
        return Err(invalid("reg coverage|citations"));
    }
    let mut out = None;
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => i += 1,
            "--out" if out.is_none() => {
                out = Some(
                    args.get(i + 1)
                        .ok_or_else(|| invalid("--out requires path"))?,
                );
                i += 2;
            }
            _ => {
                return Err(invalid(
                    "static reg accepts only --out and --json; no company or login",
                ));
            }
        }
    }
    let coverage = regulatory::bundled_coverage().map_err(|e| (1, e))?;
    let report = if command == "coverage" {
        regulatory::render_coverage(&coverage)
    } else {
        regulatory::render_citations(&coverage)
    }
    .map_err(|e| (1, e))?;
    if let Some(path) = out {
        std::fs::write(path, &report)
            .map_err(|_| invalid("could not write requested regulatory report"))?;
    }
    Ok(
        json!({"ok":coverage.get("ok").and_then(Value::as_bool).unwrap_or(false),"report":report,"coverage":coverage}),
    )
}
pub fn compliance(
    flags: &BTreeMap<&str, &str>,
    allow: bool,
    confirm: bool,
    preview: bool,
) -> Result<Value, (u8, Error)> {
    if confirm
        || preview
        || flags
            .keys()
            .any(|k| !["--company", "--server", "--cookie-file", "--as-of", "--out"].contains(k))
    {
        return Err(invalid(
            "compliance-report requires company/current cookie; optional --as-of/--out only",
        ));
    }
    let company = flags
        .get("--company")
        .ok_or_else(|| invalid("--company required"))?;
    let server = flags
        .get("--server")
        .ok_or_else(|| invalid("--server required"))?;
    let cookie = flags
        .get("--cookie-file")
        .ok_or_else(|| invalid("--cookie-file required"))?;
    let cookie = rentemester_client::read_cookie_file(std::path::Path::new(cookie))
        .map_err(crate::remote::client_error)?;
    let api = rentemester_client::ApiClient::new(server, allow, Some(cookie))
        .map_err(crate::remote::client_error)?;
    let rt = tokio::runtime::Runtime::new().map_err(|_| invalid("HTTP runtime unavailable"))?;
    let result = rt
        .block_on(api.compliance_report(company, flags.get("--as-of").copied()))
        .map_err(crate::remote::client_error)?;
    if let Some(path) = flags.get("--out") {
        let html = result
            .pointer("/complianceReport/html")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("missing server HTML"))?;
        std::fs::write(path, html).map_err(|_| invalid("could not write report"))?;
    }
    Ok(json!({"ok":result.get("ok").and_then(Value::as_bool).unwrap_or(false),"data":result}))
}

/// The original CLI is an offline projection, not the native `run` envelope.
pub fn accepts(args: &[String]) -> bool {
    args.first().is_some_and(|s| s == "reg")
}
struct Options {
    command: String,
    out: Option<String>,
    json: bool,
    help: bool,
}
fn parse(args: &[String]) -> Result<Options, String> {
    let mut options = Options {
        command: String::new(),
        out: None,
        json: false,
        help: false,
    };
    let mut format = "human";
    let mut i = 1;
    while i < args.len() {
        let (name, attached) = crate::original_args::option(&args[i]);
        match name {
            "--json" | "--help" => {
                if attached.is_some() {
                    return Err(format!("Flag {name} does not take a value"));
                }
                if name == "--json" {
                    options.json = true;
                } else {
                    options.help = true;
                }
                i += 1;
            }
            "--out" | "--format" => {
                let (value, next) = crate::original_args::value(args, i)
                    .ok_or_else(|| format!("Flag {name} requires a value"))?;
                if name == "--out" {
                    options.out = Some(value.trim().to_owned()).filter(|s| !s.is_empty());
                } else if matches!(value, "json" | "human") {
                    format = value;
                } else {
                    return Err("--format must be either json or human".into());
                }
                i = next;
            }
            "coverage" | "citations" if options.command.is_empty() => {
                options.command = name.to_owned();
                i += 1;
            }
            _ => return Err(format!("Unknown argument: {}", args[i])),
        }
    }
    options.json |= format == "json";
    if options.command.is_empty() && !options.help {
        return Err("reg coverage|citations [--out <file.md>] [--format json|human]".into());
    }
    Ok(options)
}
pub fn help() -> Value {
    json!({"ok":true,"commands":["reg coverage [--out <file.md>] [--format json|human]","reg citations [--out <file.md>] [--format json|human]"],"flags":["--out","--format","--json","--help"],"offline":true})
}
fn diagnostic(message: &str, exit: u8) -> crate::dated_reports::Output {
    crate::dated_reports::Output {
        stdout: String::new(),
        stderr: format!("{message}\n"),
        exit,
    }
}
pub fn preflight(args: &[String]) -> Result<bool, crate::dated_reports::Output> {
    parse(args).map(|_| true).map_err(|e| diagnostic(&e, 2))
}
/// No runtime binding is needed or consulted for these embedded-corpus reports.
pub fn binding_refusal(args: &[String]) -> crate::dated_reports::Output {
    execute(args)
}
fn project_coverage(coverage: &Value, out: Option<&str>) -> Value {
    let count = |key: &str| coverage[key].as_array().unwrap().len();
    let mut result = json!({"ok":count("closureErrors") == 0 && count("driftErrors") == 0 && count("scopeErrors") == 0,
        "closureErrors":count("closureErrors"),"driftErrors":count("driftErrors"),"scopeErrors":count("scopeErrors"),"uncitedRules":count("uncitedRules")});
    for (public, native) in [
        ("inScopeOperativeProvisions", "inScopeOperativeCount"),
        ("inScopeCitedProvisions", "inScopeCitedCount"),
        ("operativeProvisions", "operativeCount"),
        ("citedProvisions", "citedCount"),
    ] {
        result[public] = coverage["overall"][native].clone();
    }
    result["perSource"] = coverage["perSource"].as_array().unwrap().iter().map(|source| json!({
        "sourceId":source["sourceId"],"inScopeOperativeProvisions":source["inScopeOperativeCount"],"inScopeCitedProvisions":source["inScopeCitedCount"],"operativeProvisions":source["operativeCount"],"citedProvisions":source["citedCount"]})).collect();
    if let Some(path) = out {
        result["reportPath"] = json!(path);
    }
    let text = |e: &Value, key: &str| e[key].as_str().unwrap_or("").to_owned();
    let mut errors = Vec::new();
    for (key, label, verb) in [
        ("closureErrors", "closure", "unresolved"),
        ("driftErrors", "drift", "stale"),
    ] {
        for e in coverage[key].as_array().unwrap() {
            errors.push(format!(
                "{label}: {} ({}) cites {verb} {}",
                text(e, "ruleId"),
                text(e, "sourceId"),
                text(e, "ref")
            ));
        }
    }
    for e in coverage["scopeErrors"].as_array().unwrap() {
        errors.push(match e["kind"].as_str() {
            Some("missing_source") => {
                format!("scope: {} missing from scope.yaml", text(e, "sourceId"))
            }
            Some("bad_endpoint") => format!(
                "scope: {} range endpoint § {} does not exist",
                text(e, "sourceId"),
                text(e, "paragraf")
            ),
            _ => format!(
                "scope: {} ({}) cites out-of-scope {}",
                text(e, "ruleId"),
                text(e, "sourceId"),
                text(e, "ref")
            ),
        });
    }
    if !errors.is_empty() {
        result["errors"] = json!(errors);
    }
    result
}
fn human_coverage(v: &Value) -> String {
    let n = |key: &str| v[key].as_u64().unwrap();
    let mut lines = vec![
        format!("{} reg coverage", if v["ok"] == true { "✔" } else { "✘" }),
        format!(
            "  In-scope provisions cited: {}/{}",
            n("inScopeCitedProvisions"),
            n("inScopeOperativeProvisions")
        ),
        format!(
            "  Corpus-wide (incl. out of scope): {}/{}",
            n("citedProvisions"),
            n("operativeProvisions")
        ),
        format!("  Closure errors: {}", n("closureErrors")),
        format!("  Drift errors: {}", n("driftErrors")),
        format!("  Scope errors: {}", n("scopeErrors")),
        format!("  Uncited rules: {}", n("uncitedRules")),
    ];
    if let Some(path) = v["reportPath"].as_str() {
        lines.push(format!("  Report: {path}"));
    }
    lines.push("  Per source (in-scope cited / in-scope operative):".into());
    for source in v["perSource"].as_array().unwrap() {
        lines.push(format!(
            "    {}: {}/{}",
            source["sourceId"].as_str().unwrap(),
            source["inScopeCitedProvisions"],
            source["inScopeOperativeProvisions"]
        ));
    }
    if let Some(errors) = v["errors"].as_array() {
        for error in errors {
            lines.push(format!("  → {}", error.as_str().unwrap()));
        }
    }
    format!("{}\n", lines.join("\n"))
}
pub fn execute(args: &[String]) -> crate::dated_reports::Output {
    let options = match parse(args) {
        Ok(v) => v,
        Err(e) => return diagnostic(&e, 2),
    };
    if options.help {
        return crate::dated_reports::Output {
            stdout: format!("{}\n", help()),
            stderr: String::new(),
            exit: 0,
        };
    }
    let coverage = match regulatory::bundled_coverage() {
        Ok(v) => v,
        Err(e) => return diagnostic(&e.message, 1),
    };
    // Additional native integrity checks are never silently discarded by the source projection.
    if coverage["sourceErrors"]
        .as_array()
        .is_none_or(|e| !e.is_empty())
    {
        return diagnostic("embedded regulatory source integrity check failed", 1);
    }
    let report = if options.command == "coverage" {
        regulatory::render_coverage(&coverage)
    } else {
        regulatory::render_citations(&coverage)
    };
    let report = match report {
        Ok(v) => v,
        Err(e) => return diagnostic(&e.message, 1),
    };
    if let Some(path) = &options.out {
        if let Err(e) = std::fs::write(path, &report) {
            return diagnostic(&format!("could not write regulatory report {path}: {e}"), 1);
        }
    }
    let result = if options.command == "coverage" {
        project_coverage(&coverage, options.out.as_deref())
    } else if let Some(path) = &options.out {
        json!({"ok":true,"reportPath":path})
    } else {
        json!({"ok":true,"review":report})
    };
    let ok = result["ok"] == true;
    let text = if options.json {
        format!("{result}\n")
    } else if options.command == "coverage" {
        human_coverage(&result)
    } else if let Some(path) = &options.out {
        format!("✔ reg citations\n  Review: {path}\n")
    } else {
        format!("{report}\n")
    };
    if !ok && !options.json {
        diagnostic(text.trim_end_matches('\n'), 1)
    } else {
        crate::dated_reports::Output {
            stdout: text,
            stderr: String::new(),
            exit: if ok { 0 } else { 1 },
        }
    }
}
