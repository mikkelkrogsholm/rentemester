mod annual_exports;
mod audit_log;
mod audit_verify;
mod backup_governance;
mod backup_registration;
mod backup_remote;
mod bank_accounts_original;
mod bank_import;
mod bank_matches;
mod bank_original;
mod bank_queries;
mod cvr;
mod dashboard_artifact;
mod dated_reports;
mod direct_bank_original;
mod eu_sales;
mod handoff_export;
mod invoice_bad_debt;
mod invoice_claims_original;
mod invoice_compensation;
mod invoice_financial_original;
mod invoice_interest;
mod invoice_lifecycle;
mod invoice_original;
mod invoice_public_original;
mod journal_history;
mod local_binding;
mod original_args;
mod original_discovery;
mod period_original;
mod oss_report;
mod payable_original;
mod recurring_original;
mod saft_artifact;
use rentemester_engine::{Error, trial_balance::Period};
use serde_json::json;
use std::{collections::BTreeMap, io::Read, path::Path, process::ExitCode};
mod accruals;
mod annual_reports;
mod assets;
mod bank_correction;
mod bank_reconciliation;
mod bilagsmail;
mod budget;
mod commitments;
mod company_creation;
mod company_knowledge;
mod company_setup;
mod consolidation;
mod corporate_records;
mod document_pdf;
mod documents;
mod elimination;
mod exceptions;
mod intercompany;
mod invoice_extraction;
mod mileage;
mod ownership;
mod parties;
mod portable_backup;
mod privacy;
mod public_einvoice;
mod receivables;
mod recurring_invoices;
mod reminder_claims;
mod remote;
mod report_reads;
mod reviewed_bookkeeping;
mod saft;
mod workspace_inbox;
fn run(args: Vec<String>) -> Result<serde_json::Value, (u8, Error)> {
    if args.first().map(String::as_str) == Some("journal")
        && args.get(1).map(String::as_str) == Some("list")
    {
        return journal_history::run(&args[2..]);
    }
    if args.first().map(String::as_str) == Some("documents") {
        if args.len() == 1 || args == ["documents", "--help"] {
            return Ok(invoice_extraction::help());
        }
        if args.get(1).is_some_and(|s| invoice_extraction::accepts(s)) {
            if args.get(2).map(String::as_str) == Some("--help") && args.len() == 3 {
                return Ok(invoice_extraction::help());
            }
            return remote::run(&args[1..]);
        }
        return Err((
            2,
            Error::new(
                "invalid_input",
                "expected documents extract-invoice|invoice-extraction; see documents --help",
            ),
        ));
    }
    if args.first().map(String::as_str) == Some("dashboard") {
        return remote::run(&args);
    }
    if args.first().map(String::as_str) == Some("remote") {
        return remote::run(&args[1..]);
    }
    if args == ["--help"] {
        return Ok(original_discovery::help());
    }
    if args == ["validate-journal", "--help"] {
        return Ok(
            json!({"ok":true,"usage":"rentemester-rust validate-journal --company PATH < request.json","format":"rentemester-manual-journal-preview/1","requestFormat":rentemester_storage::journal_preview::REQUEST_FORMAT,"maxRequestBytes":rentemester_storage::journal_preview::MAX_REQUEST_BYTES,"requestFields":["format","expectedRuleVersion","latestAllowedDate","request"],"businessErrors":"data.validation.errors","transportErrors":"errors","exitCodes":{"0":"accepted advisory preview","1":"business/source/rule refusal","2":"usage/transport/type refusal"},"activeRuleVersion":rentemester_storage::manual_journal_rule_version().map_err(|e|(1,e))?,"scope":"advisory read-only source snapshot; explicit preview horizon; no posting authority"}),
        );
    }
    if args.first().map(String::as_str) == Some("validate-journal") {
        if args.len() != 3
            || args[1] != "--company"
            || args[2].is_empty()
            || args[2].starts_with("--")
        {
            return Err((
                2,
                Error::new(
                    "invalid_input",
                    "expected validate-journal --company PATH and bounded JSON on stdin",
                ),
            ));
        }
        let mut input = Vec::new();
        std::io::stdin()
            .lock()
            .take(rentemester_storage::journal_preview::MAX_REQUEST_BYTES + 1)
            .read_to_end(&mut input)
            .map_err(|_| {
                (
                    2,
                    Error::new("invalid_input", "could not read journal preview request"),
                )
            })?;
        if input.len() as u64 > rentemester_storage::journal_preview::MAX_REQUEST_BYTES {
            return Err((
                2,
                Error::new(
                    "invalid_input",
                    "journal preview request exceeds byte limit",
                ),
            ));
        }
        let input: rentemester_storage::PreviewInput =
            serde_json::from_slice(&input).map_err(|_| {
                (
                    2,
                    Error::new(
                        "invalid_input",
                        "expected one strict canonical journal preview envelope",
                    ),
                )
            })?;
        if input.format != rentemester_storage::journal_preview::REQUEST_FORMAT {
            return Err((
                2,
                Error::new(
                    "invalid_input",
                    "unsupported journal preview request format",
                ),
            ));
        }
        let report = rentemester_storage::preview_manual_journal(Path::new(&args[2]), input)
            .map_err(|e| (1, e))?;
        return Ok(
            json!({"ok":report.validation.ok,"format":"rentemester-manual-journal-preview/1","data":report}),
        );
    }
    if args == ["rehearse-company-baseline", "--help"] {
        return Ok(
            json!({"ok":true,"usage":"rentemester-rust rehearse-company-baseline --company PATH","format":"rentemester-company-baseline-rehearsal/1","scope":"known unversioned physical baseline private copy only; no source migration, adoption or writer authority"}),
        );
    }
    if args.first().map(String::as_str) == Some("rehearse-company-baseline") {
        if args.len() != 3
            || args[1] != "--company"
            || args[2].is_empty()
            || args[2].starts_with("--")
        {
            return Err((
                2,
                Error::new(
                    "invalid_input",
                    "expected rehearse-company-baseline --company PATH",
                ),
            ));
        }
        let report = rentemester_storage::rehearse_company_baseline(Path::new(&args[2]))
            .map_err(|e| (1, e))?;
        return Ok(
            json!({"ok":true,"format":"rentemester-company-baseline-rehearsal/1","data":report}),
        );
    }
    if args == ["rehearse-control-migration", "--help"] {
        return Ok(
            json!({"ok":true,"usage":"rentemester-rust rehearse-control-migration --workspace PATH","format":"rentemester-control-migration-rehearsal/1","scope":"private control copy only; no writer authority"}),
        );
    }
    if args.first().map(String::as_str) == Some("rehearse-control-migration") {
        if args.len() != 3
            || args[1] != "--workspace"
            || args[2].is_empty()
            || args[2].starts_with("--")
        {
            return Err((
                2,
                Error::new(
                    "invalid_input",
                    "expected rehearse-control-migration --workspace PATH",
                ),
            ));
        }
        let report = rentemester_storage::rehearse_control_migration(Path::new(&args[2]))
            .map_err(|e| (1, e))?;
        return Ok(
            json!({"ok":true,"format":"rentemester-control-migration-rehearsal/1","data":report}),
        );
    }
    if args == ["rehearse-company-migration", "--help"] {
        return Ok(
            json!({"ok":true,"usage":"rentemester-rust rehearse-company-migration --company PATH","format":"rentemester-company-migration-rehearsal/1","scope":"private-copy full canonical prefixes0–36 and bounded minimal suffix32–36; no source migration or writer authority"}),
        );
    }
    if args.first().map(String::as_str) == Some("rehearse-company-migration") {
        if args.len() != 3
            || args[1] != "--company"
            || args[2].is_empty()
            || args[2].starts_with("--")
        {
            return Err((
                2,
                Error::new(
                    "invalid_input",
                    "expected rehearse-company-migration --company PATH",
                ),
            ));
        }
        let report = rentemester_storage::rehearse_company_migration(Path::new(&args[2]))
            .map_err(|e| (1, e))?;
        return Ok(
            json!({"ok":true,"format":"rentemester-company-migration-rehearsal/1","data":report}),
        );
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
    let args = match original_args::prepare(std::env::args().skip(1).collect()) {
        Ok(args) => args,
        Err((code, error)) => {
            eprintln!("{}: {}; see --help", error.code, error.message);
            return ExitCode::from(code);
        }
    };
    if let Some(output) = original_discovery::execute(&args) {
        print!("{}", output.stdout);
        eprint!("{}", output.stderr);
        return ExitCode::from(output.exit);
    }
    if args.first().is_some_and(|s| s == "period") {
        if args == ["period"] || args == ["period", "--help"] {
            println!("{}", period_original::help());
            return ExitCode::SUCCESS;
        }
        if !period_original::accepts(&args) {
            eprintln!("Unknown period command; see period --help");
            return ExitCode::from(2);
        }
        if let Err(output) = period_original::preflight(&args) {
            print!("{}", output.stdout);
            eprint!("{}", output.stderr);
            return ExitCode::from(output.exit);
        }
    }
    let args = match local_binding::prepare(args) {
        Ok(args) => args,
        Err((code, error)) => {
            println!(
                "{}",
                json!({"ok":false,"code":error.code,"errors":[error.message]})
            );
            return ExitCode::from(code);
        }
    };
    if period_original::accepts(&args) {
        let output = period_original::execute(&args);
        print!("{}", output.stdout);
        eprint!("{}", output.stderr);
        return ExitCode::from(output.exit);
    }
    if recurring_original::accepts(&args) {
        let output = recurring_original::execute(&args);
        print!("{}", output.stdout);
        eprint!("{}", output.stderr);
        return ExitCode::from(output.exit);
    }
    if invoice_financial_original::accepts(&args) {
        let output = invoice_financial_original::execute(&args);
        print!("{}", output.stdout);
        eprint!("{}", output.stderr);
        return ExitCode::from(output.exit);
    }
    if invoice_public_original::accepts(&args) {
        let output = invoice_public_original::execute(&args);
        print!("{}", output.stdout);
        eprint!("{}", output.stderr);
        return ExitCode::from(output.exit);
    }
    if invoice_claims_original::accepts(&args) {
        let output = invoice_claims_original::execute(&args);
        print!("{}", output.stdout);
        eprint!("{}", output.stderr);
        return ExitCode::from(output.exit);
    }
    if invoice_lifecycle::accepts(&args) {
        let output = invoice_lifecycle::execute(&args);
        print!("{}", output.stdout);
        eprint!("{}", output.stderr);
        return ExitCode::from(output.exit);
    }
    if invoice_original::accepts(&args) {
        let output = invoice_original::execute(&args);
        print!("{}", output.stdout);
        eprint!("{}", output.stderr);
        return ExitCode::from(output.exit);
    }
    if payable_original::accepts(&args) {
        let output = payable_original::execute(&args);
        print!("{}", output.stdout);
        eprint!("{}", output.stderr);
        return ExitCode::from(output.exit);
    }
    if direct_bank_original::accepts(&args) {
        let output = direct_bank_original::execute(&args);
        print!("{}", output.stdout);
        eprint!("{}", output.stderr);
        return ExitCode::from(output.exit);
    }
    if bank_original::accepts(&args) {
        let output = bank_original::execute(&args);
        print!("{}", output.stdout);
        eprint!("{}", output.stderr);
        return ExitCode::from(output.exit);
    }
    if bank_accounts_original::accepts(&args) {
        let output = bank_accounts_original::execute(&args);
        print!("{}", output.stdout);
        eprint!("{}", output.stderr);
        return ExitCode::from(output.exit);
    }
    if bank_import::accepts(&args) {
        let output = bank_import::execute(&args);
        print!("{}", output.stdout);
        eprint!("{}", output.stderr);
        return ExitCode::from(output.exit);
    }
    if bank_queries::accepts(&args) {
        let output = bank_queries::execute(&args);
        print!("{}", output.stdout);
        eprint!("{}", output.stderr);
        return ExitCode::from(output.exit);
    }
    if args.first().map(String::as_str) == Some("system")
        && args.get(1).map(String::as_str) == Some("export-saft")
    {
        let output = saft_artifact::execute(&args[1..]);
        print!("{}", output.stdout);
        eprint!("{}", output.stderr);
        return ExitCode::from(output.exit);
    }
    if args.first().map(String::as_str) == Some("system")
        && args
            .get(1)
            .is_some_and(|s| matches!(s.as_str(), "export-authority" | "export-accountant"))
    {
        let output = handoff_export::execute(&args[1..]);
        print!("{}", output.stdout);
        eprint!("{}", output.stderr);
        return ExitCode::from(output.exit);
    }

    if args.first().map(String::as_str) == Some("report") {
        let output = if args
            .get(1)
            .is_some_and(|s| matches!(s.as_str(), "annual" | "tax"))
        {
            annual_exports::execute(&args[1..])
        } else if args == ["report", "--help"] {
            dated_reports::Output {
                stdout: format!(
                    "{}\n",
                    serde_json::to_string_pretty(
                        &json!({"dated":dated_reports::help(),"annualTax":annual_exports::help()})
                    )
                    .unwrap()
                ),
                stderr: String::new(),
                exit: 0,
            }
        } else {
            dated_reports::execute(&args[1..])
        };
        print!("{}", output.stdout);
        eprint!("{}", output.stderr);
        return ExitCode::from(output.exit);
    }
    match run(args) {
        Ok(value) => {
            if let Some(text) = value.as_str() {
                println!("{text}");
            } else {
                println!("{value}");
            }
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

mod scheduled_recurring;

mod service_principals;

mod hosted_recurring;

mod invoice_email;

mod workspace_transfer;

mod bookkeeping_batch;

mod payable_correction;

mod intercompany_dispositions;

mod contact_import;

mod cfo_analytics;

mod archive_import;

mod opening_balance;

mod portfolio_cashflow;

mod dinero_import;
