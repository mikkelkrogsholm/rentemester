//! Original exception commands; server authentication owns review attribution.
use crate::dated_reports::Output;
use serde_json::{Value,json};
use std::collections::BTreeMap;
pub fn accepts(args: &[String]) -> bool {
    args.first().is_some_and(|s| s == "exceptions") && args.get(1).is_some_and(|s| matches!(s.as_str(),"list"|"resolve"))
}
pub fn help() -> Value {
    json!({"commands":["exceptions list --company SLUG [--status open|resolved|all] [--include-archived]","exceptions resolve --company SLUG --id POSITIVE [--note TEXT]"],"connection":"--server ORIGIN or --local-profile PATH; --cookie-file PATH or --service-key-file PATH; --allow-loopback-http; --json or --format json|human","authority":"Explicit resolve invocation confirms one write. Server-owned identity; no stdin, caller actor, extra confirm, idempotency key or retry."})
}
fn output(value: Value, json_mode: bool, exit: u8) -> Output {
    if json_mode && exit != 2 {
        return Output {
            stdout: format!("{}\n", serde_json::to_string_pretty(&value).unwrap()),
            stderr: String::new(),
            exit,
        };
    }
    if exit == 0 {
        Output {
            stdout: format!(
                "Exceptions\n{}\n",
                serde_json::to_string_pretty(&value).unwrap()
            ),
            stderr: String::new(),
            exit,
        }
    } else {
        Output {
            stdout: String::new(),
            stderr: format!("{}\n", value["errors"]),
            exit,
        }
    }
}
fn usage(message: &str) -> Output {
    output(json!({"ok":false,"errors":[message]}), false, 2)
}

fn parse(args: &[String]) -> Result<BTreeMap<&str,&str>,Output> {
    let mut flags = BTreeMap::new();
    let mut cursor = 2;
    while cursor < args.len() {
        let (key,attached) = crate::original_args::option(&args[cursor]);
        let global = matches!(key,"--company"|"--server"|"--cookie-file"|"--service-key-file"|"--local-profile"|"--json"|"--help"|"--allow-loopback-http");
        let domain = match args.get(1).map(String::as_str) { Some("list") => matches!(key,"--status"|"--include-archived"), Some("resolve") => matches!(key,"--id"|"--note"), _ => false };
        if !global && !domain { return Err(usage("Unknown exceptions flag")); }
        let boolean = matches!(key,"--json"|"--help"|"--allow-loopback-http"|"--include-archived");
        let (value,next) = if boolean {
            if attached.is_some() { return Err(usage("Boolean flag does not take a value")); }
            ("true",cursor+1)
        } else { crate::original_args::value(args,cursor).ok_or_else(|| usage("Missing flag value"))? };
        if flags.insert(key,value).is_some() { return Err(usage("Duplicate exceptions flag")); }
        cursor = next;
    }
    if !flags.contains_key("--help") && args.get(1).is_some_and(|s| s == "resolve") {
        positive_id(flags.get("--id").copied().unwrap_or(""))?;
        if flags.get("--note").is_some_and(|s| s.len() > 4096) { return Err(usage("Note exceeds4096 bytes")); }
    }
    Ok(flags)
}
fn positive_id(raw: &str) -> Result<i64,Output> {
    raw.parse::<i64>().ok().filter(|n| (1..=9_007_199_254_740_991).contains(n)).filter(|_| raw.bytes().all(|b| b.is_ascii_digit())).ok_or_else(|| usage("Missing required --id <n>; positive safe integer required"))
}
pub fn preflight(args: &[String]) -> Result<(),Output> { parse(args).map(|_| ()) }
pub fn execute(args: &[String]) -> Output {
    let flags = match parse(args) { Ok(f) => f, Err(o) => return o };
    if flags.contains_key("--help") { return Output { stdout:format!("{}\n",help()),stderr:String::new(),exit:0 }; }
    for key in ["--company","--server"] {
        if flags.get(key).is_none_or(|s| s.is_empty()) { return usage(&format!("Explicit {key} required")); }
    }
    if flags.contains_key("--cookie-file") == flags.contains_key("--service-key-file") { return usage("Choose exactly one --cookie-file/--service-key-file"); }
    let json_mode = flags.contains_key("--json");
    let api = match crate::remote::credential_client(flags["--server"],flags.contains_key("--allow-loopback-http"),&flags,false) {
        Ok(api) => api,
        Err((exit,e)) => return output(json!({"ok":false,"code":e.code,"errors":[e.message]}),json_mode,exit)
    };
    let rt = match tokio::runtime::Runtime::new() { Ok(r) => r, Err(_) => return output(json!({"ok":false,"errors":["HTTP runtime unavailable"]}),json_mode,1) };
    let result = rt.block_on(async {
        if args[1] == "list" {
            api.exceptions_original_list(flags["--company"],flags.get("--status").copied(),flags.contains_key("--include-archived")).await
        } else {
            let mut body = json!({});
            if let Some(note) = flags.get("--note") { body["note"] = json!(note); }
            api.exceptions_original_resolve(flags["--company"],positive_id(flags["--id"]).unwrap(),serde_json::to_vec(&body).unwrap(),true).await
        }
    });
    match result {
        Ok(value) => { let exit = if value["ok"] == false { 1 } else { 0 }; output(value,json_mode,exit) }
        Err(e) => { let (exit,e) = crate::remote::client_error(e); output(json!({"ok":false,"code":e.code,"errors":[e.message]}),json_mode,exit) }
    }
}
