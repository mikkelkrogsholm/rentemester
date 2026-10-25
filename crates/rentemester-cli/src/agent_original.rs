//! Local packaging and presentation only. All seven phases execute in the common server.
use crate::dated_reports::Output;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::IsTerminal, path::Path};
pub fn help() -> Value {
    json!({"ok":true,"usage":"agent run --company SLUG --as-of YYYY-MM-DD [--inbox DIR] [--metadata-dir DIR] [--bank-csv FILE] [--json | --format json|human]","connection":"--server ORIGIN or --local-profile ABS_PRIVATE_PROFILE; --cookie-file PATH or --service-key-file PATH; --allow-loopback-http. RENTEMESTER_COMPANY fallback. Optional --actor is an assertion of current credential only.","authority":"Explicit invocation confirms the full original loop. Native actor is the actual authenticated user/service principal; report.company is the canonical selected company slug, not a server private path. Program rentemester-runtime-agent.","recovery":"One POST, no automatic retry. Transport/lost reply means outcome unknown; reconcile canonical journal/bank/documents/exceptions before another invocation.","phases":["ingest","book","route","payables","reconcile","deadlines","report"]})
}
fn usage(s: &str) -> Output {
    Output {
        stdout: String::new(),
        stderr: format!("{s}\n"),
        exit: 2,
    }
}
/// Normalize command words around ordinary global options without touching their values.
pub fn prepare(args: &[String]) -> Option<Vec<String>> {
    let mut words = vec![];
    let mut i = 0;
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
    if words.first().is_none_or(|i| args[*i] != "agent") {
        return None;
    }
    let n = if words.get(1).is_some_and(|i| args[*i] == "run") {
        2
    } else {
        1
    };
    let mut out = vec!["agent".into()];
    if n == 2 {
        out.push("run".into());
    }
    out.extend(
        args.iter()
            .enumerate()
            .filter(|(i, _)| !words[..n].contains(i))
            .map(|(_, s)| s.clone()),
    );
    Some(out)
}
fn parse(args: &[String]) -> Result<BTreeMap<&str, &str>, Output> {
    if args.get(1).is_none_or(|s| s != "run") {
        return Err(usage("Expected agent run; see agent --help"));
    }
    let mut f = BTreeMap::new();
    let mut i = 2;
    while i < args.len() {
        let (k, a) = crate::original_args::option(&args[i]);
        if !matches!(
            k,
            "--company"
                | "--as-of"
                | "--inbox"
                | "--metadata-dir"
                | "--bank-csv"
                | "--server"
                | "--cookie-file"
                | "--service-key-file"
                | "--local-profile"
                | "--actor"
                | "--json"
                | "--format"
                | "--help"
                | "--allow-loopback-http"
        ) {
            return Err(usage(&format!("Unknown agent flag {k}")));
        }
        let (v, n) = if matches!(k, "--json" | "--help" | "--allow-loopback-http") {
            if a.is_some() {
                return Err(usage("Boolean flag takes no value"));
            }
            ("true", i + 1)
        } else {
            crate::original_args::value(args, i).ok_or_else(|| usage("Missing flag value"))?
        };
        if v.is_empty() || (f.contains_key(k) && k != "--format" && k != "--json") {
            return Err(usage("Empty or duplicate agent flag"));
        }
        if k == "--format" && !matches!(v, "json" | "human") {
            return Err(usage("--format must be json or human"));
        }
        f.insert(k, v);
        i = n;
    }
    if f.contains_key("--cookie-file") && f.contains_key("--service-key-file") {
        return Err(usage("Choose exactly one --cookie-file/--service-key-file"));
    }
    if f.contains_key("--local-profile")
        && (f.contains_key("--server") || f.contains_key("--allow-loopback-http"))
    {
        return Err(usage(
            "--local-profile conflicts with --server/--allow-loopback-http",
        ));
    }
    if f.get("--local-profile")
        .is_some_and(|p| !Path::new(p).is_absolute())
    {
        return Err(usage("--local-profile requires absolute path"));
    }
    if f.get("--actor").is_some_and(|s| {
        s.trim() != *s || s.chars().any(char::is_control) || s.encode_utf16().count() > 160
    }) {
        return Err(usage("--actor is a trimmed current-identity assertion"));
    }
    if !f.contains_key("--help") && !f.contains_key("--as-of") {
        return Err(usage("Missing required --as-of <YYYY-MM-DD>"));
    }
    Ok(f)
}
/// Preflight is argv-only and runs before any credentials or local files are opened.
pub fn preflight(args: &[String]) -> Result<(), Output> {
    parse(args).map(|_| ())
}
fn failed(error: String) -> Value {
    json!({"state":"failed","error":error})
}
/// Whole listing and all sidecars are parsed before the single server request.
pub fn package_inbox(inbox: &Path, metadata: Option<&Path>) -> Value {
    let load = || -> Result<Value, String> {
        let entries = std::fs::read_dir(inbox)
            .map_err(|_| format!("inbox directory does not exist: {}", inbox.display()))?;
        let mut names = vec![];
        for e in entries {
            let e = e.map_err(|_| "could not read inbox directory".to_string())?;
            let name = e
                .file_name()
                .into_string()
                .map_err(|_| "inbox filename is not UTF-8".to_string())?;
            names.push(name);
        }
        names.sort();
        let mut items = vec![];
        for name in names {
            if name.starts_with('.') {
                continue;
            }
            let path = Path::new(&name);
            let ext = path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if !matches!(ext.as_str(), "txt" | "pdf" | "png" | "jpg" | "jpeg") {
                continue;
            }
            let sidecar = metadata.unwrap_or(inbox).join(format!(
                "{}.json",
                path.file_stem().unwrap().to_string_lossy()
            ));
            let raw = std::fs::read(&sidecar).map_err(|_| {
                format!(
                    "bilag {name} has no metadata sibling at {}",
                    sidecar.display()
                )
            })?;
            let value: Value = serde_json::from_slice(&raw)
                .map_err(|_| format!("invalid metadata JSON for bilag {name}"))?;
            let bytes = std::fs::read(inbox.join(&name))
                .map_err(|_| format!("could not read bilag {name}"))?;
            items.push(json!({"fileName":name,"fileBase64":base64(&bytes),"metadata":value}));
        }
        Ok(json!({"state":"files","items":items}))
    };
    load().unwrap_or_else(failed)
}
fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in bytes.chunks(3) {
        let a = c[0] as usize;
        let b = c.get(1).copied().unwrap_or(0) as usize;
        let d = c.get(2).copied().unwrap_or(0) as usize;
        out.push(ABC[a >> 2] as char);
        out.push(ABC[((a & 3) << 4) | (b >> 4)] as char);
        out.push(if c.len() > 1 {
            ABC[((b & 15) << 2) | (d >> 6)] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            ABC[d & 63] as char
        } else {
            '='
        });
    }
    out
}
pub fn execute(args: &[String]) -> Output {
    if args == ["agent"] || args == ["agent", "--help"] {
        return Output {
            stdout: format!("{}\n", help()),
            stderr: String::new(),
            exit: 0,
        };
    }
    if let Err(o) = preflight(args) {
        return o;
    }
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
    let company = f.get("--company").map(|s| s.to_string()).or_else(|| {
        std::env::var("RENTEMESTER_COMPANY")
            .ok()
            .filter(|s| !s.trim().is_empty())
    });
    let Some(company) = company else {
        return usage("Missing required --company <slug>");
    };
    let mut owned_server = None;
    let mut company = company;
    if let Some(profile) = f.get("--local-profile") {
        let local = match rentemester_client::LocalRuntime::load(Path::new(profile)) {
            Ok(l) => l,
            Err(e) => return usage(&e.to_string()),
        };
        company = match rentemester_client::knowledge_company_selector(Some(&local), &company) {
            Ok(c) => c,
            Err(e) => return usage(&e.to_string()),
        };
        owned_server = Some(local.origin().to_owned());
        if let Err(e) = local.install_process() {
            return usage(&e.to_string());
        }
    }
    let Some(server) = owned_server
        .as_deref()
        .or_else(|| f.get("--server").copied())
    else {
        return usage("--server or --local-profile required");
    };
    if !f.contains_key("--cookie-file") && !f.contains_key("--service-key-file") {
        return usage("--cookie-file or --service-key-file required");
    }
    let api = match crate::remote::credential_client(
        server,
        owned_server.is_some() || f.contains_key("--allow-loopback-http"),
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
    let mut request = json!({"asOf":f["--as-of"],"confirm":true});
    if let Some(a) = f.get("--actor") {
        request["expectedActor"] = json!(a);
    }
    if let Some(dir) = f.get("--inbox") {
        request["inbox"] = package_inbox(Path::new(dir), f.get("--metadata-dir").map(Path::new));
    }
    // A bank read failure travels to the coordinator AFTER packaged intake, never an early return.
    if let Some(path) = f.get("--bank-csv") {
        request["bank"] = match std::fs::read_to_string(path) {
            Ok(csv) => {
                json!({"state":"csv","fileName":Path::new(path).file_name().unwrap_or_default().to_string_lossy(),"csv":csv})
            }
            Err(_) => failed(format!(
                "bank CSV does not exist or is not readable UTF-8: {path}"
            )),
        };
    }
    let rt = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(_) => {
            return Output {
                stdout: String::new(),
                stderr: "HTTP runtime unavailable\n".into(),
                exit: 1,
            };
        }
    };
    match rt.block_on(api.original_agent_run(&company, &request)) {
        Ok(v) => {
            let exit = if v["ok"] == true { 0 } else { 1 };
            let json = f.contains_key("--json")
                || f.get("--format") == Some(&"json")
                || (f.get("--format") != Some(&"human") && !std::io::stdout().is_terminal());
            Output {
                stdout: format!(
                    "{}\n",
                    if json {
                        v.to_string()
                    } else {
                        format_report(&v)
                    }
                ),
                stderr: String::new(),
                exit,
            }
        }
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
fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v[k].as_str().unwrap_or("")
}
fn amount(v: &Value, ccy: &str) -> String {
    let n = v.as_f64().unwrap_or(0.);
    let raw = format!("{:.2}", n.abs());
    let (whole, decimal) = raw.split_once('.').unwrap();
    let mut grouped = String::new();
    for (i, c) in whole.chars().enumerate() {
        if i > 0 && (whole.len() - i) % 3 == 0 {
            grouped.push('.');
        }
        grouped.push(c);
    }
    let c = ccy.trim().to_uppercase();
    format!(
        "{}{grouped},{decimal} {}",
        if n < 0. { "-" } else { "" },
        if c == "DKK" { "kr." } else { &c }
    )
}
/// Pure source-compatible human renderer. Input must be a validated flat Report.
pub fn format_report(v: &Value) -> String {
    let mut lines = vec![
        "Rentemester runtime-agent — kørsel".into(),
        "=".repeat(34),
        "regel:    DK-RUNTIME-AGENT-001".into(),
        format!("actor:    {}", s(v, "actor")),
        format!("as-of:    {}", s(v, "asOf")),
        format!("company:  {}", s(v, "company")),
        format!(
            "faser:    {}",
            v["phases"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| s.as_str().unwrap())
                .collect::<Vec<_>>()
                .join(" → ")
        ),
    ];
    if v["ok"] == false {
        lines.extend([String::new(), "KØRSEL FEJLEDE:".into()]);
        for e in v["errors"].as_array().unwrap() {
            lines.push(format!("  ✗ {}", e.as_str().unwrap()));
        }
        return lines.join("\n");
    }
    for (key, heading) in [
        ("expensesBooked", "— Bogført automatisk —"),
        ("payablesMatched", "— Kreditorposter afregnet automatisk —"),
        ("openExceptions", "— Exception-kø (kræver et menneske) —"),
        ("upcomingDeadlines", "— Kommende deadlines —"),
    ] {
        lines.extend([String::new(), heading.into()]);
        let rows = v[key].as_array().unwrap();
        if rows.is_empty() {
            lines.push(
                match key {
                    "openExceptions" => "  ✓ ingen åbne exceptions",
                    "upcomingDeadlines" => "  ✓ ingen deadlines inden for horisonten",
                    _ => "  (ingen)",
                }
                .into(),
            );
        }
        for r in rows {
            let entry = if s(r, "journalEntryNo").is_empty() {
                String::new()
            } else {
                format!(" [{}]", s(r, "journalEntryNo"))
            };
            match key {
                "expensesBooked" => lines.push(format!(
                    "  ✓ {} {} → konto {} ({}, moms={}){entry}",
                    s(r, "supplier"),
                    amount(&r["amount"], s(r, "currency")),
                    s(r, "expenseAccount"),
                    s(r, "label"),
                    s(r, "vatTreatment")
                )),
                "payablesMatched" => lines.push(format!(
                    "  ✓ {} {} → kreditorpost #{}{entry}",
                    s(r, "supplier"),
                    amount(&r["amount"], "DKK"),
                    r["payableId"]
                )),
                "openExceptions" => {
                    lines.push(format!(
                        "  ! [{}] #{} {}: {}",
                        s(r, "severity"),
                        r["exceptionId"],
                        s(r, "type"),
                        s(r, "message")
                    ));
                    if !s(r, "requiredAction").is_empty() {
                        lines.push(format!("      → {}", s(r, "requiredAction")));
                    }
                }
                _ => {
                    lines.push(format!(
                        "  {} {} {}..{} — forfalder {} ({} dage)",
                        if r["ready"] == true { "✓" } else { "!" },
                        if r["kind"] == "vat_period" {
                            "Momsangivelse"
                        } else {
                            "Årsrapport"
                        },
                        s(r, "periodStart"),
                        s(r, "periodEnd"),
                        s(r, "dueDate"),
                        r["daysRemaining"]
                    ));
                    lines.push(format!("      {}", s(r, "note")));
                }
            }
        }
    }
    lines.extend([String::new(), "=== Kørsel afsluttet ===".into()]);
    for s in v["summary"].as_array().unwrap() {
        lines.push(format!("  • {}", s.as_str().unwrap()));
    }
    lines.join("\n")
}
