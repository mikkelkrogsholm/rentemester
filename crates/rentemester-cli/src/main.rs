use rentemester_engine::{Error, trial_balance::Period};
use serde_json::json;
use std::{collections::BTreeMap, path::Path, process::ExitCode};
fn run(args: Vec<String>) -> Result<serde_json::Value, (u8, Error)> {
    if args == ["--help"] {
        return Ok(json!({"ok":true,"commands":[
            {"name":"trial-balance","usage":"trial-balance --company PATH --from YYYY-MM-DD --to YYYY-MM-DD","scope":"experimental local read-only report"},
            {"name":"verify-journal-hashes","usage":"verify-journal-hashes --company PATH","scope":"experimental journal canonical hashes; not full audit integrity"}
        ]}));
    }
    if args == ["verify-journal-hashes", "--help"] {
        return Ok(
            json!({"ok":true,"usage":"rentemester-rust verify-journal-hashes --company PATH","format":"rentemester-journal-hashes/1","scope":"experimental local read-only journal canonical hashes; not full audit integrity"}),
        );
    }
    if args.first().map(String::as_str) == Some("verify-journal-hashes") {
        if args.len() != 3
            || args[1] != "--company"
            || args[2].is_empty()
            || args[2].starts_with("--")
        {
            return Err((
                2,
                Error::new(
                    "invalid_input",
                    "expected verify-journal-hashes --company PATH",
                ),
            ));
        }
        let report =
            rentemester_storage::verify_journal_hashes(Path::new(&args[2])).map_err(|e| (1, e))?;
        return Ok(json!({"ok":report.ok,"format":"rentemester-journal-hashes/1","data":report}));
    }
    if args == ["trial-balance", "--help"] {
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
            if value.get("ok").and_then(serde_json::Value::as_bool) == Some(false) {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            }
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
