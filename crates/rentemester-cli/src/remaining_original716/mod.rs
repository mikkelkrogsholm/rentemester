//! Original remaining command boundaries; the admitted common server owns every domain effect.
use crate::dated_reports::Output;
use rentemester_client::ApiClient;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::IsTerminal, path::Path};
mod presentation;
mod report;
type Flags<'a> = BTreeMap<&'a str, &'a str>;
fn usage(message: impl ToString) -> Output {
    Output {
        stdout: String::new(),
        stderr: format!("{}\n", message.to_string()),
        exit: 2,
    }
}
fn owned(a: &str, b: &str) -> bool {
    matches!(
        (a, b),
        ("retention", "status")
            | ("gdpr", "export" | "erase")
            | ("compliance", "report")
            | ("opening-balance", "post")
    )
}
/// Only command words move. Attached values, including equals and leading dashes, stay literal.
pub fn prepare(args: &[String]) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let (key, attached) = crate::original_args::option(&args[i]);
        if key.starts_with("--") {
            i = if attached.is_some()
                || matches!(
                    key,
                    "--json" | "--help" | "--example" | "--allow-loopback-http"
                ) {
                i + 1
            } else {
                crate::original_args::value(args, i).map_or(i + 1, |(_, n)| n)
            };
        } else {
            words.push(i);
            i += 1;
        }
    }
    if words.len() < 2 || !owned(&args[words[0]], &args[words[1]]) {
        return None;
    }
    let mut result = vec![args[words[0]].clone(), args[words[1]].clone()];
    result.extend(
        args.iter()
            .enumerate()
            .filter(|(i, _)| !words[..2].contains(i))
            .map(|(_, s)| s.clone()),
    );
    Some(result)
}
fn parse(args: &[String]) -> Result<Flags<'_>, Output> {
    let mut f = Flags::new();
    let mut i = 2;
    while i < args.len() {
        let (key, attached) = crate::original_args::option(&args[i]);
        let common = matches!(
            key,
            "--company"
                | "--server"
                | "--local-profile"
                | "--cookie-file"
                | "--service-key-file"
                | "--allow-loopback-http"
                | "--actor"
                | "--actor-via"
                | "--json"
                | "--format"
                | "--help"
                | "--example"
        );
        let specific = match (args[0].as_str(), args[1].as_str()) {
            ("retention", _) => key == "--as-of",
            ("gdpr", "export") => {
                matches!(key, "--cvr" | "--subject" | "--name" | "--as-of" | "--out")
            }
            ("gdpr", _) => matches!(key, "--cvr" | "--subject" | "--name"),
            ("compliance", _) => matches!(key, "--out" | "--as-of" | "--as-of-instant"),
            _ => key == "--input",
        };
        if !common && !specific {
            return Err(usage(format!("Unknown argument: {}", args[i])));
        }
        let (value, next) = if matches!(
            key,
            "--json" | "--help" | "--example" | "--allow-loopback-http"
        ) {
            if attached.is_some() {
                return Err(usage(format!("Flag {key} does not take a value")));
            }
            ("true", i + 1)
        } else {
            crate::original_args::value(args, i)
                .ok_or_else(|| usage(format!("Flag {key} requires a value")))?
        };
        if key == "--format" && !matches!(value, "json" | "human") {
            return Err(usage("--format must be either json or human"));
        }
        f.insert(key, value);
        i = next;
    }
    if f.contains_key("--help") || f.contains_key("--example") {
        return Ok(f);
    }
    if args[0] == "opening-balance" && f.get("--input").is_none_or(|s| s.is_empty()) {
        return Err(usage("Missing required --input <file.json>"));
    }
    if args[0] == "compliance" && f.get("--out").is_none_or(|s| s.is_empty()) {
        return Err(usage("Missing required --out <file.html>"));
    }
    if args[0] == "compliance"
        && f.get("--as-of").is_some_and(|d| {
            d.len() != 10
                || d.as_bytes().iter().enumerate().any(|(i, b)| {
                    if i == 4 || i == 7 {
                        *b != b'-'
                    } else {
                        !b.is_ascii_digit()
                    }
                })
        })
    {
        return Err(usage("--as-of must be YYYY-MM-DD"));
    }
    if f.contains_key("--local-profile")
        && (f.contains_key("--server") || f.contains_key("--allow-loopback-http"))
    {
        return Err(usage(
            "--local-profile conflicts with remote transport flags",
        ));
    }
    Ok(f)
}
fn result(v: Value, f: &Flags<'_>, family: &str) -> Output {
    let ok = v["ok"] != false;
    let json = f.contains_key("--json")
        || f.get("--format") == Some(&"json")
        || (f.get("--format") != Some(&"human") && !std::io::stdout().is_terminal());
    let text = if json {
        format!("{}\n", serde_json::to_string_pretty(&v).expect("JSON"))
    } else {
        presentation::render(&v, family)
    };
    Output {
        stdout: if ok || json {
            text.clone()
        } else {
            String::new()
        },
        stderr: if !ok && !json { text } else { String::new() },
        exit: if ok { 0 } else { 1 },
    }
}
fn fail(message: impl ToString, f: &Flags<'_>, family: &str) -> Output {
    result(
        json!({"ok":false,"errors":[message.to_string()]}),
        f,
        family,
    )
}
fn connection(f: &Flags<'_>, company: &str) -> Result<(ApiClient, String), Output> {
    if let Some(profile) = f.get("--local-profile") {
        if !Path::new(profile).is_absolute() {
            return Err(usage("--local-profile requires absolute path"));
        }
        let local = rentemester_client::LocalRuntime::load(Path::new(profile)).map_err(usage)?;
        let company =
            rentemester_client::knowledge_company_selector(Some(&local), company).map_err(usage)?;
        local.install_process().map_err(usage)?;
        let api = crate::remote::credential_client(local.origin(), true, f, false).map_err(
            |(exit, e)| Output {
                stdout: String::new(),
                stderr: format!("{}: {}\n", e.code, e.message),
                exit,
            },
        )?;
        return Ok((api, company));
    }
    if Path::new(company).is_absolute() || company.contains('/') || company.contains('\\') {
        return Err(usage(
            "Company paths require explicit verified --local-profile",
        ));
    }
    let server = f
        .get("--server")
        .ok_or_else(|| usage("--server or --local-profile required"))?;
    let api =
        crate::remote::credential_client(server, f.contains_key("--allow-loopback-http"), f, false)
            .map_err(|(exit, e)| Output {
                stdout: String::new(),
                stderr: format!("{}: {}\n", e.code, e.message),
                exit,
            })?;
    Ok((api, company.into()))
}
pub async fn execute(args: &[String]) -> Option<Output> {
    let args = prepare(args)?;
    Some(run(&args).await)
}
async fn run(args: &[String]) -> Output {
    let f = match parse(args) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let family = if args[0] == "gdpr" && args[1] == "erase" {
        "gdpr-erase"
    } else {
        args[0].as_str()
    };
    if f.contains_key("--help") {
        return result(
            json!({"ok":true,"command":format!("{} {}",args[0],args[1]),"connection":"--server ORIGIN or --local-profile ABS_PATH; current --cookie-file or --service-key-file; optional --json/--format json|human","authority":"Common engine admission only; mutations sent once, ambiguous write remains outcome unknown."}),
            &f,
            family,
        );
    }
    if f.contains_key("--example") {
        return if family == "opening-balance" {
            result(
                json!({"cutOverDate":"2026-01-01","note":"Synthetic opening balance","lines":[{"accountNo":"2000","debitAmount":10.0},{"accountNo":"5000","creditAmount":10.0}]}),
                &f,
                family,
            )
        } else {
            usage("No example available for this command")
        };
    }
    let company = f
        .get("--company")
        .map(|s| s.to_string())
        .or_else(|| std::env::var("RENTEMESTER_COMPANY").ok())
        .unwrap_or_default();
    if company.is_empty() {
        return usage("Missing required --company <path>");
    }
    let (api, company) = match connection(&f, &company) {
        Ok(v) => v,
        Err(e) => return e,
    };
    if family == "compliance" {
        return report::execute(&api, &company, &f).await;
    }
    if family == "opening-balance" {
        if let Some(actor) = f.get("--actor") {
            if f.contains_key("--service-key-file") {
                return usage(
                    "--actor assertion requires a current human cookie; service credentials determine their own principal",
                );
            }
            let me = match api.me().await {
                Ok(v) => v,
                Err(e) => return fail(e, &f, family),
            };
            if me["user"]["id"]
                .as_str()
                .is_none_or(|id| *actor != format!("user:{id}"))
            {
                return fail("actor differs from current cookie user", &f, family);
            }
        }
        // The same immutable credential identifies every plan/apply request; server fences
        // recheck current authority in each phase. Caller actor is never outgoing authority.
        return opening(&api, &company, &f).await;
    }
    if let Some(actor) = f.get("--actor") {
        let operation = if family == "retention" {
            "retention"
        } else {
            args[1].as_str()
        };
        let mut input = json!({"expectedActor":actor});
        if operation != "retention" {
            input["confirm"] = json!(true);
        }
        for (flag, key) in [
            ("--cvr", "cvr"),
            ("--subject", "cvr"),
            ("--name", "name"),
            ("--as-of", "asOf"),
        ] {
            if let Some(v) = f.get(flag) {
                input[key] = json!(v);
            }
        }
        match api.original716_remaining(&company, operation, &input).await {
            Ok(v) => return finish(v, &f, family, &args[1]),
            Err(e) => return fail(e, &f, family),
        }
    }
    let response = match (args[0].as_str(), args[1].as_str()) {
        ("retention", _) => {
            api.retention_status_original118(&company, f.get("--as-of").copied())
                .await
        }
        ("gdpr", "export") => {
            let mut input = json!({"company":company,"confirm":true});
            if let Some(cvr) = f.get("--cvr").or_else(|| f.get("--subject")) {
                input["cvr"] = json!(cvr);
            }
            if let Some(name) = f.get("--name") {
                input["name"] = json!(name);
            }
            if let Some(date) = f.get("--as-of") {
                input["asOf"] = json!(date);
            }
            api.original717_privacy("gdpr_export", &input).await
        }
        ("gdpr", _) => {
            let mut input = json!({"confirm":true});
            if let Some(cvr) = f.get("--cvr").or_else(|| f.get("--subject")) {
                input["cvr"] = json!(cvr);
            }
            if let Some(name) = f.get("--name") {
                input["name"] = json!(name);
            }
            api.gdpr_erase(&company, serde_json::to_vec(&input).expect("JSON"), true)
                .await
                .map(|v| {
                    if v["ok"] == true {
                        v["gdprErasure"].clone()
                    } else {
                        v
                    }
                })
        }
        _ => return opening(&api, &company, &f).await,
    };
    let v = match response {
        Ok(v) => v,
        Err(e) => return fail(e, &f, family),
    };
    finish(v, &f, family, &args[1])
}
fn finish(mut v: Value, f: &Flags<'_>, family: &str, verb: &str) -> Output {
    if !v["ok"].is_boolean() {
        return fail("Malformed canonical result", &f, family);
    }
    if family == "gdpr" && verb == "export" && v["ok"] == true {
        if let Some(dir) = f.get("--out").map(|s| s.trim()).filter(|s| !s.is_empty()) {
            let raw = v["subject"]["cvr"]
                .as_str()
                .or_else(|| v["subject"]["name"].as_str())
                .unwrap_or("subject");
            let key: String = raw
                .encode_utf16()
                .map(|c| {
                    if c <= 127 && ((c as u8).is_ascii_alphanumeric() || c == 95 || c == 45) {
                        char::from(c as u8)
                    } else {
                        '_'
                    }
                })
                .collect();
            let Some(date) = v["asOf"]
                .as_str()
                .filter(|s| s.len() == 10 && s.bytes().all(|b| b.is_ascii_digit() || b == b'-'))
            else {
                return fail("Malformed export date", &f, family);
            };
            let path = Path::new(dir).join(format!("gdpr-export-{key}-{date}.json"));
            if let Err(e) = report::private_output(
                &path,
                serde_json::to_string_pretty(&v).expect("JSON").as_bytes(),
            ) {
                return fail(e, &f, family);
            }
            v["outPath"] = json!(path);
        }
    }
    result(v, &f, family)
}
async fn opening(api: &ApiClient, company: &str, f: &Flags<'_>) -> Output {
    let family = "opening-balance";
    let raw = match std::fs::read(f["--input"]) {
        Ok(v) => v,
        Err(e) => return fail(e, f, family),
    };
    let mut input: Value = match serde_json::from_slice(&raw) {
        Ok(v) => v,
        Err(e) => return fail(e, f, family),
    };
    // Transport adaptation only: reuse the existing common Money parser, never balance/post locally.
    if let Some(lines) = input["lines"].as_array_mut() {
        for line in lines {
            for key in ["debitAmount", "creditAmount"] {
                if let Some(amount) = line.get(key) {
                    let Some(number) = amount.as_number() else {
                        return fail("Opening amounts must be source kroner numbers", f, family);
                    };
                    let money = match rentemester_engine::money::Money::parse(&number.to_string()) {
                        Ok(v) => v,
                        Err(e) => return fail(e.message, f, family),
                    };
                    line[key] = json!(money.decimal());
                }
            }
        }
    }
    if input.get("createdBy").is_some() || input.get("createdByProgram").is_some() {
        return fail(
            "Opening actor fields cannot override authenticated credential authority",
            f,
            family,
        );
    }
    let plan = match api
        .opening_balance_plan(company, serde_json::to_vec(&input).expect("JSON"))
        .await
    {
        Ok(v) => v,
        Err(e) => return fail(e, f, family),
    };
    if plan["ok"] != true {
        return result(plan, f, family);
    }
    if plan["plan"]["status"] != "ready" {
        return fail(plan["plan"]["blockers"].to_string(), f, family);
    }
    let Some(hash) = plan["plan"]["planHash"].as_str() else {
        return fail("Malformed opening plan", f, family);
    };
    // Explicit original post invocation confirms this exact input once. Same plan gives the
    // same receipt key; no transport retry or local journal mutation is performed.
    let apply = json!({"input":input,"planHash":hash,"idempotencyKey":format!("original716-opening-{hash}"),"confirm":true});
    let applied = match api
        .opening_balance_apply(company, serde_json::to_vec(&apply).expect("JSON"), true)
        .await
    {
        Ok(v) => v,
        Err(e) => {
            return fail(
                format!(
                    "Opening outcome unknown or refused: {e}; inspect canonical opening and journal before another invocation"
                ),
                f,
                family,
            );
        }
    };
    if applied["ok"] != true {
        return result(applied, f, family);
    }
    let r = &applied["openingBalance"];
    if !r["journalEntryId"].is_number()
        || !r["journalEntryNo"].is_string()
        || !r["journalEntryHash"].is_string()
    {
        return fail(
            "Opening outcome unknown: malformed committed receipt; inspect canonical opening and journal before another invocation",
            f,
            family,
        );
    }
    let mut receipt = json!({"ok":true,"entryId":r["journalEntryId"],"entryNo":r["journalEntryNo"],"entryHash":r["journalEntryHash"],"cutOverDate":r["cutOverDate"],"errors":[]});
    // Only forward an actual canonical rule union; never manufacture a balanced-only list.
    if let Some(rules) = applied["appliedRules"]
        .as_array()
        .filter(|a| a.iter().all(Value::is_string))
    {
        receipt["appliedRules"] = json!(rules);
    }
    result(receipt, f, family)
}
