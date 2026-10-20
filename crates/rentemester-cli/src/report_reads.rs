//! Read the requested source without client-side report reconstruction.
use rentemester_client::Report;
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub fn accepts(command: &str) -> bool {
    matches!(
        command,
        "journal" | "trial-balance" | "income-statement" | "balance" | "overview" | "report-export"
    )
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    http: bool,
    confirm: bool,
    preview: bool,
) -> Result<Value, (u8, Error)> {
    let invalid = |m| (2, Error::new("invalid_input", m));
    let export = command == "report-export";
    if confirm
        || preview
        || flags.keys().any(|k| {
            !([
                "--server",
                "--cookie-file",
                "--service-key-file",
                "--company",
                "--year",
                "--account",
                "--source",
            ]
            .contains(k)
                || export && ["--report", "--format"].contains(k))
        })
    {
        return Err(invalid("exact read-only report selectors required"));
    }
    let required = |k| {
        flags.get(k).copied().ok_or_else(|| {
            invalid("explicit server/cookie/company and export report/format required")
        })
    };
    let report = match if export {
        required("--report")?
    } else {
        command
    } {
        "journal" => Report::Journal,
        "trial-balance" => Report::TrialBalance,
        "income-statement" => Report::IncomeStatement,
        "balance" => Report::Balance,
        "overview" if !export => Report::Overview,
        _ => return Err(invalid("unknown report")),
    };
    if flags.contains_key("--account") && !matches!(report, Report::Journal) {
        return Err(invalid("account allowed only on journal"));
    }
    let api = crate::remote::credential_client(required("--server")?, http, flags, false)?;
    let company = required("--company")?;
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|_| (1, Error::new("transport", "could not start HTTP runtime")))?;
    let value = runtime
        .block_on(async {
            let (year, account, source) = (
                flags.get("--year").copied(),
                flags.get("--account").copied(),
                flags.get("--source").copied(),
            );
            if export {
                api.report_export(
                    company,
                    report,
                    year,
                    account,
                    source,
                    flags.get("--format").copied().unwrap_or(""),
                )
                .await
            } else {
                api.report_source(company, report, year, account, source)
                    .await
            }
        })
        .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":value["ok"].as_bool().unwrap_or(false),"data":value}))
}
