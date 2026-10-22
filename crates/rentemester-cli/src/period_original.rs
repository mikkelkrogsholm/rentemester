//! Original period lifecycle. The authenticated server owns evidence and authority.
use crate::dated_reports::Output;
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub fn accepts(args: &[String]) -> bool {
    args.first().is_some_and(|s| s == "period")
        && args.get(1).is_some_and(|s| {
            matches!(
                s.as_str(),
                "readiness" | "review" | "status" | "close" | "reopen"
            )
        })
}
pub fn help() -> Value {
    json!({"commands":["period readiness --from YYYY-MM-DD --to YYYY-MM-DD","period review --from YYYY-MM-DD --to YYYY-MM-DD --packet-hash SHA256 --confirm yes","period status --review-id N","period close --from YYYY-MM-DD --to YYYY-MM-DD --confirm yes [--packet-hash SHA256 --review-id N] [--kind vat_period|vat_quarter|fiscal_year|custom] [--status closed|reported] [--reference TEXT] [--reason TEXT]","period reopen --from YYYY-MM-DD --to YYYY-MM-DD --reason TEXT [--kind KIND]"],"connection":"--company SLUG (or RENTEMESTER_COMPANY), --server ORIGIN (or --local-profile PATH), exactly one --cookie-file PATH/--service-key-file PATH; optional --allow-loopback-http, --json/--format json|human","authority":"Server-owned identity; no actor, force bypass, automatic retries or local writes. Reopen invocation confirms implicitly. Close defaults to closed; reported is distinct."})
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
                "Regnskabsperiode\n{}\n",
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
type Flags<'a> = BTreeMap<&'a str, &'a str>;
fn parse(args: &[String]) -> Result<Flags<'_>, Output> {
    let op = args.get(1).map(String::as_str).unwrap_or("");
    let mut flags = BTreeMap::new();
    let mut cursor = 2;
    while cursor < args.len() {
        let (key, attached) = crate::original_args::option(&args[cursor]);
        let global = matches!(
            key,
            "--company"
                | "--server"
                | "--cookie-file"
                | "--service-key-file"
                | "--local-profile"
                | "--json"
                | "--help"
                | "--allow-loopback-http"
        );
        let allowed = match op {
            "readiness" => matches!(key, "--from" | "--to"),
            "review" => matches!(key, "--from" | "--to" | "--packet-hash" | "--confirm"),
            "status" => key == "--review-id",
            "close" => matches!(
                key,
                "--from"
                    | "--to"
                    | "--packet-hash"
                    | "--review-id"
                    | "--confirm"
                    | "--kind"
                    | "--status"
                    | "--reference"
                    | "--reason"
            ),
            "reopen" => matches!(key, "--from" | "--to" | "--kind" | "--reason"),
            _ => false,
        };
        if !global && !allowed {
            return Err(usage(
                "Unknown period flag; --force and caller actor are not supported",
            ));
        }
        let boolean = matches!(key, "--json" | "--help" | "--allow-loopback-http");
        let (value, next) = if boolean {
            if attached.is_some() {
                return Err(usage("Boolean flag does not take a value"));
            }
            ("true", cursor + 1)
        } else {
            crate::original_args::value(args, cursor).ok_or_else(|| usage("Missing flag value"))?
        };
        if flags.insert(key, value).is_some() {
            return Err(usage("Duplicate period flag"));
        }
        cursor = next;
    }
    if flags.contains_key("--help") {
        return Ok(flags);
    }
    if op != "status" {
        let from = flags.get("--from").ok_or_else(|| usage("Missing --from"))?;
        let to = flags.get("--to").ok_or_else(|| usage("Missing --to"))?;
        rentemester_engine::trial_balance::Period::new(from, to)
            .map_err(|_| usage("Invalid ISO period dates"))?;
    }
    if matches!(op, "review" | "close") && flags.get("--confirm") != Some(&"yes") {
        return Err(usage("--confirm must be exactly yes"));
    }
    if op == "review" && !flags.contains_key("--packet-hash") {
        return Err(usage("Missing --packet-hash"));
    }
    if let Some(hash) = flags.get("--packet-hash")
        && (hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err(usage("--packet-hash must be exactly 64 hexadecimal characters"));
    }
    if op == "status" && !flags.contains_key("--review-id") {
        return Err(usage("Missing --review-id"));
    }
    if let Some(id) = flags.get("--review-id") {
        positive_id(id)?;
    }
    if let Some(kind) = flags.get("--kind")
        && !matches!(*kind, "vat_period" | "vat_quarter" | "fiscal_year" | "custom")
    {
        return Err(usage("Invalid --kind"));
    }
    if let Some(status) = flags.get("--status")
        && !matches!(*status, "closed" | "reported")
    {
        return Err(usage("Invalid --status"));
    }
    if op == "reopen" && flags.get("--reason").is_none_or(|s| s.trim().is_empty()) {
        return Err(usage("Missing nonblank --reason"));
    }
    Ok(flags)
}
fn positive_id(raw: &str) -> Result<i64, Output> {
    raw.parse::<i64>()
        .ok()
        .filter(|id| (1..=9_007_199_254_740_991).contains(id))
        .filter(|_| raw.bytes().all(|b| b.is_ascii_digit()))
        .ok_or_else(|| usage("--review-id must be a positive safe integer"))
}
/// Run before local profile/credential I/O, including for malformed domain flags.
pub fn preflight(args: &[String]) -> Result<(), Output> {
    parse(args).map(|_| ())
}
pub fn execute(args: &[String]) -> Output {
    let flags = match parse(args) {
        Ok(flags) => flags,
        Err(out) => return out,
    };
    if flags.contains_key("--help") || !accepts(args) {
        return Output {
            stdout: format!("{}\n", help()),
            stderr: String::new(),
            exit: 0,
        };
    }
    for key in ["--company", "--server"] {
        if flags.get(key).is_none_or(|s| s.is_empty()) {
            return usage(&format!("Explicit {key} required"));
        }
    }
    if flags.contains_key("--cookie-file") == flags.contains_key("--service-key-file") {
        return usage("Choose exactly one --cookie-file/--service-key-file");
    }
    let json_mode = flags.contains_key("--json");
    let api = match crate::remote::credential_client(
        flags["--server"],
        flags.contains_key("--allow-loopback-http"),
        &flags,
        false,
    ) {
        Ok(api) => api,
        Err((exit, e)) => {
            return output(
                json!({"ok":false,"code":e.code,"errors":[e.message]}),
                json_mode,
                exit,
            );
        }
    };
    let mut body = json!({"confirm":true});
    for (flag, key) in [
        ("--from", "periodStart"),
        ("--to", "periodEnd"),
        ("--packet-hash", "packetHash"),
        ("--kind", "kind"),
        ("--status", "status"),
        ("--reference", "reference"),
        ("--reason", "reason"),
    ] {
        if let Some(value) = flags.get(flag) {
            body[key] = json!(value);
        }
    }
    if let Some(id) = flags.get("--review-id") {
        body["reviewId"] = json!(positive_id(id).unwrap());
    }
    let bytes = serde_json::to_vec(&body).unwrap();
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(_) => {
            return output(
                json!({"ok":false,"errors":["HTTP runtime unavailable"]}),
                json_mode,
                1,
            );
        }
    };
    let company = flags["--company"];
    let result = rt.block_on(async {
        match args[1].as_str() {
            "readiness" => {
                api.period_original_readiness(company, flags["--from"], flags["--to"])
                    .await
            }
            "status" => {
                api.period_original_status(company, positive_id(flags["--review-id"]).unwrap())
                    .await
            }
            "review" => api.period_original_review(company, bytes, true).await,
            "close" => api.period_original_close(company, bytes, true).await,
            "reopen" => api.period_original_reopen(company, bytes, true).await,
            _ => unreachable!(),
        }
    });
    match result {
        Ok(value) => {
            let exit = if value["ok"] == false { 1 } else { 0 };
            output(value, json_mode, exit)
        }
        Err(e) => {
            let (exit, e) = crate::remote::client_error(e);
            output(
                json!({"ok":false,"code":e.code,"errors":[e.message]}),
                json_mode,
                exit,
            )
        }
    }
}
