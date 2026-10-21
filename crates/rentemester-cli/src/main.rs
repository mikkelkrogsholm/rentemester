mod bank_matches;
mod cvr;
mod backup_remote;
mod backup_governance;
mod eu_sales;
mod oss_report;
use rentemester_engine::{Error, trial_balance::Period};
use serde_json::json;
use std::{collections::BTreeMap, io::Read, path::Path, process::ExitCode};
mod reminder_claims;
mod report_reads;
mod company_creation;
mod saft;
mod portable_backup;
mod company_setup;
mod privacy;
mod budget;
mod exceptions;
mod mileage;
mod company_knowledge;
mod parties;
mod workspace_inbox;
mod bilagsmail;
mod public_einvoice;
mod corporate_records;
mod ownership;
mod intercompany;
mod elimination;
mod consolidation;
mod accruals;
mod assets;
mod recurring_invoices;
mod bank_correction;
mod bank_reconciliation;
mod documents;
mod document_pdf;
mod invoice_extraction;
mod reviewed_bookkeeping;
mod annual_reports;
mod commitments;
mod receivables;
mod remote;
fn run(args: Vec<String>) -> Result<serde_json::Value, (u8, Error)> {
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
        return Err((2,Error::new("invalid_input","expected documents extract-invoice|invoice-extraction; see documents --help")));
    }
    if args.first().map(String::as_str) == Some("remote") {
        return remote::run(&args[1..]);
    }
    if args == ["--help"] {
        return Ok(json!({"ok":true,"commands":[
            {"name":"remote","usage":"remote --help","scope":"explicit authenticated Rust HTTP server; no local fallback"},
            {"name":"documents","usage":"documents --help","scope":"registered-PDF invoice extraction or retained evidence; explicit remote server/session"},
            {"name":"validate-journal","usage":"validate-journal --company PATH < request.json","scope":"experimental source-backed advisory manual validation; no writer authority"},
            {"name":"trial-balance","usage":"trial-balance --company PATH --from YYYY-MM-DD --to YYYY-MM-DD","scope":"experimental local read-only report"},
            {"name":"rehearse-company-baseline","usage":"rehearse-company-baseline --company PATH","scope":"experimental explicit known unversioned physical baseline copy; no adoption authority"},
            {"name":"rehearse-control-migration","usage":"rehearse-control-migration --workspace PATH","scope":"experimental private control copy; no source migration"},
            {"name":"rehearse-company-migration","usage":"rehearse-company-migration --company PATH","scope":"experimental full canonical company prefixes0–36 and bounded minimal suffix32–36; no source migration"},
            {"name":"verify-journal-hashes","usage":"verify-journal-hashes --company PATH","scope":"experimental journal canonical hashes; not full audit integrity"}
        ]}));
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
