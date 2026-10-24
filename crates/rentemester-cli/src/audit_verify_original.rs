//! Original flat diagnostic projection; the native verifier remains authoritative.
use crate::dated_reports::Output;
use serde_json::{Value, json};
use std::collections::BTreeMap;
const LABEL: &str = "Verificerer audit-kæde og bogføringsintegritet.";
pub fn accepts(args: &[String]) -> bool {
    args.first().is_some_and(|s| s == "audit")
}
pub fn help() -> Value {
    json!({"commands":["audit verify --company SLUG|VERIFIED_LOCAL_PATH"],"connection":"--server ORIGIN or --local-profile ABSOLUTE_PATH; --cookie-file PATH or --service-key-file PATH; --allow-loopback-http; --json or --format json|human","output":{"current":"{ok,entries,errors}","refusal":"{ok:false,errors}"},"exitCodes":{"0":"verified clean","1":"findings or unavailable observation","2":"usage"},"compatibility":"Native audit verification v1 only. Offline pending-schema metadata is unavailable through this API; fail closed without invented schema or entry counts.","authority":"Read-only CompanyRead; no stdin, actor override, confirmation, repair, retry or fallback."})
}
fn usage(message: &str) -> Output {
    Output { stdout: String::new(), stderr: format!("{message}; see audit verify --help\n"), exit: 2 }
}
fn parse(args: &[String]) -> Result<BTreeMap<&str, &str>, Output> {
    let family_help = args == ["audit"] || args == ["audit", "--help"];
    if family_help { return Ok(BTreeMap::from([("--help", "true")])); }
    if args.get(1).is_none_or(|s| s != "verify") { return Err(usage("Unknown audit command")); }
    let mut flags = BTreeMap::new();
    let mut cursor = 2;
    while cursor < args.len() {
        let (key, attached) = crate::original_args::option(&args[cursor]);
        if !matches!(key, "--company"|"--server"|"--local-profile"|"--cookie-file"|"--service-key-file"|"--allow-loopback-http"|"--json"|"--help"|"--format") { return Err(usage("Unknown audit verify flag or argument")); }
        let (value, next) = if matches!(key, "--json"|"--help"|"--allow-loopback-http") {
            if attached.is_some() { return Err(usage("Boolean flag does not take a value")); }
            ("true", cursor + 1)
        } else { crate::original_args::value(args, cursor).filter(|(v,_)| !v.is_empty()).ok_or_else(|| usage("Missing flag value"))? };
        if flags.insert(key,value).is_some() { return Err(usage("Duplicate audit verify flag")); }
        cursor = next;
    }
    if flags.get("--format").is_some_and(|v| !matches!(*v,"human"|"json")) { return Err(usage("Invalid output format")); }
    if flags.contains_key("--server") && flags.contains_key("--local-profile") { return Err(usage("Choose one explicit transport")); }
    if flags.contains_key("--local-profile") && flags.contains_key("--allow-loopback-http") { return Err(usage("Local profile cannot override origin")); }
    if flags.contains_key("--cookie-file") && flags.contains_key("--service-key-file") { return Err(usage("Choose one private credential")); }
    if let Some(server) = flags.get("--server") {
        rentemester_client::ApiClient::new(server,flags.contains_key("--allow-loopback-http"),None).map_err(|_| usage("Invalid explicit server origin"))?;
    }
    if flags.contains_key("--help") { return Ok(flags); }
    let company = flags.get("--company").ok_or_else(|| usage("Explicit --company required"))?;
    if company.len() > 128 || (!flags.contains_key("--local-profile") && !company.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')) { return Err(usage("Invalid company selector")); }
    if !flags.contains_key("--server") && !flags.contains_key("--local-profile") { return Err(usage("Explicit transport required")); }
    if !flags.contains_key("--cookie-file") && !flags.contains_key("--service-key-file") { return Err(usage("Explicit private credential required")); }
    Ok(flags)
}
pub fn preflight(args: &[String]) -> Result<bool, Output> { parse(args).map(|f| f.contains_key("--help")) }
fn emit(value: Value, json_mode: bool) -> Output {
    let exit = if value["ok"] == true { 0 } else { 1 };
    if json_mode { return Output { stdout: format!("{}\n",serde_json::to_string_pretty(&value).unwrap()), stderr: String::new(), exit }; }
    if exit == 0 { Output { stdout: format!("✔ {LABEL}\n  Posteringer kontrolleret: {}\n",value["entries"]), stderr: String::new(), exit } }
    else { Output { stdout: String::new(), stderr: format!("✘ {LABEL} failed\n{}",value["errors"].as_array().unwrap().iter().map(|e| format!("  → {}\n",e.as_str().unwrap())).collect::<String>()), exit } }
}
pub fn refusal(args: &[String], message: &str) -> Output {
    emit(json!({"ok":false,"errors":[message]}),args.iter().any(|s|s=="--json"))
}
pub fn execute(args: &[String]) -> Output {
    let flags = match parse(args) { Ok(f)=>f, Err(o)=>return o };
    let json_mode = flags.contains_key("--json");
    let fail = |message: &str| emit(json!({"ok":false,"errors":[message]}),json_mode);
    let api = match crate::remote::credential_client(flags["--server"],flags.contains_key("--allow-loopback-http"),&flags,false) {
        Ok(api)=>api, Err(_)=>return fail("Audit verification unavailable: private credential or transport refused")
    };
    let runtime = match tokio::runtime::Runtime::new() { Ok(rt)=>rt, Err(_)=>return fail("Audit verification runtime unavailable") };
    match runtime.block_on(api.audit_verify(flags["--company"])) {
        Ok(value) if value["audit"]["available"] == true => {
            let audit = &value["audit"];
            emit(json!({"ok":audit["ok"],"entries":audit["entries"],"errors":audit["errors"]}),json_mode)
        }
        Ok(_) => fail("Audit verification unavailable; offline schema projection unsupported by native audit verification v1"),
        Err(_) => fail("Audit verification unavailable: transport, authorization or protocol refused"),
    }
}
