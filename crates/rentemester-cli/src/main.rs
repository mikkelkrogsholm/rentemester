use rentemester_engine::{Error, trial_balance::Period};
use serde_json::json;
use std::{collections::BTreeMap, path::Path, process::ExitCode};
fn run(args: Vec<String>) -> Result<serde_json::Value, (u8, Error)> {
    if args == ["--help"] || args == ["trial-balance", "--help"] {
        return Ok(
            json!({"ok":true,"usage":"rentemester-rust trial-balance --company PATH --from YYYY-MM-DD --to YYYY-MM-DD","format":"rentemester-trial-balance/1","scope":"experimental local read-only current-schema report"}),
        );
    }
    let usage = || {
        (
            2,
            Error::new(
                "invalid_input",
                "expected trial-balance --company PATH --from YYYY-MM-DD --to YYYY-MM-DD",
            ),
        )
    };
    if args.first().map(String::as_str) != Some("trial-balance") || args.len() != 7 {
        return Err(usage());
    }
    let mut flags = BTreeMap::new();
    for pair in args[1..].chunks_exact(2) {
        if !["--company", "--from", "--to"].contains(&pair[0].as_str())
            || pair[1].is_empty()
            || flags.insert(pair[0].as_str(), pair[1].as_str()).is_some()
        {
            return Err(usage());
        }
    }
    let company = flags.get("--company").ok_or_else(usage)?;
    let period = Period::new(
        flags.get("--from").ok_or_else(usage)?,
        flags.get("--to").ok_or_else(usage)?,
    )
    .map_err(|e| (2, e))?;
    let report =
        rentemester_storage::trial_balance(Path::new(company), period).map_err(|e| (1, e))?;
    Ok(json!({"ok":true,"format":"rentemester-trial-balance/1","data":report}))
}
fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(value) => {
            println!("{value}");
            ExitCode::SUCCESS
        }
        Err((code, error)) => {
            println!(
                "{}",
                json!({"ok":false,"code":error.code,"errors":[error.message]})
            );
            ExitCode::from(code)
        }
    }
}
