//! Original import adapters. Parsing and all accounting stay on the common server.
use crate::dated_reports::Output;
use rentemester_client::{ApiClient, Error};
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
#[path = "dinero_import/local.rs"]
mod original_source;
const LIMIT: usize = 12 * 1024 * 1024;
const RUN_LABEL: &str = "Migrerer en virksomhed fra et andet bogføringssystem ind i Rentemester. Parser eksportfilen med den valgte per-system-parser og bogfører resultatet som virksomhedens primobalance (#179). Idempotent: præcis én import/primobalance pr. virksomhed.";
const RUN_EXAMPLE: &str = include_str!("../../../examples/import-synthetic.csv");
fn usage(s: impl AsRef<str>) -> Output {
    Output {
        stdout: String::new(),
        stderr: format!("{}\n", s.as_ref()),
        exit: 2,
    }
}
pub fn accepts(a: &[String]) -> bool {
    a.first().is_some_and(|s| s == "import")
}
pub fn needs_company(a: &[String]) -> bool {
    a.get(1)
        .is_some_and(|s| matches!(s.as_str(), "contacts" | "archive" | "run"))
}
pub fn help() -> Value {
    json!({"commands":["import run --company SLUG|PATH --file EXPORT [--system synthetic-csv|dinero] [--dry-run VALUE]","import systems [--format json|human]","import archive --company SLUG|PATH [--system dinero] [--year INTEGER] [--format json|human]","import contacts --company SLUG|PATH --file Kontakter.csv [--enrich-cvr] [--default-role customer|vendor]"],"connection":"Existing --server ORIGIN or verified --local-profile ABS_PRIVATE_PROFILE; --cookie-file PRIVATE_FILE. --workspace is verified local workspace equality only. --allow-loopback-http is explicit.","controls":"Explicit command confirms one write. Current cookie authority; optional --actor user:ID equality check only. Contact import changes master data only. Run reconciles the source chart/company and adopts opening balances and Dinero history. Default role vendor; explicit original CSV requires only Kontaktnavn. Server classifies nonzero sales/purchases independently. CVR enrichment optional; operator credentials/cache unchanged. No automatic retry: outcome_unknown requires canonical contacts/audit readback before another explicit write.","example":"import run --example writes the original synthetic CSV sample without credentials or source-file access.","run":"Original complete server import; --dry-run VALUE is presence-valued (even false means dry-run). No generated key/hash or retry. Lost or invalid receipts require canonical imports/opening/journals/source readback. Archive: Archive is read-only historical reference data outside the live ledger; every list/detail revalidates current authority and physical originals. --system is trimmed/default dinero; omitted or blank --year lists years."})
}
fn parse(a: &[String]) -> Result<BTreeMap<&str, &str>, Output> {
    if a == ["import"] || a == ["import", "--help"] {
        return Ok(BTreeMap::from([("--help", "true")]));
    }
    let op = a
        .get(1)
        .map(String::as_str)
        .filter(|s| matches!(*s, "systems" | "contacts" | "archive" | "run"))
        .ok_or_else(|| {
            usage("Supported: import run, systems, contacts and archive; see import --help")
        })?;
    let mut f = BTreeMap::new();
    let mut i = 2;
    while i < a.len() {
        let (k, attached) = crate::original_args::option(&a[i]);
        if !matches!(
            k,
            "--server"
                | "--local-profile"
                | "--workspace"
                | "--cookie-file"
                | "--company"
                | "--actor"
                | "--json"
                | "--format"
                | "--help"
                | "--allow-loopback-http"
        ) && !(op == "contacts" && matches!(k, "--file" | "--enrich-cvr" | "--default-role"))
            && !(op == "run" && matches!(k, "--file" | "--system" | "--dry-run" | "--example"))
            && !(op == "archive" && matches!(k, "--system" | "--year"))
        {
            return Err(usage(format!("Unknown import flag {k}")));
        }
        let (v, next) = if matches!(
            k,
            "--json" | "--help" | "--allow-loopback-http" | "--enrich-cvr" | "--example"
        ) {
            if attached.is_some() {
                return Err(usage("Boolean flag takes no value"));
            }
            ("true", i + 1)
        } else {
            crate::original_args::value(a, i)
                .ok_or_else(|| usage(format!("Missing value for {k}")))?
        };
        if f.insert(k, v).is_some() {
            return Err(usage(format!("Duplicate {k}")));
        }
        i = next;
    }
    if f.get("--format")
        .is_some_and(|v| !matches!(*v, "human" | "json"))
    {
        return Err(usage("--format must be human or json"));
    }
    if f.contains_key("--help") || f.contains_key("--example") {
        return Ok(f);
    }
    if op == "systems" {
        return Ok(f);
    }
    if op == "run" {
        run_body(&f)?;
    }
    let role = f.get("--default-role").map(|s| s.trim()).unwrap_or("");
    if !matches!(role, "" | "customer" | "vendor") {
        return Err(usage("--default-role must be 'customer' or 'vendor'"));
    }
    let local = f.contains_key("--local-profile");
    if local && (f.contains_key("--server") || f.contains_key("--allow-loopback-http"))
        || !local && f.contains_key("--workspace")
    {
        return Err(usage(
            "--workspace requires verified --local-profile; explicit transports are exclusive",
        ));
    }
    if f.get("--company").is_none_or(|s| s.trim().is_empty()) {
        return Err(usage("Explicit --company required"));
    }
    if !local && !f.contains_key("--server") {
        return Err(usage(
            "Existing runtime required: --server or verified --local-profile; no autostart or local fallback",
        ));
    }
    if !f.contains_key("--cookie-file") {
        return Err(usage("Explicit private human --cookie-file required"));
    }
    if f.get("--actor")
        .is_some_and(|s| !s.starts_with("user:") || s.len() <= 5)
    {
        return Err(usage("--actor only asserts current cookie user:ID"));
    }
    if let Some(s) = f.get("--server") {
        ApiClient::new(s, f.contains_key("--allow-loopback-http"), None)
            .map_err(|_| usage("Invalid explicit server origin"))?;
    }
    if op == "contacts" {
        body(&f)?;
    } else if op == "archive" {
        archive_year(&f)?;
    }
    Ok(f)
}
// Bounded regular UTF-8 input; validate exact JSON bytes before any credentials or transport.
fn body(f: &BTreeMap<&str, &str>) -> Result<Vec<u8>, Output> {
    let path = std::path::Path::new(
        f.get("--file")
            .copied()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| usage("Missing required --file <Kontakter.csv>"))?,
    );
    let file = std::fs::File::open(path).map_err(|_| usage("Contacts CSV cannot be opened"))?;
    let meta = file
        .metadata()
        .map_err(|_| usage("Contacts CSV metadata unavailable"))?;
    if !meta.is_file() || meta.len() == 0 || meta.len() > LIMIT as u64 {
        return Err(usage(
            "Contacts CSV must be a nonempty bounded regular file",
        ));
    }
    let mut bytes = Vec::new();
    file.take((LIMIT + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| usage("Contacts CSV read failed"))?;
    let content = std::str::from_utf8(&bytes).map_err(|_| usage("Contacts CSV must be UTF-8"))?;
    if content.is_empty() || bytes.len() > LIMIT {
        return Err(usage("Contacts CSV must be nonempty and bounded"));
    }
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| usage("Contacts CSV basename must be UTF-8"))?;
    let role = f
        .get("--default-role")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or("vendor");
    let encoded=serde_json::to_vec(&json!({"fileName":name,"content":content,"enrichCvr":f.contains_key("--enrich-cvr"),"confirm":true,"defaultRole":role})).unwrap();
    if encoded.len() > LIMIT {
        return Err(usage("Exact encoded contact import JSON exceeds 12 MiB"));
    }
    Ok(encoded)
}
pub fn preflight(a: &[String]) -> Result<bool, Output> {
    let f = parse(a)?;
    if f.contains_key("--example") {
        return Err(run_example());
    }
    if f.contains_key("--help") {
        return Ok(true);
    }
    // Main's preflight output path returns without entering local/profile binding.
    if a.get(1).is_some_and(|s| s == "systems") {
        return Err(execute(a));
    }
    Ok(false)
}
fn machine(a: &[String]) -> bool {
    a.iter().any(|s| s == "--json" || s == "--format=json")
        || a.windows(2).any(|w| w == ["--format", "json"])
}
fn emit(v: Value, machine: bool) -> Output {
    let exit = if v["ok"] == false { 1 } else { 0 };
    let text = if machine {
        serde_json::to_string_pretty(&v).unwrap()
    } else if let Some(rows) = v["systems"].as_array() {
        let mut s = "system         | label\n".to_owned();
        for r in rows {
            s.push_str(&format!(
                "{:<14} | {}\n",
                r["system"].as_str().unwrap(),
                r["label"].as_str().unwrap()
            ));
        }
        s
    } else {
        let mut s = format!("{} Contact import\n", if exit == 0 { "✔" } else { "✘" });
        if let Some(summary) = v["summary"].as_object() {
            for (k, v) in summary {
                s.push_str(&format!("  {k}: {v}\n"));
            }
        }
        for e in v["errors"].as_array().into_iter().flatten() {
            s.push_str(&format!("  → {}\n", e.as_str().unwrap()));
        }
        s
    };
    Output {
        stdout: format!("{text}\n"),
        stderr: String::new(),
        exit,
    }
}
pub fn binding_refusal(a: &[String]) -> Output {
    if a.get(1).is_some_and(|s| s == "run") {
        return emit_run(
            json!({"ok":false,"errors":["Verified runtime/owner/company binding refused; no operation dispatched"]}),
            machine(a),
        );
    }
    if a.get(1).is_some_and(|s| s == "archive") {
        return emit_archive(
            json!({"ok":false,"years":[],"errors":["Verified runtime/owner/company binding refused; no operation dispatched"]}),
            machine(a),
        );
    }
    emit(
        json!({"ok":false,"errors":["Verified runtime/owner/company binding refused; no operation dispatched"]}),
        machine(a),
    )
}
fn flat(v: Value) -> Result<Value, Error> {
    if v["ok"] == false && v["code"].is_string() {
        if v.as_object().is_none_or(|o| {
            o.keys()
                .any(|k| !matches!(k.as_str(), "ok" | "code" | "errors" | "outcome"))
        }) || v.get("errors").is_some_and(|e| {
            e.as_array()
                .is_none_or(|a| a.iter().any(|e| !e.is_string()))
        }) {
            return Err(Error::OutcomeUnknown);
        }
        let errors = v
            .get("errors")
            .cloned()
            .unwrap_or_else(|| json!([v["code"].as_str().unwrap()]));
        return Ok(json!({"ok":false,"errors":errors}));
    }
    let inner = &v["import"];
    let fields = [
        "parsed",
        "customersCreated",
        "vendorsCreated",
        "skipped",
        "enriched",
        "enrichmentFailures",
    ];
    if !v["ok"].is_boolean()
        || v.as_object().is_none_or(|o| o.len() != 2)
        || inner.as_object().is_none_or(|o| {
            o.keys()
                .any(|k| !matches!(k.as_str(), "detected" | "summary" | "errors"))
        })
        || inner["summary"].as_object().is_none_or(|o| {
            o.len() != fields.len() || fields.iter().any(|k| !o.get(*k).is_some_and(Value::is_u64))
        })
        || inner["errors"]
            .as_array()
            .is_none_or(|a| a.iter().any(|e| !e.is_string()))
    {
        return Err(Error::OutcomeUnknown);
    }
    Ok(json!({"ok":v["ok"],"summary":inner["summary"],"errors":inner["errors"]}))
}
pub fn execute(a: &[String]) -> Output {
    let f = match parse(a) {
        Ok(f) => f,
        Err(o) => return o,
    };
    if f.contains_key("--example") {
        return run_example();
    }
    if f.contains_key("--help") {
        return Output {
            stdout: format!("{}\n", help()),
            stderr: String::new(),
            exit: 0,
        };
    }
    let m = machine(a);
    if a[1] == "systems" {
        return emit(
            json!({"ok":true,"systems":[{"system":"synthetic-csv","label":"Synthetic CSV (import-framework example)"},{"system":"dinero","label":"Dinero (data export — chart of accounts, master data & opening balance)"}]}),
            m,
        );
    }
    if a[1] == "run" {
        return execute_run(&f, m);
    }
    if a[1] == "archive" {
        return execute_archive(&f, m);
    }
    let bytes = match body(&f) {
        Ok(b) => b,
        Err(o) => return o,
    };
    let fail = |s: String| emit(json!({"ok":false,"errors":[s]}), m);
    let api = match crate::remote::credential_client(
        f["--server"],
        f.contains_key("--allow-loopback-http"),
        &f,
        false,
    ) {
        Ok(api) => api,
        Err((_, e)) => return fail(format!("{}: {}", e.code, e.message)),
    };
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(_) => return fail("Runtime unavailable".into()),
    };
    let result = rt.block_on(async {
        if let Some(actor) = f.get("--actor") {
            let me = api.me().await?;
            let id = me["user"]["id"].as_str().ok_or(Error::Protocol)?;
            if *actor != format!("user:{id}") {
                return Err(Error::InvalidInput(
                    "actor differs from current cookie user",
                ));
            }
        }
        flat(api.import_contacts(f["--company"], bytes, true).await?)
    });
    match result {
        Ok(v) => emit(v, m),
        Err(e) => {
            let (_, e) = crate::remote::client_error(e);
            fail(format!(
                "{}: {}; no automatic retry; read canonical contacts and audit before another explicit write",
                e.code, e.message
            ))
        }
    }
}

