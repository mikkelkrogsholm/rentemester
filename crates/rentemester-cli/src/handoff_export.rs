//! Original explicit export invocation confirms one audited server export, then writes verified local files.
use crate::dated_reports::Output;
use rentemester_client::{Error, write_handoff_package};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};
fn fail(message: &str, exit: u8, json_mode: bool) -> Output {
    if json_mode && exit == 1 {
        Output {
            stdout: format!(
                "{}\n",
                serde_json::to_string_pretty(&json!({"ok":false,"errors":[message]})).unwrap()
            ),
            stderr: String::new(),
            exit,
        }
    } else {
        Output {
            stdout: String::new(),
            stderr: format!("{message}\n"),
            exit,
        }
    }
}
pub fn help() -> Value {
    json!({"usage":"system export-authority|export-accountant --server ORIGIN --cookie-file PATH --company SLUG --from YYYY-MM-DD --to YYYY-MM-DD --out DIR [--requested-at ISO-8601] [--requester NAME] [--json] [--allow-loopback-http]","effects":"Explicit export invocation confirms one audited server export. Verified files are placed in a fresh local DIR/package-root without overwrite; existing siblings remain unchanged. Local failure can follow committed export audit; never automatically retry. Company filesystem/default launcher compatibility remains pending R44. No filing, submission or signed-backup claim."})
}
struct Input<'a> {
    profile: &'static str,
    flags: BTreeMap<&'a str, &'a str>,
    json: bool,
    http: bool,
}
fn parse(args: &[String]) -> Result<Input<'_>, String> {
    let profile = match args.first().map(String::as_str) {
        Some("export-authority") => "authority",
        Some("export-accountant") => "accountant_handoff",
        _ => return Err("Expected system export-authority|export-accountant".into()),
    };
    let mut flags = BTreeMap::new();
    let (mut json, mut http) = (false, false);
    let mut cursor = 1;
    while cursor < args.len() {
        let (key, attached) = crate::original_args::option(&args[cursor]);
        match key {
            "--json" if attached.is_none() && !json => {
                json = true;
                cursor += 1;
            }
            "--allow-loopback-http" if attached.is_none() && !http => {
                http = true;
                cursor += 1;
            }
            "--server" | "--cookie-file" | "--company" | "--from" | "--to" | "--out"
            | "--requested-at" | "--requester" => {
                let (value, next) = crate::original_args::value(args, cursor)
                    .filter(|(value, _)| !value.is_empty())
                    .ok_or_else(|| format!("Missing value for {key}"))?;
                if flags.insert(key, value).is_some() {
                    return Err(format!("Duplicate flag {key}"));
                }
                cursor = next;
            }
            _ => return Err(format!("Unknown or duplicate export flag {key}")),
        }
    }
    if [
        "--server",
        "--cookie-file",
        "--company",
        "--from",
        "--to",
        "--out",
    ]
    .iter()
    .any(|key| !flags.contains_key(key))
    {
        return Err(
            "Missing required --server, --cookie-file, --company, --from, --to, or --out".into(),
        );
    }
    Ok(Input {
        profile,
        flags,
        json,
        http,
    })
}
pub fn execute(args: &[String]) -> Output {
    if args
        .iter()
        .skip(1)
        .take_while(|s| s.as_str() != "--")
        .any(|s| s == "--help")
    {
        return Output {
            stdout: format!("{}\n", serde_json::to_string_pretty(&help()).unwrap()),
            stderr: String::new(),
            exit: 0,
        };
    }
    let input = match parse(args) {
        Ok(v) => v,
        Err(e) => return fail(&e, 2, false),
    };
    let company = input.flags["--company"];
    if !company
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
    {
        return fail(
            "Company must be a connected-server slug; filesystem/default company launcher compatibility remains pending R44",
            1,
            input.json,
        );
    }
    let api = match crate::remote::credential_client(
        input.flags["--server"],
        input.http,
        &input.flags,
        false,
    ) {
        Ok(api) => api,
        Err((code, e)) => return fail(&e.message, code, input.json),
    };
    let mut body =
        json!({"periodStart":input.flags["--from"],"periodEnd":input.flags["--to"],"confirm":true});
    for (flag, key) in [
        ("--requested-at", "requestedAt"),
        ("--requester", "requester"),
    ] {
        if let Some(value) = input.flags.get(flag) {
            body[key] = json!(value);
        }
    }
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(_) => return fail("could not start HTTP runtime", 1, input.json),
    };
    let downloaded = match runtime.block_on(api.handoff_export(
        company,
        input.profile,
        serde_json::to_vec(&body).unwrap(),
        true,
    )) {
        Ok(v) => v,
        Err(e) => return export_failure(e, input.json),
    };
    let result = match write_handoff_package(&downloaded, Path::new(input.flags["--out"])) {
        Ok(v) => v,
        Err(e) => {
            return fail(
                &format!(
                    "{e}. Server export audit may already be committed; do not automatically export again."
                ),
                1,
                input.json,
            );
        }
    };
    let stdout = if input.json {
        serde_json::to_string_pretty(&result).unwrap()
    } else {
        format!(
            "✔ {}\n  Eksportmappe: {}\n  Manifest: {}",
            if input.profile == "authority" {
                "Myndighedseksport"
            } else {
                "Revisor-eksport"
            },
            result["exportDir"].as_str().unwrap_or("—"),
            result["manifestPath"].as_str().unwrap_or("—")
        )
    };
    Output {
        stdout: stdout + "\n",
        stderr: String::new(),
        exit: 0,
    }
}
fn export_failure(error: Error, json_mode: bool) -> Output {
    fail(
        &format!("{error}. No automatic retry: a lost export response can follow an audit commit."),
        1,
        json_mode,
    )
}
