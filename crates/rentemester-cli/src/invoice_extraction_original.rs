//! Original extraction invocation and flat source results; native aliases stay separate.
use crate::dated_reports::Output;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::IsTerminal};

pub fn accepts(args: &[String]) -> bool {
    args.first().is_some_and(|s| s == "documents")
        && args
            .get(1)
            .is_some_and(|s| matches!(s.as_str(), "extract-invoice" | "invoice-extraction"))
}
pub fn help() -> Value {
    json!({"ok":true,"commands":["documents extract-invoice --document-id ID","documents invoice-extraction --document-id ID"],"connection":"--company SLUG or verified local company path; RENTEMESTER_COMPANY fallback; --server ORIGIN or --local-profile ABS_PRIVATE_PROFILE; --cookie-file PATH or --service-key-file PATH; --allow-loopback-http; --json or --format json|human","authority":"extract-invoice invocation explicitly requests extraction; no --confirm flag. Optional --actor on writes is only an exact assertion of the current credential identity (user:{subject} or service_account:{subject}), never authority. No environment actor. Optional --workspace only asserts the verified local profile workspace and is never sent over HTTP.","output":"Flat {ok:true,extraction} or {ok:false,errors:[...]}; needs_resolution is successful retained evidence, not accounting. Piped output defaults to JSON; terminal output defaults to human.","native":"remote extract-invoice --document-id ID --confirm and remote invoice-extraction retain native boolean confirmation and nested output; see remote --help.","recovery":"No automatic retries. After outcome_unknown, read documents invoice-extraction before considering another explicit attempt."})
}
pub fn family_help() -> Value {
    let mut all = crate::document_party_original::family_help();
    let extraction = help();
    all["commands"]
        .as_array_mut()
        .unwrap()
        .extend(extraction["commands"].as_array().unwrap().iter().cloned());
    all["invoiceExtraction"] = extraction;
    all
}
fn usage(message: &str) -> Output {
    Output {
        stdout: String::new(),
        stderr: format!("{message}\n"),
        exit: 2,
    }
}
fn parse(args: &[String]) -> Result<BTreeMap<&str, &str>, Output> {
    let mut flags = BTreeMap::new();
    let mut cursor = 2;
    while cursor < args.len() {
        let (key, attached) = crate::original_args::option(&args[cursor]);
        if !matches!(
            key,
            "--document-id"
                | "--company"
                | "--server"
                | "--cookie-file"
                | "--service-key-file"
                | "--local-profile"
                | "--workspace"
                | "--actor"
                | "--json"
                | "--format"
                | "--help"
                | "--allow-loopback-http"
        ) {
            return Err(usage(&format!(
                "Unknown invoice extraction flag {key}; see documents --help"
            )));
        }
        let (raw, next) = if matches!(key, "--json" | "--help" | "--allow-loopback-http") {
            if attached.is_some() {
                return Err(usage("Boolean flag takes no value"));
            }
            ("true", cursor + 1)
        } else {
            crate::original_args::value(args, cursor).ok_or_else(|| usage("Missing flag value"))?
        };
        if raw.trim().is_empty() || flags.insert(key, raw).is_some() {
            return Err(usage("Empty or duplicate extraction flag"));
        }
        cursor = next;
    }
    if flags
        .get("--format")
        .is_some_and(|s| !matches!(*s, "json" | "human"))
    {
        return Err(usage("--format must be json or human"));
    }
    if flags.contains_key("--cookie-file") && flags.contains_key("--service-key-file") {
        return Err(usage("Choose exactly one --cookie-file/--service-key-file"));
    }
    if flags.contains_key("--local-profile")
        && (flags.contains_key("--server") || flags.contains_key("--allow-loopback-http"))
    {
        return Err(usage(
            "--local-profile conflicts with --server/--allow-loopback-http",
        ));
    }
    if flags
        .get("--local-profile")
        .is_some_and(|s| !std::path::Path::new(s).is_absolute())
    {
        return Err(usage(
            "--local-profile requires an absolute private profile path",
        ));
    }
    if flags.contains_key("--help") {
        return Ok(flags);
    }
    if !flags.contains_key("--server") && !flags.contains_key("--local-profile") {
        return Err(usage("--server or --local-profile required"));
    }
    if !flags.contains_key("--cookie-file") && !flags.contains_key("--service-key-file") {
        return Err(usage("--cookie-file or --service-key-file required"));
    }
    let id = flags
        .get("--document-id")
        .ok_or_else(|| usage("Missing required --document-id <n>"))?;
    if !id.bytes().all(|b| b.is_ascii_digit())
        || id
            .parse::<i64>()
            .ok()
            .is_none_or(|n| !(1..=9_007_199_254_740_991).contains(&n))
    {
        return Err(usage("--document-id must be a positive safe integer"));
    }
    if let Some(actor) = flags.get("--actor") {
        if args[1] != "extract-invoice" || actor.trim() != *actor || actor.chars().count() > 160 {
            return Err(usage(
                "--actor must be a trimmed nonblank assertion of at most 160 characters on extraction writes only",
            ));
        }
    }
    Ok(flags)
}
pub fn preflight(args: &[String]) -> Result<(), Output> {
    let flags = parse(args)?;
    if !flags.contains_key("--help")
        && let Some(workspace) = flags.get("--workspace")
    {
        let profile = flags
            .get("--local-profile")
            .ok_or_else(|| usage("--workspace requires --local-profile"))?;
        let local = rentemester_client::LocalRuntime::load(std::path::Path::new(profile))
            .map_err(|_| usage("Could not validate local profile"))?;
        if std::path::Path::new(workspace) != local.workspace_root() {
            return Err(usage(
                "--workspace differs from verified local profile workspace",
            ));
        }
    }
    Ok(())
}
pub fn execute(args: &[String]) -> Output {
    let flags = match parse(args) {
        Ok(f) => f,
        Err(o) => return o,
    };
    if flags.contains_key("--help") {
        return Output {
            stdout: format!("{}\n", help()),
            stderr: String::new(),
            exit: 0,
        };
    }
    let json_mode = flags.contains_key("--json")
        || flags.get("--format") == Some(&"json")
        || (flags.get("--format") != Some(&"human") && !std::io::stdout().is_terminal());
    for key in ["--server", "--company"] {
        if !flags.contains_key(key) {
            return usage(&format!("Explicit {key} required"));
        }
    }
    let api = match crate::remote::credential_client(
        flags["--server"],
        flags.contains_key("--allow-loopback-http"),
        &flags,
        false,
    ) {
        Ok(api) => api,
        Err((exit, e)) => return client_failure(exit, e, json_mode, &args[1]),
    };
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(_) => {
            return render(
                json!({"ok":false,"errors":["HTTP runtime unavailable"]}),
                json_mode,
                1,
                &args[1],
            );
        }
    };
    let id = flags["--document-id"].parse::<i64>().unwrap();
    let result = runtime.block_on(async {
        if args[1] == "extract-invoice" {
            api.original_extract_invoice(
                flags["--company"],
                id,
                true,
                flags.get("--actor").copied(),
            )
            .await
        } else {
            api.original_invoice_extraction(flags["--company"], id)
                .await
        }
    });
    match result {
        Ok(v) => {
            let exit = if v["ok"] == true { 0 } else { 1 };
            render(v, json_mode, exit, &args[1])
        }
        Err(e) => {
            let (exit, e) = crate::remote::client_error(e);
            client_failure(exit, e, json_mode, &args[1])
        }
    }
}
fn client_failure(
    exit: u8,
    e: rentemester_engine::Error,
    json_mode: bool,
    operation: &str,
) -> Output {
    if exit == 2 {
        return usage(&e.message);
    }
    render(
        json!({"ok":false,"errors":[format!("{}: {}", e.code, e.message)]}),
        json_mode,
        exit,
        operation,
    )
}
fn render(value: Value, json_mode: bool, exit: u8, operation: &str) -> Output {
    if json_mode {
        return Output {
            stdout: format!("{value}\n"),
            stderr: String::new(),
            exit,
        };
    }
    let heading = if operation == "extract-invoice" {
        "Udtrækker citerbar fakturaevidens fra et gemt PDF-bilag."
    } else {
        "Viser fakturaudtræk og undtagelsesstatus uden filsti."
    };
    if exit == 0 {
        return Output {
            stdout: format!("✔ {heading}\n"),
            stderr: String::new(),
            exit,
        };
    }
    let mut lines = vec![format!("✘ {heading} failed")];
    if let Some(errors) = value["errors"].as_array() {
        for error in errors.iter().filter_map(Value::as_str) {
            lines.push(format!("  → {error}"));
        }
    }
    Output {
        stdout: String::new(),
        stderr: format!("{}\n", lines.join("\n")),
        exit,
    }
}
