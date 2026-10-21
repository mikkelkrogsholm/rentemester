//! Original annual/tax commands over the shared server; local XHTML publication is explicit.
use crate::dated_reports::Output;
use rentemester_client::{ApiClient, Error};
use rentemester_engine::money::Money;
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs::OpenOptions, io::Write, path::Path};
fn fail(message: impl AsRef<str>, exit: u8) -> Output {
    Output {
        stdout: String::new(),
        stderr: format!("{}\n", message.as_ref()),
        exit,
    }
}
pub fn help() -> Value {
    json!({"usage":["report annual --server ORIGIN --cookie-file PATH --company SLUG --from YYYY-MM-DD --to YYYY-MM-DD [--ixbrl-out FILE.xhtml] [--json] [--allow-loopback-http]","report tax --server ORIGIN --cookie-file PATH --company SLUG --from YYYY-MM-DD --to YYYY-MM-DD [--json] [--allow-loopback-http]","report annual --ixbrl-taxonomy [--json]"],"scope":"Read-only closed-year preparation through the configured server; company filesystem/default launcher compatibility remains pending R44. Taxonomy introspection is pure/local. XHTML is hash-verified and written only to a new explicitly selected file; existing files are refused. Tax/iXBRL are prepared for owner/advisor review, not submitted to TastSelv/Virk."})
}
struct Input<'a> {
    kind: &'a str,
    flags: BTreeMap<&'a str, &'a str>,
    json: bool,
    http: bool,
    taxonomy: bool,
}
fn parse(args: &[String]) -> Result<Input<'_>, String> {
    let kind = args
        .first()
        .map(String::as_str)
        .filter(|s| matches!(*s, "annual" | "tax"))
        .ok_or("Expected report annual|tax")?;
    let mut flags = BTreeMap::new();
    let (mut json, mut http, mut taxonomy) = (false, false, false);
    let mut cursor = 1;
    while cursor < args.len() {
        let key = args[cursor].as_str();
        match key {
            "--json" if !json => {
                json = true;
                cursor += 1;
            }
            "--allow-loopback-http" if !http => {
                http = true;
                cursor += 1;
            }
            "--ixbrl-taxonomy" if kind == "annual" && !taxonomy => {
                taxonomy = true;
                cursor += 1;
            }
            "--server" | "--cookie-file" | "--company" | "--from" | "--to" | "--ixbrl-out" => {
                let value = args
                    .get(cursor + 1)
                    .filter(|s| !s.is_empty() && !s.starts_with("--"))
                    .ok_or_else(|| format!("Missing value for {key}"))?;
                if flags.insert(key, value.as_str()).is_some() {
                    return Err(format!("Duplicate flag {key}"));
                }
                cursor += 2;
            }
            _ => {
                return Err(format!(
                    "Unknown or duplicate annual/tax flag {key}; see report {kind} --help"
                ));
            }
        }
    }
    if taxonomy {
        if !flags.is_empty() || http {
            return Err(
                "--ixbrl-taxonomy is standalone and requires no company/server/date selectors"
                    .into(),
            );
        }
    } else {
        if !flags.contains_key("--from") || !flags.contains_key("--to") {
            return Err("Missing required --from <YYYY-MM-DD> or --to <YYYY-MM-DD>".into());
        }
        if ["--server", "--cookie-file", "--company"]
            .iter()
            .any(|k| !flags.contains_key(k))
        {
            return Err("Explicit --server, --cookie-file and --company required".into());
        }
        if kind == "tax" && flags.contains_key("--ixbrl-out") {
            return Err("--ixbrl-out is allowed only on report annual".into());
        }
    }
    Ok(Input {
        kind,
        flags,
        json,
        http,
        taxonomy,
    })
}
fn client_error(error: Error) -> (u8, String) {
    let (code, error) = crate::remote::client_error(error);
    (code, format!("{}: {}", error.code, error.message))
}
fn publish(path: &str, xhtml: &str) -> Result<(), (u8, String)> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file=options.open(Path::new(path)).map_err(|_|(1,"artifact_write_failed: could not create new iXBRL file; existing targets are never overwritten".into()))?;
    let result = file
        .write_all(xhtml.as_bytes())
        .and_then(|_| file.sync_all());
    drop(file);
    if result.is_err() {
        let _ = std::fs::remove_file(path);
        return Err((
            1,
            "artifact_write_failed: could not finish iXBRL file".into(),
        ));
    }
    Ok(())
}
async fn read(api: &ApiClient, input: &Input<'_>) -> Result<Value, (u8, String)> {
    let (company, start, end) = (
        input.flags["--company"],
        input.flags["--from"],
        input.flags["--to"],
    );
    if input.kind == "tax" {
        return api
            .annual_export(company, start, end, "tax")
            .await
            .map_err(client_error);
    }
    if let Some(path) = input.flags.get("--ixbrl-out") {
        let export = api
            .annual_export(company, start, end, "ixbrl")
            .await
            .map_err(client_error)?;
        let mut report = export
            .get("annualReport")
            .filter(|v| v.is_object() && v["ok"].is_boolean())
            .cloned()
            .ok_or_else(|| {
                (
                    1,
                    "protocol: annual export omitted its admitted annual report".into(),
                )
            })?;
        report["ixbrl"] = if export["ok"] == true {
            publish(
                path,
                export["xhtml"]
                    .as_str()
                    .ok_or_else(|| (1, "protocol: XHTML omitted".into()))?,
            )?;
            json!({"ok":true,"path":path,"sha256":export["sha256"],"taxonomy":export["taxonomy"],"appliedRules":export["appliedRules"]})
        } else {
            json!({"ok":false,"path":null,"sha256":"","errors":export["errors"]})
        };
        Ok(report)
    } else {
        let response = api
            .annual_report(company, start, end)
            .await
            .map_err(client_error)?;
        response["annualReport"]
            .get("report")
            .filter(|v| v.is_object() && v["ok"].is_boolean())
            .cloned()
            .ok_or_else(|| (1, "protocol: annual report missing".into()))
    }
}
pub fn execute(args: &[String]) -> Output {
    if args.len() == 2 && args[1] == "--help" && matches!(args[0].as_str(), "annual" | "tax") {
        return Output {
            stdout: format!("{}\n", serde_json::to_string_pretty(&help()).unwrap()),
            stderr: String::new(),
            exit: 0,
        };
    }
    let input = match parse(args) {
        Ok(input) => input,
        Err(error) => return fail(error, 2),
    };
    if input.taxonomy {
        let mut taxonomy = rentemester_engine::ixbrl::taxonomy();
        taxonomy["ok"] = json!(true);
        taxonomy["elementCount"] = json!(taxonomy["elements"].as_array().map_or(0, Vec::len));
        return if input.json {
            render("taxonomy", &taxonomy, true)
        } else {
            Output {
                stdout: format!(
                    "✔ iXBRL-taksonomi (afgrænset udsnit)\n  Navn: {}\n",
                    text(&taxonomy["name"])
                ),
                stderr: String::new(),
                exit: 0,
            }
        };
    }
    let (start, end) = (input.flags["--from"], input.flags["--to"]);
    if !rentemester_client::valid_annual_bounds(start, end) {
        let error = if rentemester_client::valid_annual_bounds(start, start)
            && rentemester_client::valid_annual_bounds(end, end)
        {
            "fiscalYearStart must be before or equal to fiscalYearEnd"
        } else {
            "fiscalYearStart and fiscalYearEnd must be YYYY-MM-DD"
        };
        return render(
            input.kind,
            &json!({"ok":false,"fiscalYearStart":start,"fiscalYearEnd":end,"errors":[error]}),
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
        Err((code, error)) => return fail(error.message, code),
    };
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(_) => return fail("transport: could not start HTTP runtime", 1),
    };
    match runtime.block_on(read(&api, &input)) {
        Ok(report) => render(input.kind, &report, input.json),
        Err((code, error)) => fail(error, code),
    }
}
fn text(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| "—".into())
}
fn rows(value: &Value) -> impl Iterator<Item = &Value> {
    value.as_array().into_iter().flatten()
}
fn statement_rows(value: &Value) -> impl Iterator<Item = &Value> {
    rows(if value.is_array() {
        value
    } else {
        &value["lines"]
    })
}
fn money(value: &Value) -> String {
    if !value.is_number() {
        return "—".into();
    }
    let Ok(amount) = Money::parse(&value.to_string()) else {
        return "—".into();
    };
    let value = amount.decimal();
    let Some((whole, fraction)) = value.split_once('.') else {
        return "—".into();
    };
    let digits = whole.strip_prefix('-').unwrap_or(whole);
    let mut grouped = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push('.');
        }
        grouped.push(c);
    }
    format!(
        "{}{},{} kr.",
        if whole.starts_with('-') { "-" } else { "" },
        grouped,
        fraction
    )
}
fn section(lines: &mut Vec<String>, label: &str, value: &Value) {
    lines.push(format!("{label}:"));
    if statement_rows(value).next().is_none() {
        lines.push("  (ingen)".into());
    }
    for row in statement_rows(value) {
        lines.push(format!(
            "  {} {}: {}",
            text(&row["accountNo"]),
            text(&row["name"]),
            money(&row["amount"])
        ));
    }
    lines.push(format!(
        "{:<33}{}",
        format!("  {label} i alt:"),
        money(&value["total"])
    ));
}
fn annual_human(report: &Value) -> Vec<String> {
    let mut lines = vec![format!(
        "Årsrapport (regnskabsklasse B) for regnskabsåret {} til {}",
        text(&report["fiscalYearStart"]),
        text(&report["fiscalYearEnd"])
    )];
    if report["company"]["name"]
        .as_str()
        .is_some_and(|s| !s.is_empty())
    {
        let mut name = text(&report["company"]["name"]);
        if report["company"]["cvr"]
            .as_str()
            .is_some_and(|s| !s.is_empty())
        {
            name.push_str(&format!(", CVR {}", text(&report["company"]["cvr"])));
        }
        lines.push(name);
    }
    lines.extend([String::new(), "Resultatopgørelse".into(), String::new()]);
    let pl = &report["profitAndLoss"];
    for (key, label, total) in [
        ("income", "Indtægter", "totalIncome"),
        ("expense", "Omkostninger", "totalExpense"),
    ] {
        lines.push(format!("{label}:"));
        if statement_rows(&pl[key]).next().is_none() {
            lines.push("  (ingen)".into());
        }
        for row in statement_rows(&pl[key]) {
            lines.push(format!(
                "  {} {}: {}",
                text(&row["accountNo"]),
                text(&row["name"]),
                money(&row["amount"])
            ));
        }
        lines.push(format!(
            "{:<33}{}",
            format!("  {label} i alt:"),
            money(&pl[total])
        ));
        lines.push(String::new());
    }
    let result = money(&report["aretsResultat"]);
    lines.push(if result.starts_with('-') {
        format!(
            "Årets resultat: underskud på {}",
            result.trim_start_matches('-')
        )
    } else if Money::parse(&report["aretsResultat"].to_string())
        .is_ok_and(|v| v != Money::default())
    {
        format!("Årets resultat: overskud på {result}")
    } else {
        "Årets resultat: 0,00 kr.".into()
    });
    lines.push(String::new());
    let bs = &report["balanceSheet"];
    lines.extend([
        format!(
            "Balance pr. {}",
            bs["asOfDate"]
                .as_str()
                .unwrap_or_else(|| report["fiscalYearEnd"].as_str().unwrap_or("—"))
        ),
        String::new(),
    ]);
    for (key, label) in [
        ("assets", "Aktiver"),
        ("liabilities", "Passiver"),
        ("equity", "Egenkapital"),
    ] {
        section(&mut lines, label, &bs[key]);
        if key != "equity" {
            lines.push(String::new());
        }
    }
    lines.push(format!(
        "{:<33}{}",
        "  Periodens resultat:",
        money(&bs["periodResult"])
    ));
    lines.push(String::new());
    lines.push(format!(
        "  Aktiver i alt:                 {}",
        money(&bs["totalAssets"])
    ));
    lines.push(format!(
        "  Passiver og egenkapital i alt: {}",
        money(&bs["totalLiabilitiesAndEquity"])
    ));
    lines.push(
        if bs["balanced"] == true {
            "  Balancen går op (aktiver = passiver + egenkapital)."
        } else {
            "  ADVARSEL: balancen går ikke op."
        }
        .into(),
    );
    lines.push(String::new());
    if rows(&report["notes"]).next().is_some() {
        lines.push("Noter:".into());
        for note in rows(&report["notes"]) {
            lines.push(format!(
                "  - {}{}",
                text(&note["title"]),
                if note["placeholder"] == true {
                    " (skabelon — udfyldes)"
                } else {
                    ""
                }
            ));
        }
        lines.push(String::new());
    }
    if report["ledelsespategning"]["text"]
        .as_str()
        .is_some_and(|s| !s.is_empty())
    {
        lines.push("Ledelsespåtegning:".into());
        lines.push(format!("  {}", text(&report["ledelsespategning"]["text"])));
        if report["ledelsespategning"]["placeholder"] == true {
            lines.push("  (skabelon — gennemgås og godkendes af ledelsen)".into());
        }
        lines.push(String::new());
    }
    lines
}
fn tax_human(report: &Value) -> Vec<String> {
    let mut lines = vec![
        format!(
            "Skattemæssig indkomst (oplysningsskema) for regnskabsåret {} til {}",
            text(&report["fiscalYearStart"]),
            text(&report["fiscalYearEnd"])
        ),
        String::new(),
        format!(
            "  Årets resultat (bogført):        {}",
            money(&report["bookkeptResult"])
        ),
        String::new(),
        "Skattemæssige reguleringer:".into(),
    ];
    if rows(&report["adjustments"]).next().is_none() {
        lines.push("  (ingen deterministiske reguleringer fundet)".into());
    }
    for row in rows(&report["adjustments"]) {
        lines.push(format!(
            "  + {}: {}",
            text(&row["label"]),
            money(&row["amount"])
        ));
    }
    lines.extend([
        format!(
            "  Reguleringer i alt:              {}",
            money(&report["totalAdjustments"])
        ),
        String::new(),
        format!(
            "  Skattepligtig indkomst:          {}",
            money(&report["taxableIncome"])
        ),
        if report["corporateTax"].is_null() {
            "  Selskabsskat (22%):              ikke beregnet (se needs-review)".into()
        } else {
            format!(
                "  Selskabsskat (22%):              {}",
                money(&report["corporateTax"])
            )
        },
        String::new(),
    ]);
    if rows(&report["needsReview"]).next().is_some() {
        lines
            .push("Punkter til gennemgang (needs-review) — afklares af ejer eller revisor:".into());
        for item in rows(&report["needsReview"]) {
            lines.push(format!("  - {}", text(&item["label"])));
            lines.push(format!("    {}", text(&item["requiredAction"])));
        }
        lines.push(String::new());
    }
    lines
}
fn render(kind: &str, report: &Value, json: bool) -> Output {
    let failed = report["ok"] == false;
    if json {
        return Output {
            stdout: format!("{}\n", serde_json::to_string_pretty(report).unwrap()),
            stderr: String::new(),
            exit: u8::from(failed),
        };
    }
    let title = if kind == "annual" {
        "Årsrapport"
    } else {
        "Skattemæssig indkomst"
    };
    if failed {
        let mut lines = vec![format!("✘ {title} failed")];
        for error in rows(&report["errors"]) {
            lines.push(format!("  → {}", text(error)));
        }
        if lines.len() == 1 {
            lines.push("  → Kommandoen fejlede uden en specifik fejlbesked.".into());
        }
        return fail(lines.join("\n"), 1);
    }
    let mut lines = if kind == "annual" {
        annual_human(report)
    } else {
        tax_human(report)
    };
    if report["disclaimer"].is_string() {
        lines.push(text(&report["disclaimer"]));
    }
    if rows(&report["warnings"]).next().is_some() {
        lines.push(String::new());
        lines.push("Advarsler:".into());
        for warning in rows(&report["warnings"]) {
            lines.push(format!("  - {}", text(warning)));
        }
    }
    if lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    Output {
        stdout: format!("{}\n", lines.join("\n")),
        stderr: String::new(),
        exit: 0,
    }
}
