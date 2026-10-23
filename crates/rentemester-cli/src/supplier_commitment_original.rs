//! Original supplier-commitment invocation and flat source results; native aliases stay separate.
use crate::dated_reports::Output;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::IsTerminal};

use rentemester_client::{SupplierCommitmentOriginalOperation as Op, validate_original_supplier_commitment};
fn operation(name: &str) -> Option<Op> {
    Some(match name { "plan"=>Op::Plan,"apply"=>Op::Apply,"list"=>Op::List,"change"=>Op::Change,"match"=>Op::Match,"matches"=>Op::Matches,"alerts"=>Op::Alerts,_=>return None })
}
pub fn accepts(args: &[String]) -> bool {
    args.first().is_some_and(|s|s=="supplier-commitment") && args.get(1).is_some_and(|s|operation(s).is_some())
}
pub fn help() -> Value {
    json!({"ok":true,"commands":["supplier-commitment plan --input INLINE_JSON","supplier-commitment apply --input INLINE_JSON --plan-hash HASH [--idempotency-key KEY] --confirm yes","supplier-commitment list","supplier-commitment change --id ID --action paused|ended|superseded --reason TEXT --confirm yes","supplier-commitment match --input INLINE_JSON --confirm yes","supplier-commitment matches [--id ID]","supplier-commitment alerts --as-of DATE"],"connection":"--company SLUG --server ORIGIN or --local-profile PATH; --cookie-file PATH or --service-key-file PATH; --json or --format human; optional write --actor is credential equality assertion only","compatibility":"Native source-bound hashes bind current active vendor/provenance, immutable document source bytes, requested input and previousPayloadHash. Schedule is nextDate+90 inclusive, NOT offline pure TypeScript parity. vendorSnapshot asserts current vendor NAME; omitted status is active, non-active initial status refused. vendor:<id> and document:<id> remain native identifiers.","safety":"Planning only, no accounting effects. Explicit approval, no automatic retries. Reconcile unknown writes through canonical reads; native remote aliases unchanged."})
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
            "--input" | "--plan-hash" | "--idempotency-key" | "--confirm"
                | "--id" | "--action" | "--reason" | "--as-of"
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
                "Unknown supplier-commitment flag {key}; see supplier-commitment --help"
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
        if flags.insert(key, raw).is_some() {
            return Err(usage("Empty or duplicate supplier-commitment flag"));
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
    payload(args, &flags)?;
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
    let input = match payload(args, &flags) {
        Ok(v) => v,
        Err(o) => return o,
    };
    let result = runtime.block_on(api.original_supplier_commitment(
        flags["--company"], operation(&args[1]).unwrap(), &input, flags.get("--actor").copied()));
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
        Output {
            stdout: format!("{value}\n"),
            stderr: String::new(),
            exit,
        }
    } else if exit == 0 {
        Output {
            stdout: format!("Supplier commitment {operation}\n{value}\n"),
            stderr: String::new(),
            exit,
        }
    } else {
        Output {
            stdout: String::new(),
            stderr: format!("Supplier commitment {operation} failed\n{value}\n"),
            exit,
        }
    }
}
fn payload(args: &[String], flags: &BTreeMap<&str, &str>) -> Result<Value, Output> {
    let op=operation(&args[1]).ok_or_else(||usage("Unknown supplier-commitment operation"))?;
    let allowed: &[&str] = match op {
        Op::Plan=> &["--input"], Op::Apply=> &["--input","--plan-hash","--idempotency-key","--confirm","--actor"],
        Op::List=> &[], Op::Change=> &["--id","--action","--reason","--confirm","--actor"],
        Op::Match=> &["--input","--confirm","--actor"], Op::Matches=> &["--id"], Op::Alerts=> &["--as-of"],
    };
    for k in flags.keys() {
        if !allowed.contains(k) && !["--company","--server","--cookie-file","--service-key-file","--local-profile","--workspace","--json","--format","--help","--allow-loopback-http"].contains(k) {return Err(usage("Flag not allowed for this supplier-commitment operation"));}
    }
    let required=|k|flags.get(k).copied().filter(|s|!s.trim().is_empty()).ok_or_else(||usage(&format!("Missing required {k}")));
    let mut input=json!({});
    if matches!(op,Op::Plan|Op::Apply|Op::Match) {
        let raw=required("--input")?;
        if raw.len()>65536 {return Err(usage("Inline JSON exceeds byte limit"));}
        let value:Value=serde_json::from_str(raw).map_err(|_|usage("--input requires INLINE JSON, not a filename"))?;
        if op==Op::Match {if !value.is_object() {return Err(usage("Match input must be an object"));} input=value;} else {input["commitment"]=value;}
    }
    match op {
        Op::Apply=> {input["payloadHash"]=json!(required("--plan-hash")?); if let Some(k)=flags.get("--idempotency-key") {input["idempotencyKey"]=json!(k);}},
        Op::Change=> {input=json!({"commitmentId":required("--id")?,"action":required("--action")?,"reason":required("--reason")?});},
        Op::Matches=> {if let Some(id)=flags.get("--id") {input["commitmentId"]=json!(id);}},
        Op::Alerts=> {input["asOf"]=json!(required("--as-of")?);},
        _=>{},
    }
    if matches!(op,Op::Apply|Op::Change|Op::Match) {
        if required("--confirm")?!="yes" {return Err(usage("--confirm yes required"));}
        if op==Op::Match && input.get("confirm").is_some() {return Err(usage("Match confirmation belongs in --confirm yes, not input"));}
        input["confirm"]=json!(true);
    }
    validate_original_supplier_commitment(op,&input).map_err(|e|usage(&e.to_string()))?;
    Ok(input)
}
