//! Original read-only report analytics ABI over the existing canonical CFO producer.
use crate::dated_reports::Output;
use rentemester_client::{ApiClient, CfoAnalyticsQuery, LocalRuntime};
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::IsTerminal, path::Path};
const REQUIRED: &str =
    "report analytics requires --workspace, --scope company|portfolio|group, --from and --to";
fn usage(s: &str) -> Output {
    Output {
        stdout: String::new(),
        stderr: format!("{s}\n"),
        exit: 2,
    }
}
pub fn help() -> Value {
    json!({"ok":true,"usage":"report analytics --workspace WORKSPACE --scope company|portfolio|group --from DATE --to DATE [--company-slug SLUG|--company-slugs a,b|--group-profile-id ID]","selectors":["--account","--party","--dimension","--currency","--aggregate none|sum","--cursor","--limit 1..200"],"connection":"Explicit verified --local-profile and --cookie-file. Workspace must exactly equal that profile's root or ID. Original remote workspace assertions are unavailable; native cfo-analytics remains remote-capable. No autostart, local ledger reads or client calculations.","output":"Source-flat {ok:true,analytics} or {ok:false,errors}; --json wins, otherwise last --format json|human and piped JSON/TTY human. Help is passive; current server credentials authorize this read."})
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
            }
        } else {
            words.push(i);
            i += 1;
        }
    }
    if words.len() < 2 || args[words[0]] != "report" || args[words[1]] != "analytics" {
        return None;
    }
    let mut out = vec!["report".into(), "analytics".into()];
    out.extend(
        args.iter()
            .enumerate()
            .filter(|(i, _)| !words[..2].contains(i))
            .map(|(_, s)| s.clone()),
    );
    Some(out)
}
fn limit(raw: &str) -> Option<u16> {
    let s = raw.trim();
    let n = if let Some(v) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(v, 16).ok()? as f64
    } else if let Some(v) = s.strip_prefix("0b").or_else(|| s.strip_prefix("0B")) {
        u64::from_str_radix(v, 2).ok()? as f64
    } else if let Some(v) = s.strip_prefix("0o").or_else(|| s.strip_prefix("0O")) {
        u64::from_str_radix(v, 8).ok()? as f64
    } else {
        s.parse::<f64>().ok()?
    };
    (n.is_finite() && n.fract() == 0. && (1. ..=200.).contains(&n)).then_some(n as u16)
}
fn parse(args: &[String]) -> Result<BTreeMap<&str, &str>, Output> {
    let mut f = BTreeMap::new();
    let mut i = 2;
    while i < args.len() {
        let (k, a) = crate::original_args::option(&args[i]);
        if ![
            "--workspace",
            "--scope",
            "--from",
            "--to",
            "--company-slug",
            "--company-slugs",
            "--group-profile-id",
            "--account",
            "--party",
            "--dimension",
            "--currency",
            "--aggregate",
            "--cursor",
            "--limit",
            "--local-profile",
            "--server",
            "--cookie-file",
            "--json",
            "--format",
            "--help",
            "--allow-loopback-http",
        ]
        .contains(&k)
        {
            return Err(usage(&format!("Unknown report analytics flag {k}")));
        }
        let (v, n) = if matches!(k, "--json" | "--help" | "--allow-loopback-http") {
            if a.is_some() {
                return Err(usage("Boolean flag takes no value"));
            }
            ("true", i + 1)
        } else {
            crate::original_args::value(args, i)
                .ok_or_else(|| usage(&format!("Missing value for {k}")))?
        };
        if f.contains_key(k) && !matches!(k, "--json" | "--format") {
            return Err(usage(&format!("Duplicate {k}")));
        }
        if k == "--format" && !matches!(v, "json" | "human") {
            return Err(usage("--format must be json or human"));
        }
        f.insert(k, v);
        i = n;
    }
    if f.contains_key("--help") {
        return Ok(f);
    }
    if ["--workspace", "--from", "--to"]
        .iter()
        .any(|k| f.get(k).is_none_or(|s| s.is_empty()))
        || f.get("--scope")
            .is_none_or(|s| !matches!(*s, "company" | "portfolio" | "group"))
    {
        return Err(usage(REQUIRED));
    }
    if f.get("--limit").is_some_and(|s| limit(s).is_none()) {
        return Err(usage("--limit must be an integer from 1 through 200"));
    }
    if f.get("--aggregate")
        .is_some_and(|s| !matches!(*s, "none" | "sum"))
    {
        return Err(usage("--aggregate must be none or sum"));
    }
    if f.contains_key("--local-profile")
        && (f.contains_key("--server") || f.contains_key("--allow-loopback-http"))
    {
        return Err(usage(
            "Explicit --local-profile conflicts with remote transport flags",
        ));
    }
    Ok(f)
}
fn output(v: Value, f: &BTreeMap<&str, &str>) -> Output {
    let json = f.contains_key("--json")
        || f.get("--format") == Some(&"json")
        || (f.get("--format") != Some(&"human") && !std::io::stdout().is_terminal());
    let ok = v["ok"] != false;
    if json {
        return Output {
            stdout: format!(
                "{}\n",
                serde_json::to_string_pretty(&v).expect("JSON value")
            ),
            stderr: String::new(),
            exit: if ok { 0 } else { 1 },
        };
    }
    if ok {
        Output {
            stdout: "✔ report-analytics\n".into(),
            stderr: String::new(),
            exit: 0,
        }
    } else {
        let mut text = "✘ report-analytics failed".to_owned();
        for e in v["errors"].as_array().into_iter().flatten() {
            text.push_str(&format!(
                "\n  → {}",
                e.as_str().unwrap_or("Analytics unavailable")
            ));
        }
        Output {
            stdout: String::new(),
            stderr: format!("{text}\n"),
            exit: 1,
        }
    }
}
fn fail(s: impl ToString, f: &BTreeMap<&str, &str>) -> Output {
    output(json!({"ok":false,"errors":[s.to_string()]}), f)
}
pub fn execute(args: &[String]) -> Output {
    let f = match parse(args) {
        Ok(f) => f,
        Err(o) => return o,
    };
    if f.contains_key("--help") {
        return Output {
            stdout: format!("{}\n", help()),
            stderr: String::new(),
            exit: 0,
        };
    }
    let Some(profile) = f.get("--local-profile") else {
        return fail(
            "Original analytics workspace requires explicit verified --local-profile; authenticated remote workspace identity is unavailable. Native cfo-analytics supports remote queries.",
            &f,
        );
    };
    if !Path::new(profile).is_absolute() {
        return usage("--local-profile requires an absolute private profile path");
    }
    let local = match LocalRuntime::load(Path::new(profile)) {
        Ok(v) => v,
        Err(e) => return fail(e, &f),
    };
    let Some(cookie) = f.get("--cookie-file") else {
        return usage("Current --cookie-file required");
    };
    let cookie = match rentemester_client::read_cookie_file(Path::new(cookie)) {
        Ok(v) => v,
        Err(e) => return fail(e, &f),
    };
    let api = match ApiClient::new_local(&local, Some(cookie)) {
        Ok(v) => v,
        Err(e) => return fail(e, &f),
    };
    if let Err(e) = api.assert_original_workspace89(f["--workspace"]) {
        return fail(e, &f);
    }
    let normal = |k| {
        f.get(k)
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    let scope = f["--scope"];
    let companies = if scope == "company" {
        vec![normal("--company-slug").unwrap_or_default()]
    } else {
        let mut v: Vec<String> = f
            .get("--company-slugs")
            .map(|s| {
                s.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        v.sort();
        v.dedup();
        v
    };
    let q = CfoAnalyticsQuery {
        scope: scope.into(),
        from: f["--from"].into(),
        to: f["--to"].into(),
        company_slugs: companies,
        group_profile_id: if scope == "group" {
            f.get("--group-profile-id").map(|s| s.to_string())
        } else {
            None
        },
        account: normal("--account"),
        party: normal("--party"),
        dimension: normal("--dimension"),
        currency: normal("--currency").map(|s| s.to_uppercase()),
        aggregate: f.get("--aggregate").map(|s| s.to_string()),
        cursor: f
            .get("--cursor")
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string()),
        limit: f.get("--limit").and_then(|s| limit(s)),
    };
    let rt = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(_) => return fail("HTTP runtime unavailable", &f),
    };
    let mut body = match rt.block_on(api.cfo_analytics(&q)) {
        Ok(v) => v,
        Err(e) => return fail(e, &f),
    };
    if body["ok"] == false {
        let errors = body["errors"]
            .as_array()
            .filter(|a| !a.is_empty() && a.iter().all(Value::is_string))
            .cloned()
            .unwrap_or_else(|| {
                vec![json!(
                    body["code"].as_str().unwrap_or("Analytics query rejected")
                )]
            });
        return output(json!({"ok":false,"errors":errors}), &f);
    }
    if body["ok"] != true
        || body["schemaVersion"] != "rentemester-cfo-analytics-v1"
        || body["scope"] != scope
        || (scope == "group" && !body["group"].is_object())
        || (scope != "group"
            && (!body["rows"].is_array()
                || !body["page"].is_object()
                || !body["reconciliation"].is_object()))
    {
        return fail("Malformed canonical CFO result", &f);
    }
    body.as_object_mut().expect("validated object").remove("ok");
    output(json!({"ok":true,"analytics":body}), &f)
}