// JavaScript Number()/Number.isInteger semantics, not the native route's u16
// restriction. Out-of-range integers must still reach the guarded LIST first.
fn archive_year(f: &BTreeMap<&str, &str>) -> Result<Option<f64>, Output> {
    let Some(raw) = f.get("--year").map(|s| s.trim()).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let radix = [
        ("0x", 16),
        ("0X", 16),
        ("0b", 2),
        ("0B", 2),
        ("0o", 8),
        ("0O", 8),
    ];
    let n = if let Some((prefix, base)) = radix.iter().find(|(p, _)| raw.starts_with(p)) {
        let digits = &raw[prefix.len()..];
        if digits.is_empty() {
            None
        } else {
            digits.chars().try_fold(0.0, |n, c| {
                c.to_digit(*base).map(|d| n * *base as f64 + d as f64)
            })
        }
    } else {
        raw.parse::<f64>().ok()
    };
    n.filter(|n| n.is_finite() && n.fract() == 0.0)
        .map(Some)
        .ok_or_else(|| usage("Invalid --year: expected a four-digit fiscal year"))
}
fn emit_archive(mut v: Value, machine: bool) -> Output {
    // Original CLI ABI is flat, not an MCP envelope or native proof metadata.
    v.as_object_mut()
        .unwrap()
        .retain(|k, _| matches!(k.as_str(), "ok" | "years" | "errors"));
    let exit = if v["ok"] == true { 0 } else { 1 };
    if machine {
        return Output {
            stdout: format!("{}\n", serde_json::to_string_pretty(&v).unwrap()),
            stderr: String::new(),
            exit,
        };
    }
    if exit == 1 {
        return Output {
            stdout: String::new(),
            stderr: format!(
                "{}\n",
                v["errors"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|s| s.as_str().unwrap())
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
            exit,
        };
    }
    let mut text = String::new();
    for y in v["years"].as_array().unwrap() {
        if let Some(postings) = y["postings"].as_array() {
            text.push_str(&format!(
                "Archived {} fiscal year {} (imported {}) — read-only, not in the live ledger\n",
                y["sourceSystem"].as_str().unwrap(),
                y["fiscalYear"],
                y["importedAt"].as_str().unwrap()
            ));
            text.push_str("konto | dato | bilag | tekst | beloeb\n");
            for p in postings {
                text.push_str(&format!(
                    "{} | {} | {} | {} | {}\n",
                    p["accountNo"].as_str().unwrap(),
                    p["transactionDate"].as_str().unwrap(),
                    p["voucher"].as_str().unwrap(),
                    p["text"].as_str().unwrap(),
                    p["amount"].as_f64().unwrap()
                ));
            }
            text.push_str("konto | kontonavn | beloeb\n");
            for b in y["balances"].as_array().unwrap() {
                text.push_str(&format!(
                    "{} | {} | {}\n",
                    b["accountNo"].as_str().unwrap(),
                    b["accountName"].as_str().unwrap(),
                    b["amount"].as_f64().unwrap()
                ));
            }
        }
    }
    if text.is_empty() {
        text.push_str("fiscalYear | sourceSystem | postings | balances | importedAt\n");
        for y in v["years"].as_array().unwrap() {
            text.push_str(&format!(
                "{} | {} | {} | {} | {}\n",
                y["fiscalYear"],
                y["sourceSystem"].as_str().unwrap(),
                y["postingCount"],
                y["balanceCount"],
                y["importedAt"].as_str().unwrap()
            ));
        }
    }
    Output {
        stdout: text,
        stderr: String::new(),
        exit,
    }
}
fn execute_archive(f: &BTreeMap<&str, &str>, machine: bool) -> Output {
    let fail = |s: String| emit_archive(json!({"ok":false,"years":[],"errors":[s]}), machine);
    let api = match crate::remote::credential_client(
        f["--server"],
        f.contains_key("--allow-loopback-http"),
        f,
        false,
    ) {
        Ok(api) => api,
        Err((_, e)) => return fail(format!("{}: {}", e.code, e.message)),
    };
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(_) => return fail("Runtime unavailable".into()),
    };
    let year = match archive_year(f) {
        Ok(y) => y,
        Err(o) => return o,
    };
    let source = f
        .get("--system")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or("dinero");
    let result: Result<Value, Error> = rt.block_on(async {
        if let Some(actor) = f.get("--actor") {
            let me = api.me().await?;
            let id = me["user"]["id"].as_str().ok_or(Error::Protocol)?;
            if *actor != format!("user:{id}") {
                return Err(Error::InvalidInput(
                    "actor differs from current cookie user",
                ));
            }
        }
        Ok(rentemester_client::query_original_archive(
            &api,
            f["--company"],
            source,
            year,
            |raw, numeric| {
                rentemester_engine::archive_import::ArchiveDecimal::parse_canonical(raw)
                    .is_ok_and(|p| p.matches_numeric_decimal(numeric))
            },
        )
        .await)
    });
    match result {
        Ok(v) => emit_archive(v, machine),
        Err(e) => {
            let (_, e) = crate::remote::client_error(e);
            fail(format!("{}: {}", e.code, e.message))
        }
    }
}

