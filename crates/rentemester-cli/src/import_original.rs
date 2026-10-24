//! Original contact-import adapter. CSV parsing and all classification stay on the server.
use crate::dated_reports::Output;
use rentemester_client::{ApiClient, Error};
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
const LIMIT: usize = 12 * 1024 * 1024;
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
    a.get(1).is_some_and(|s| s == "contacts")
}
pub fn help() -> Value {
    json!({"commands":["import systems [--format json|human]","import contacts --company SLUG|PATH --file Kontakter.csv [--enrich-cvr] [--default-role customer|vendor]"],"connection":"Existing --server ORIGIN or verified --local-profile ABS_PRIVATE_PROFILE; --cookie-file PRIVATE_FILE. --workspace is verified local workspace equality only. --allow-loopback-http is explicit.","controls":"Explicit command confirms one write. Current cookie authority; optional --actor user:ID equality check only. Master data only: no journal, accounts, mappings or opening balances. Default role vendor; explicit original CSV requires only Kontaktnavn. Server classifies nonzero sales/purchases independently. CVR enrichment optional; operator credentials/cache unchanged. No automatic retry: outcome_unknown requires canonical contacts/audit readback before another explicit write.","unavailable":"import run and import archive are not implemented here."})
}
fn parse(a: &[String]) -> Result<BTreeMap<&str, &str>, Output> {
    if a == ["import"] || a == ["import", "--help"] {
        return Ok(BTreeMap::from([("--help", "true")]));
    }
    let op = a
        .get(1)
        .map(String::as_str)
        .filter(|s| matches!(*s, "systems" | "contacts"))
        .ok_or_else(|| {
            usage("Only import systems and import contacts are supported; see import --help")
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
        {
            return Err(usage(format!("Unknown import flag {k}")));
        }
        let (v, next) = if matches!(
            k,
            "--json" | "--help" | "--allow-loopback-http" | "--enrich-cvr"
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
    if f.contains_key("--help") {
        return Ok(f);
    }
    if op == "systems" {
        return Ok(f);
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
    body(&f)?;
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
