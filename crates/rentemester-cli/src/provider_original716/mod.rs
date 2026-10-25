//! Source CLI boundary only: provider I/O and admission remain in the common server.
use crate::dated_reports::Output;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::IsTerminal, path::Path};
type Flags<'a> = BTreeMap<&'a str, &'a str>;
fn specs() -> Vec<Value> {
    serde_json::from_str(include_str!("specs.json")).expect("frozen source specs")
}
fn spec(family: &str, command: &str) -> Option<Value> {
    let key = format!("{family} {command}");
    specs().into_iter().find(|s| s["key"] == key)
}
fn usage(s: &str) -> Output {
    Output {
        stdout: String::new(),
        stderr: format!("{s}\n"),
        exit: 2,
    }
}
/// Move only the two owned command words; never split or rewrite a literal flag value.
pub fn prepare(args: &[String]) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let (k, a) = crate::original_args::option(&args[i]);
        if k == "--" {
            break;
        }
        if k.starts_with("--") {
            i = if a.is_some()
                || matches!(k, "--json" | "--help" | "--force" | "--allow-loopback-http")
            {
                i + 1
            } else {
                crate::original_args::value(args, i).map_or(i + 1, |(_, n)| n)
            };
        } else {
            words.push(i);
            i += 1;
        }
    }
    if words.len() < 2 || spec(&args[words[0]], &args[words[1]]).is_none() {
        return None;
    }
    let mut out = vec![args[words[0]].clone(), args[words[1]].clone()];
    out.extend(
        args.iter()
            .enumerate()
            .filter(|(i, _)| !words[..2].contains(i))
            .map(|(_, s)| s.clone()),
    );
    Some(out)
}
fn parse<'a>(args: &'a [String], s: &Value) -> Result<Flags<'a>, Output> {
    let mut f = Flags::new();
    let mut i = 2;
    while i < args.len() {
        let (k, a) = crate::original_args::option(&args[i]);
        let allowed = s["allowedFlags"]
            .as_array()
            .is_some_and(|v| v.iter().any(|v| v == k));
        if !allowed
            && !matches!(
                k,
                "--server"
                    | "--local-profile"
                    | "--cookie-file"
                    | "--service-key-file"
                    | "--allow-loopback-http"
                    | "--actor"
                    | "--actor-via"
                    | "--json"
                    | "--format"
                    | "--help"
            )
            && !(args[0] == "efaktura" && args[1] == "konfigurer" && k == "--api-license-key-file")
        {
            return Err(usage(&format!("Unknown flag {k}")));
        }
        let (v, n) = if matches!(k, "--json" | "--help" | "--force" | "--allow-loopback-http") {
            if a.is_some() {
                return Err(usage(&format!("Flag {k} does not take a value")));
            }
            if args.get(i + 1).is_some_and(|v| !v.starts_with("--")) {
                return Err(usage(&format!("Flag {k} does not take a value")));
            }
            ("true", i + 1)
        } else {
            crate::original_args::value(args, i)
                .ok_or_else(|| usage(&format!("Flag {k} requires a value")))?
        };
        f.insert(k, v);
        i = n;
    }
    if f.get("--format")
        .is_some_and(|v| !matches!(*v, "json" | "human"))
    {
        return Err(usage("--format must be json or human"));
    }
    if f.contains_key("--local-profile")
        && (f.contains_key("--server") || f.contains_key("--allow-loopback-http"))
    {
        return Err(usage(
            "--local-profile conflicts with remote transport flags",
        ));
    }
    if f.contains_key("--cookie-file") && f.contains_key("--service-key-file") {
        return Err(usage("Choose exactly one credential file"));
    }
    Ok(f)
}
fn render(s: &Value, v: Value, f: &Flags, exit: Option<u8>) -> Output {
    let ok = v["ok"] != false;
    let exit = exit.unwrap_or(if ok { 0 } else { 1 });
    let json = f.contains_key("--json")
        || f.get("--format") == Some(&"json")
        || (f.get("--format") != Some(&"human") && !std::io::stdout().is_terminal());
    if json {
        return Output {
            stdout: format!("{}\n", serde_json::to_string_pretty(&v).unwrap()),
            stderr: String::new(),
            exit,
        };
    }
    let label = s["description"].as_str().unwrap_or("");
    let text = if ok {
        let first = label
            .split(" — ")
            .next()
            .unwrap_or(label)
            .split(". ")
            .next()
            .unwrap_or(label);
        let title = if first.chars().count() > 60 {
            format!("{}…", first.chars().take(57).collect::<String>().trim_end())
        } else {
            first.to_owned()
        };
        let mut lines = vec![format!("✔ {title}")];
        for (k, l) in [
            ("message", "Besked"),
            ("invoiceNumber", "Fakturanummer"),
            ("documentId", "Bilags-id"),
        ] {
            if let Some(v) = v
                .get(k)
                .filter(|v| v.is_string() || v.is_number() || v.is_boolean())
            {
                lines.push(format!(
                    "  {l}: {}",
                    v.as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| v.to_string())
                ));
            }
        }
        lines.join("\n")
    } else {
        let mut lines = vec![format!("✘ {label} failed")];
        for e in v["errors"].as_array().into_iter().flatten() {
            lines.push(format!("  → {}", e.as_str().unwrap_or("")));
        }
        lines.join("\n")
    };
    if ok {
        Output {
            stdout: format!("{text}\n"),
            stderr: String::new(),
            exit,
        }
    } else {
        Output {
            stdout: String::new(),
            stderr: format!("{text}\n"),
            exit,
        }
    }
}
fn refusal(s: &Value, f: &Flags, message: &str, exit: u8) -> Output {
    render(s, json!({"ok":false,"errors":[message]}), f, Some(exit))
}
fn error(e: rentemester_client::Error) -> Output {
    let (exit, e) = crate::remote::client_error(e);
    Output {
        stdout: String::new(),
        stderr: format!("{}: {}\n", e.code, e.message),
        exit,
    }
}
fn number(raw: &str) -> Option<f64> {
    let s = raw.trim();
    if s.is_empty() {
        return Some(0.);
    }
    let n = if let Some(s) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(s, 16).ok()? as f64
    } else if let Some(s) = s.strip_prefix("0b").or_else(|| s.strip_prefix("0B")) {
        u64::from_str_radix(s, 2).ok()? as f64
    } else if let Some(s) = s.strip_prefix("0o").or_else(|| s.strip_prefix("0O")) {
        u64::from_str_radix(s, 8).ok()? as f64
    } else {
        s.parse().ok()?
    };
    n.is_finite().then_some(n)
}
fn positive(raw: &str) -> Option<u64> {
    number(raw)
        .filter(|n| n.fract() == 0. && *n > 0. && *n <= 9_007_199_254_740_991.)
        .map(|n| n as u64)
}
/// All writes are sent once; no fallback transport, engine, or automatic retries.
pub async fn execute(args: &[String]) -> Option<Output> {
    let s = spec(args.first()?, args.get(1)?)?;
    Some(run(args, &s).await)
}
async fn run(args: &[String], s: &Value) -> Output {
    let f = match parse(args, s) {
        Ok(f) => f,
        Err(o) => return o,
    };
    if f.contains_key("--help") {
        return Output {
            stdout: format!("{}\n", serde_json::to_string_pretty(s).unwrap()),
            stderr: String::new(),
            exit: 0,
        };
    }
    let family = args[0].as_str();
    let cmd = args[1].as_str();
    let get = |k| f.get(k).copied();
    // Preserve real source valued-confirm gates, including the two case-sensitive test gates.
    let confirmation = if family == "efaktura" {
        match cmd {
            "konfigurer" => Some("--confirm yes required to save Digisense API credentials"),
            "registrer" => {
                Some("--confirm yes required to register the company in NemHandel via Digisense")
            }
            "registrer-test-gln" => {
                Some("--confirm yes required to register the Digisense test GLN")
            }
            "registrer-test-afsender" => {
                Some("--confirm yes required to register the Digisense test sender")
            }
            "modtag" => {
                Some("--confirm yes required to poll and ingest received e-invoices via Digisense")
            }
            "modtag-workspace" => {
                Some("--confirm yes required to poll workspace DigiSense inbound")
            }
            "onboard" => Some("--confirm yes required to onboard DigiSense"),
            "status" | "leveringsstatus" => {
                Some("--confirm yes required to record Digisense delivery status evidence")
            }
            _ => None,
        }
    } else {
        None
    };
    if cmd == "registrer"
        && ["--cvr", "--company-name"]
            .iter()
            .any(|k| get(k).is_none_or(|v| v.trim().is_empty()))
    {
        return refusal(
            s,
            &f,
            "Missing required --cvr <DKxxxxxxxx> and --company-name <text>",
            2,
        );
    }
    if let Some(message) = confirmation {
        let raw = get("--confirm").unwrap_or("").trim();
        let yes = if matches!(cmd, "registrer-test-gln" | "registrer-test-afsender") {
            raw == "yes"
        } else {
            raw.eq_ignore_ascii_case("yes")
        };
        if !yes {
            return refusal(s, &f, message, 1);
        }
    }
    let mut request = json!({"confirm":true});
    if family == "invoice" {
        let kind = get("--kind").unwrap_or("invoice").trim();
        if !matches!(kind, "invoice" | "reminder") {
            return usage("--kind must be either 'invoice' or 'reminder'");
        }
        request["kind"] = json!(kind);
        if let Some(id) = get("--document-id").and_then(positive) {
            request["documentId"] = json!(id);
        } else if let Some(n) = get("--invoice-number")
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            request["invoiceNumber"] = json!(n);
        }
        if let Some(to) = get("--to") {
            request["to"] = json!(to);
        }
    }
    if family == "mail-intake" {
        let Some(source) = get("--source").filter(|s| !s.is_empty()) else {
            return usage("Missing required --source <eml-file-or-maildrop-dir>");
        };
        request["source"] = json!(source);
    }
    if cmd == "modtag-workspace" {
        let Some(workspace) = get("--workspace").map(str::trim).filter(|v| !v.is_empty()) else {
            return refusal(s, &f, "Missing required --workspace <dir>", 2);
        };
        request["workspace"] = json!(workspace);
    }
    if matches!(cmd, "status" | "leveringsstatus") {
        let Some(id) = get("--document-id").and_then(positive) else {
            return refusal(
                s,
                &f,
                "Missing required --document-id <positive integer>",
                2,
            );
        };
        request["documentId"] = json!(id);
    }
    if cmd == "konfigurer" {
        let secret = if let Some(path) = get("--api-license-key-file") {
            if get("--api-license-key").is_some() {
                return usage("Choose --api-license-key or --api-license-key-file");
            }
            match rentemester_client::read_service_key_file(Path::new(path)) {
                Ok(v) => v,
                Err(e) => return error(e),
            }
        } else {
            get("--api-license-key").unwrap_or("").trim().to_owned()
        };
        if secret.is_empty() {
            return refusal(s, &f, "Missing required --api-license-key <secret>", 2);
        }
        let env = get("--environment").unwrap_or("test").trim().to_lowercase();
        if !matches!(env.as_str(), "test" | "production") {
            return refusal(
                s,
                &f,
                "--environment must be 'production' or 'test' (default 'test')",
                1,
            );
        }
        request["apiLicenseKey"] = json!(secret);
        request["environment"] = json!(env);
    }
    if cmd == "registrer" {
        request["cvr"] = json!(get("--cvr").unwrap().trim());
        request["companyName"] = json!(get("--company-name").unwrap().trim());
        if let Some(n) = get("--network")
            .map(str::trim)
            .filter(|s| matches!(*s, "nemhandel" | "peppol"))
        {
            request["network"] = json!(n);
        }
    }
    if cmd == "registrer-test-gln"
        && get("--network").is_some_and(|v| !matches!(v.trim(), "nemhandel" | "peppol"))
    {
        return refusal(s, &f, "--network must be nemhandel or peppol", 1);
    }
    for (flag, key) in [
        ("--limit", "limit"),
        ("--since-uid", "sinceUid"),
        ("--imap-port", "imapPort"),
    ] {
        if let Some(raw) = get(flag) {
            let Some(n) = number(raw) else {
                return refusal(s, &f, &format!("{flag} must be a number"), 1);
            };
            request[key] = if n.fract() == 0. && n >= i64::MIN as f64 && n <= i64::MAX as f64 {
                json!(n as i64)
            } else {
                json!(n)
            };
        }
    }
    for (flag, key) in [
        ("--digisense-company-key", "digisenseCompanyKey"),
        ("--max-timestamp", "maxTimestamp"),
    ] {
        if let Some(v) = get(flag).map(str::trim).filter(|s| !s.is_empty()) {
            request[key] = json!(v);
        }
    }
    if matches!(family, "mail-intake" | "imap-intake") || cmd == "modtag" {
        request["force"] = json!(f.contains_key("--force"));
    }
    if let Some(path) = get("--metadata") {
        let raw = std::fs::read(path)
            .map_err(|e| e.to_string())
            .and_then(|b| serde_json::from_slice::<Value>(&b).map_err(|e| e.to_string()));
        let value = match raw {
            Ok(v) => v,
            Err(e) => return refusal(s, &f, &format!("could not read --metadata {path}: {e}"), 1),
        };
        let per = family != "efaktura"
            && value.as_object().is_some_and(|o| {
                !o.is_empty() && o.keys().all(|k| k.starts_with('<') && k.ends_with('>'))
            });
        request[if per {
            "metadataPerMessage"
        } else {
            "metadata"
        }] = value;
    }
    if family == "imap-intake" {
        for (flag, key, env) in [
            ("--imap-host", "imapHost", "RENTEMESTER_IMAP_HOST"),
            (
                "--imap-username",
                "imapUsername",
                "RENTEMESTER_IMAP_USERNAME",
            ),
            ("--imap-mailbox", "imapMailbox", "RENTEMESTER_IMAP_MAILBOX"),
        ] {
            if let Some(v) = get(flag)
                .map(str::to_owned)
                .or_else(|| std::env::var(env).ok())
            {
                request[key] = json!(v);
            }
        }
    }
    let company = get("--company")
        .map(str::to_owned)
        .or_else(|| std::env::var("RENTEMESTER_COMPANY").ok());
    if cmd != "modtag-workspace" && company.as_ref().is_none_or(|v| v.is_empty()) {
        return usage("Missing required --company <slug>");
    }
    let mut origin = None;
    if let Some(path) = get("--local-profile") {
        let local = match rentemester_client::LocalRuntime::load(Path::new(path)) {
            Ok(v) => v,
            Err(e) => return error(e),
        };
        origin = Some(local.origin().to_owned());
        if let Err(e) = local.install_process() {
            return error(e);
        }
    }
    let Some(server) = origin.as_deref().or_else(|| get("--server")) else {
        return usage("--server or --local-profile required");
    };
    let api = match crate::remote::credential_client(
        server,
        origin.is_some() || f.contains_key("--allow-loopback-http"),
        &f,
        false,
    ) {
        Ok(v) => v,
        Err((exit, e)) => {
            return Output {
                stdout: String::new(),
                stderr: format!("{}: {}\n", e.code, e.message),
                exit,
            };
        }
    };
    if let Some(actor) = get("--actor") {
        if f.contains_key("--service-key-file") {
            return usage(
                "--actor assertion requires a current human cookie; service credentials determine their own principal",
            );
        }
        let me = match api.me().await {
            Ok(v) => v,
            Err(e) => return error(e),
        };
        if me["user"]["id"]
            .as_str()
            .is_none_or(|id| actor != format!("user:{id}"))
        {
            return refusal(s, &f, "actor differs from current cookie user", 1);
        }
    }
    if let Some(company) = company.as_deref() {
        request["company"] = json!(company);
    }
    let result = match (family, cmd) {
        ("efaktura", "registrer-test-gln" | "registrer-test-afsender") => {
            api.original716_test_registration(cmd, &request).await
        }
        ("efaktura", "konfigurer") => api.original717("efaktura_konfigurer", &request).await,
        ("mail-intake", _) => api.original716_intake("mail_intake_ingest", &request).await,
        ("imap-intake", _) => api.original716_intake("imap_intake_poll", &request).await,
        ("invoice", _) => api.original717("invoice_send_email", &request).await,
        ("efaktura", "status" | "leveringsstatus") => {
            let company = match api.original717_company_selector(company.as_deref().unwrap()) {
                Ok(v) => v,
                Err(e) => return error(e),
            };
            request.as_object_mut().unwrap().remove("company");
            if let Some(key) = request
                .as_object_mut()
                .unwrap()
                .remove("digisenseCompanyKey")
            {
                request["companyKey"] = key;
            }
            api.invoice_public_original_status(&company, serde_json::to_vec(&request).unwrap())
                .await
        }
        _ => {
            let name = format!("efaktura_{}", cmd.replace('-', "_"));
            api.original717_digisense(&name, &request).await
        }
    };
    match result {
        Ok(v) => {
            let exit = if matches!(cmd, "status" | "leveringsstatus")
                && matches!(v["status"].as_str(), Some("failed" | "uncertain"))
            {
                Some(1)
            } else {
                None
            };
            render(s, v, &f, exit)
        }
        Err(e) => error(e),
    }
}