fn run_body(f: &BTreeMap<&str, &str>) -> Result<Vec<u8>, Output> {
    let path = std::path::Path::new(
        f.get("--file")
            .copied()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| usage("Missing required --file <export-file-or-directory>"))?,
    );
    if !path.try_exists().unwrap_or(false) {
        return Err(usage("Export path does not exist"));
    }
    let system = f
        .get("--system")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or("synthetic-csv");
    let (name, bytes) = match system {
        "synthetic-csv" => {
            let (bytes, _) = original_source::stable_file(path, 48 * 1024).map_err(|_| {
                usage("Synthetic source must be a stable regular UTF-8 file <=48 KiB")
            })?;
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| usage("Source basename must be UTF-8"))?;
            (name.to_owned(), bytes)
        }
        "dinero" => original_source::source(path).map_err(|e| usage(e.message))?,
        _ => return Err(usage("Unknown --system; available: synthetic-csv, dinero")),
    };
    rentemester_client::original_import_body(system, &name, &bytes, f.contains_key("--dry-run"))
        .map_err(|e| usage(e.to_string()))
}
fn execute_run(f: &BTreeMap<&str, &str>, machine: bool) -> Output {
    let bytes = match run_body(f) {
        Ok(b) => b,
        Err(o) => return o,
    };
    let fail = |s: String| emit_run(json!({"ok":false,"errors":[s]}), machine);
    let api = match crate::remote::credential_client(
        f["--server"],
        f.contains_key("--allow-loopback-http"),
        f,
        false,
    ) {
        Ok(api) => api,
        Err((_, e)) => return fail(format!("{}: {}", e.code, e.message)),
    };
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(_) => return fail("Runtime unavailable".into()),
    };
    let result = rt.block_on(async {
        if let Some(actor) = f.get("--actor") {
            let me = api.me().await?;
            let id = me["user"]["id"].as_str().ok_or(Error::Protocol)?;
            if *actor != format!("user:{id}") {
                return Err(Error::InvalidInput(
                    "actor differs from current cookie user",
                ));
            }
        }
        api.import_original(f["--company"], bytes).await
    });
    match result {
        Ok(v) => emit_run(v, machine),
        Err(e) => {
            let (_, e) = crate::remote::client_error(e);
            fail(format!(
                "{}: {}; no automatic retry; read canonical imports/opening/journals/source before another explicit action",
                e.code, e.message
            ))
        }
    }
}

