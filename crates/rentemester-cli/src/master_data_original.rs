//! Original six customer/vendor commands over the shared authenticated SDK.
use crate::dated_reports::Output;
use rentemester_client::MasterDataOriginalKind;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::IsTerminal};
pub fn accepts(args: &[String]) -> bool {
    matches!(
        (
            args.first().map(String::as_str),
            args.get(1).map(String::as_str)
        ),
        (
            Some("customer"),
            Some("create" | "list" | "validate-vat" | "cvr-lookup")
        ) | (Some("vendor"), Some("create" | "list"))
    )
}
pub fn help() -> Value {
    json!({"commands":["customer create [--name TEXT] [--address TEXT] [--cvr TEXT] [--email TEXT] [--ean TEXT] [--payment-terms DAYS] [--currency ISO] [--notes TEXT] [--from-cvr CVR]","customer list [--archived]","customer validate-vat --cvr EU-VAT","customer cvr-lookup --cvr CVR-nummer","vendor create [--name TEXT] [--address TEXT] [--cvr TEXT] [--country ISO] [--identifier-kind dk_cvr|eu_vat|non_eu] [--expense-account TEXT] [--default-vat TEXT] [--notes TEXT] [--from-cvr CVR]","vendor list [--archived]"],"connection":"--company SLUG; --server ORIGIN or --local-profile PATH; --cookie-file PATH or --service-key-file PATH; --allow-loopback-http; --json or --format json|human","authority":"Create and validate-vat invocation confirms one operation; server-owned identity, no caller actor, stdin, extra confirmation, idempotency key or retry."})
}
fn usage(s: &str) -> Output {
    Output {
        stdout: String::new(),
        stderr: format!("{s}\n"),
        exit: 2,
    }
}
fn parse(args: &[String]) -> Result<BTreeMap<&str, &str>, Output> {
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
                | "--format"
                | "--json"
                | "--help"
                | "--allow-loopback-http"
        );
        let domain = match args[1].as_str() {
            "list" => key == "--archived",
            "create" => {
                matches!(
                    key,
                    "--name" | "--address" | "--cvr" | "--notes" | "--from-cvr"
                ) || if args[0] == "customer" {
                    matches!(key, "--email" | "--ean" | "--payment-terms" | "--currency")
                } else {
                    matches!(
                        key,
                        "--country" | "--identifier-kind" | "--expense-account" | "--default-vat"
                    )
                }
            }
            _ => key == "--cvr",
        };
        if !global && !domain {
            return Err(usage(&format!("Unknown master-data flag {key}")));
        }
        let boolean = matches!(
            key,
            "--json" | "--help" | "--allow-loopback-http" | "--archived"
        );
        let (raw, next) = if boolean {
            if attached.is_some() {
                return Err(usage("Boolean flag does not take a value"));
            }
            ("true", cursor + 1)
        } else {
            crate::original_args::value(args, cursor).ok_or_else(|| usage("Missing flag value"))?
        };
        if flags.insert(key, raw).is_some() {
            return Err(usage("Duplicate master-data flag"));
        }
        cursor = next;
    }
    if flags
        .get("--format")
        .is_some_and(|s| !matches!(*s, "human" | "json"))
    {
        return Err(usage("--format must be json or human"));
    }
    if !flags.contains_key("--help")
        && matches!(args[1].as_str(), "validate-vat" | "cvr-lookup")
        && flags.get("--cvr").is_none_or(|s| s.is_empty())
    {
        return Err(usage(if args[1] == "validate-vat" {
            "Missing required --cvr <EU-VAT>"
        } else {
            "Missing required --cvr <CVR-nummer>"
        }));
    }
    Ok(flags)
}
pub fn preflight(args: &[String]) -> Result<(), Output> {
    parse(args).map(|_| ())
}
fn label(args: &[String]) -> &'static str {
    match (args[0].as_str(), args[1].as_str()) {
        ("customer", "create") => "Opretter en append-only kundepost til genbrug på fakturaer.",
        ("vendor", "create") => "Opretter en append-only leverandørpost til bilagsindlæsning.",
        (_, "validate-vat") => "Validerer et EU-VAT-nummer via VIES og cacher resultatet.",
        _ => {
            "Slår en virksomhed op i CVR-registret og viser stamdata (navn, adresse, branche, form, status, ledelse). Read-only; cacher snapshottet. Kræver CVR_USERNAME/CVR_PASSWORD."
        }
    }
}
fn scalar(v: &Value) -> Option<String> {
    match v {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}
fn field_label(k: &str) -> String {
    match k {
        "name" => "Navn",
        "cvr" => "CVR",
        "email" => "E-mail",
        "address" => "Adresse",
        "postalCode" => "Postnummer",
        "city" => "By",
        "industryText" => "Branche",
        "auditWaived" => "Revision fravalgt",
        "fiscalYearStartMonth" => "Regnskabsår starter (måned)",
        "customerId" => "Kunde-id",
        "vendorId" => "Leverandør-id",
        _ => return humanize(k),
    }
    .into()
}
fn humanize(k: &str) -> String {
    let mut s = String::new();
    for (i, c) in k.chars().enumerate() {
        if i == 0 {
            s.extend(c.to_uppercase());
        } else if c.is_uppercase() {
            s.push(' ');
            s.push(c);
        } else {
            s.push(c);
        }
    }
    s
}
fn human(value: &Value, args: &[String]) -> String {
    if args[1] == "list" {
        let rows = value["rows"].as_array().unwrap();
        let customer = args[0] == "customer";
        let mut lines = vec![format!(
            "{} ({})",
            if customer { "Kunder" } else { "Leverandører" },
            rows.len()
        )];
        if rows.is_empty() {
            lines.push(
                if customer {
                    "Ingen kunder registreret."
                } else {
                    "Ingen leverandører registreret."
                }
                .into(),
            );
        }
        for r in rows {
            lines.push(String::new());
            lines.push(format!("#{} — {}", r["id"], r["name"].as_str().unwrap()));
            for (k, l) in [
                ("vatOrCvr", "CVR/moms-nr."),
                ("address", "Adresse"),
                ("email", "E-mail"),
                ("phone", "Telefon"),
                ("eanNumber", "EAN-nummer"),
                ("paymentTermsDays", "Betalingsbetingelser"),
                ("defaultCurrency", "Standardvaluta"),
                ("defaultExpenseAccount", "Standard omkostningskonto"),
                ("defaultVatTreatment", "Standard momsbehandling"),
                ("notes", "Noter"),
            ] {
                if let Some(s) = scalar(&r[k]) {
                    lines.push(format!(
                        "  {l}: {s}{}",
                        if k == "paymentTermsDays" { " dage" } else { "" }
                    ));
                }
            }
            if r["archived"] == true {
                lines.push("  (arkiveret)".into());
            }
        }
        return lines.join("\n");
    }
    let short = label(args).split(". ").next().unwrap().trim();
    let short = if short.chars().count() > 60 {
        format!("{}…", short.chars().take(57).collect::<String>().trim_end())
    } else {
        short.into()
    };
    let mut lines = vec![format!("✔ {short}")];
    let mut seen = std::collections::BTreeSet::new();
    for k in ["customerId", "vendorId"] {
        if let Some(s) = scalar(&value[k]) {
            lines.push(format!("  {}: {s}", field_label(k)));
            seen.insert(k.to_owned());
        }
    }
    if let Some(o) = value["company"].as_object() {
        for (k, v) in o {
            if !seen.contains(k) {
                if let Some(s) = scalar(v) {
                    lines.push(format!("  {}: {s}", field_label(k)));
                }
            }
        }
    }
    lines.join("\n")
}
fn output(v: Value, json_mode: bool, exit: u8, args: &[String]) -> Output {
    if exit == 2 {
        return usage(
            &v["errors"]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| e.as_str().unwrap())
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }
    if json_mode {
        return Output {
            stdout: format!("{}\n", serde_json::to_string_pretty(&v).unwrap()),
            stderr: String::new(),
            exit,
        };
    }
    if exit == 0 {
        Output {
            stdout: format!("{}\n", human(&v, args)),
            stderr: String::new(),
            exit,
        }
    } else {
        let mut message = if args[1] == "list" {
            format!(
                "✘ {} failed\n",
                if args[0] == "customer" {
                    "Kunder"
                } else {
                    "Leverandører"
                }
            )
        } else {
            format!("✘ {} failed\n", label(args))
        };
        for e in v["errors"].as_array().unwrap() {
            message.push_str(&format!("  → {}\n", e.as_str().unwrap()));
        }
        Output {
            stdout: String::new(),
            stderr: message,
            exit,
        }
    }
}
// JavaScript Number: nonfinite values are omitted; finite invalid values remain business failures.
fn source_number(s: &str) -> f64 {
    let s = s.trim();
    if s.is_empty() {
        return 0.0;
    }
    for (prefix, radix) in [
        ("0x", 16),
        ("0X", 16),
        ("0b", 2),
        ("0B", 2),
        ("0o", 8),
        ("0O", 8),
    ] {
        if let Some(v) = s.strip_prefix(prefix) {
            return u64::from_str_radix(v, radix).map_or(f64::NAN, |n| n as f64);
        }
    }
    s.parse().unwrap_or(f64::NAN)
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
    for key in ["--company", "--server"] {
        if flags.get(key).is_none_or(|s| s.is_empty()) {
            return usage(&format!("Explicit {key} required"));
        }
    }
    if flags.contains_key("--cookie-file") == flags.contains_key("--service-key-file") {
        return usage("Choose exactly one --cookie-file/--service-key-file");
    }
    let json_mode = flags.contains_key("--json")
        || flags.get("--format") == Some(&"json")
        || (flags.get("--format") != Some(&"human") && !std::io::stdout().is_terminal());
    let kind = if args[0] == "customer" {
        MasterDataOriginalKind::Customer
    } else {
        MasterDataOriginalKind::Vendor
    };
    let mut body = json!({"input":{},"confirm":true});
    if args[1] == "create" {
        for (flag, key) in [
            ("--name", "name"),
            ("--address", "address"),
            ("--cvr", "vatOrCvr"),
            ("--email", "email"),
            ("--ean", "eanNumber"),
            ("--currency", "defaultCurrency"),
            ("--notes", "notes"),
            ("--country", "countryCode"),
            ("--identifier-kind", "identifierKind"),
            ("--expense-account", "defaultExpenseAccount"),
            ("--default-vat", "defaultVatTreatment"),
        ] {
            if let Some(s) = flags.get(flag) {
                if key != "name" || !s.is_empty() {
                    body["input"][key] = json!(s);
                }
            }
        }
        if let Some(s) = flags.get("--payment-terms") {
            let n = source_number(s);
            if n.is_finite() {
                if n <= 0.0 || n.fract() != 0.0 || n >= u64::MAX as f64 {
                    return output(
                        json!({"ok":false,"errors":["paymentTermsDays must be a positive integer"]}),
                        json_mode,
                        1,
                        args,
                    );
                }
                body["input"]["paymentTermsDays"] = json!(n as u64);
            }
        }
        if let Some(s) = flags.get("--from-cvr").filter(|s| !s.is_empty()) {
            body["fromCvr"] = json!(s);
        }
    }
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
                args,
            );
        }
    };
    let rt = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(_) => {
            return output(
                json!({"ok":false,"errors":["HTTP runtime unavailable"]}),
                json_mode,
                1,
                args,
            );
        }
    };
    let result = rt.block_on(async {
        match args[1].as_str() {
            "list" => {
                api.master_data_original_list(
                    flags["--company"],
                    kind,
                    flags.contains_key("--archived"),
                )
                .await
            }
            "create" => {
                api.master_data_original_create(
                    flags["--company"],
                    kind,
                    serde_json::to_vec(&body).unwrap(),
                    true,
                )
                .await
            }
            "validate-vat" => {
                api.customer_original_validate_vat(flags["--company"], flags["--cvr"], true)
                    .await
            }
            _ => {
                api.master_data_original_cvr_lookup(flags["--company"], flags["--cvr"])
                    .await
            }
        }
    });
    match result {
        Ok(v) => {
            let exit = if v["ok"] == false { 1 } else { 0 };
            output(v, json_mode, exit, args)
        }
        Err(e) => {
            let (exit, e) = crate::remote::client_error(e);
            output(
                json!({"ok":false,"code":e.code,"errors":[e.message]}),
                json_mode,
                exit,
                args,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| (*s).into()).collect()
    }
    #[test]
    fn master_data_original_source_human_list_and_generic_vat() {
        let value = json!({"rows":[{"id":3,"name":"Synthetic","vatOrCvr":"DK12345678","address":"Street","email":"a@example.invalid","phone":"123","eanNumber":"1234","paymentTermsDays":20,"defaultCurrency":"DKK","notes":"Note","archived":true}]});
        assert_eq!(
            human(&value, &args(&["customer", "list"])),
            "Kunder (1)\n\n#3 — Synthetic\n  CVR/moms-nr.: DK12345678\n  Adresse: Street\n  E-mail: a@example.invalid\n  Telefon: 123\n  EAN-nummer: 1234\n  Betalingsbetingelser: 20 dage\n  Standardvaluta: DKK\n  Noter: Note\n  (arkiveret)"
        );
        assert_eq!(
            human(
                &json!({"ok":true,"validation":{"valid":false}}),
                &args(&["customer", "validate-vat"])
            ),
            "✔ Validerer et EU-VAT-nummer via VIES og cacher resultatet."
        );
        assert_eq!(
            human(
                &json!({"ok":true,"customerId":7}),
                &args(&["customer", "create"])
            ),
            "✔ Opretter en append-only kundepost til genbrug på fakturaer.\n  Kunde-id: 7"
        );
    }
    #[test]
    fn master_data_original_source_numbers_and_explicit_format_intent() {
        assert_eq!(source_number(""), 0.0);
        assert_eq!(source_number(" 0x10 "), 16.0);
        assert!(source_number("unknown").is_nan());
        assert!(!source_number("Infinity").is_finite());
        for root in ["customer", "vendor"] {
            assert_eq!(
                crate::original_args::prepare(args(&["--format=human", root, "list"])).unwrap(),
                args(&[root, "list", "--format", "human"])
            );
            assert_eq!(
                crate::original_args::prepare(args(&["--json", root, "list", "--format=human"]))
                    .unwrap(),
                args(&[root, "list", "--json"])
            );
            assert!(crate::original_args::requires_company(&args(&[
                root, "list"
            ])));
        }
    }
}