fn run_example() -> Output {
    Output {
        stdout: RUN_EXAMPLE.to_owned(),
        stderr: String::new(),
        exit: 0,
    }
}

// Match the original generic renderer: JSON stays on stdout; human failures
// use stderr and successful opening adoption shows its posting number.
fn emit_run(v: Value, machine: bool) -> Output {
    let failed = v["ok"] == false;
    if machine {
        return Output {
            stdout: format!("{}\n", serde_json::to_string_pretty(&v).unwrap()),
            stderr: String::new(),
            exit: u8::from(failed),
        };
    }
    if failed {
        let mut stderr = format!("✘ {RUN_LABEL} failed\n");
        for error in v["errors"].as_array().into_iter().flatten() {
            stderr.push_str(&format!("  → {}\n", error.as_str().unwrap()));
        }
        return Output {
            stdout: String::new(),
            stderr,
            exit: 1,
        };
    }
    let first = RUN_LABEL.split(". ").next().unwrap();
    let heading = if first.chars().count() > 60 {
        format!("{}…", first.chars().take(57).collect::<String>())
    } else {
        first.to_owned()
    };
    let mut stdout = format!("✔ {heading}\n");
    if let Some(number) = v["entryNo"].as_str() {
        stdout.push_str(&format!("  Posteringsnummer: {number}\n"));
    }
    Output {
        stdout,
        stderr: String::new(),
        exit: 0,
    }
}
