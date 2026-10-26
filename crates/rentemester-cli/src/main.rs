mod operator_original716;
mod remaining_original716;
mod provider_original716;
mod system_original716;
mod report_analytics_original;
mod agent_original;
mod import_original;
mod group_original;
mod company_original;
mod workspace_inbox_original;
mod party_original;
mod corporate_records_original;
mod ownership_original;
mod mileage_posting;
mod bookkeeping_batch_original;
mod accounting_draft_original;
mod accounts_original;
mod dimensions_original;
mod supplier_commitment_original;
mod vat_purchase_original;
mod asset_original;
mod accrual_original;
mod mileage_original;
mod expense_original;
mod annual_exports;
mod audit_log;
mod audit_verify;
mod audit_verify_original;
mod system_healthcheck_original;
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
mod document_party_original;
mod posting_rules_original;
mod budget_original;
mod vat_original;
mod documents_original;
mod eu_sales;
mod exceptions_original;
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
mod journal_original;
mod local_binding;
mod master_data_original;
mod original_args;
mod original_discovery;
mod oss_report;
mod payable_original;
mod period_original;
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
mod tasks;
mod intercompany;
mod invoice_extraction;
mod invoice_extraction_original;
mod mileage;
mod ownership;
mod parties;
mod portable_backup;
mod privacy;
mod regulatory;
mod public_einvoice;
mod digisense_received;
mod receivables;
mod recurring_invoices;
mod reminder_claims;
mod remote;
mod report_reads;
mod reviewed_bookkeeping;
mod saft;
mod workspace_inbox;
fn run(args: Vec<String>) -> Result<serde_json::Value, (u8, Error)> {
    if args.first().is_some_and(|s|s=="task-boards"){return tasks::run_boards(&args[1..]);}
    if args.first().is_some_and(|s|s=="task-series"){return tasks::run_series(&args[1..]);}
    if args.first().is_some_and(|s|s=="tasks"){return tasks::run(&args[1..]);}
    if args.first().is_some_and(|s|mileage_posting::accepts(s)) { return remote::run(&args); }
    if args.first().is_some_and(|s|s=="reg") { return regulatory::run(&args); }
    if args.first().is_some_and(|s|privacy::accepts(s)||s=="compliance-report") { return remote::run(&args); }
    if args.first().map(String::as_str) == Some("journal")
        && args.get(1).map(String::as_str) == Some("list")
    {
        return journal_history::run(&args[2..]);
    }
    if args.first().map(String::as_str) == Some("documents") {
        return Err((2, Error::new("invalid_input", "unknown documents command; see documents --help")));
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
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let raw = if raw.first().is_some_and(|s| mileage_posting::accepts(s)) {
        std::iter::once("remote".to_owned()).chain(raw).collect()
    } else { raw };
    if let Some(args) = report_analytics_original::prepare(&raw) {
        let o=report_analytics_original::execute(&args);
        print!("{}",o.stdout);eprint!("{}",o.stderr);return ExitCode::from(o.exit);
    }
    if let Some(args) = agent_original::prepare(&raw) {
        let o = agent_original::execute(&args);
        print!("{}",o.stdout); eprint!("{}",o.stderr); return ExitCode::from(o.exit);
    }
    if let Some(args) = operator_original716::transfer::prepare(&raw) {
        let runtime = tokio::runtime::Runtime::new().expect("CLI runtime");
        if let Some(o) = runtime.block_on(operator_original716::transfer::execute(&args)) {
            print!("{}", o.stdout); eprint!("{}", o.stderr); return ExitCode::from(o.exit);
        }
    }
    if let Some(args) = operator_original716::bootstrap::prepare(&raw) {
        let runtime = tokio::runtime::Runtime::new().expect("CLI runtime");
        if let Some(o) = runtime.block_on(operator_original716::bootstrap::execute(&args)) {
            print!("{}", o.stdout); eprint!("{}", o.stderr); return ExitCode::from(o.exit);
        }
    }
    if let Some(args) = operator_original716::schema::prepare(&raw) {
        let runtime = tokio::runtime::Runtime::new().expect("CLI runtime");
        if let Some(o) = runtime.block_on(operator_original716::schema::execute(&args)) {
            print!("{}", o.stdout); eprint!("{}", o.stderr); return ExitCode::from(o.exit);
        }
    }
    if let Some(args) = operator_original716::custody::prepare(&raw) {
        let runtime = tokio::runtime::Runtime::new().expect("CLI runtime");
        if let Some(o) = runtime.block_on(operator_original716::custody::execute(&args)) {
            print!("{}", o.stdout); eprint!("{}", o.stderr); return ExitCode::from(o.exit);
        }
    }
    if let Some(args) = operator_original716::prepare(&raw) {
        let runtime = tokio::runtime::Runtime::new().expect("CLI runtime");
        if let Some(o) = runtime.block_on(operator_original716::execute(&args)) {
            print!("{}", o.stdout); eprint!("{}", o.stderr); return ExitCode::from(o.exit);
        }
    }
    if let Some(args) = remaining_original716::prepare(&raw) {
        let runtime = tokio::runtime::Runtime::new().expect("CLI runtime");
        if let Some(o) = runtime.block_on(remaining_original716::execute(&args)) {
            print!("{}", o.stdout); eprint!("{}", o.stderr); return ExitCode::from(o.exit);
        }
    }
    if let Some(args) = provider_original716::prepare(&raw) {
        let runtime = tokio::runtime::Runtime::new().expect("CLI runtime");
        if let Some(o) = runtime.block_on(provider_original716::execute(&args)) {
            print!("{}", o.stdout); eprint!("{}", o.stderr); return ExitCode::from(o.exit);
        }
    }
    if let Some(args) = system_original716::prepare(&raw) {
        let runtime = tokio::runtime::Runtime::new().expect("CLI runtime");
        if let Some(o) = runtime.block_on(system_original716::execute(&args)) {
            print!("{}", o.stdout); eprint!("{}", o.stderr); return ExitCode::from(o.exit);
        }
    }
    let args = match original_args::prepare_with_output(
        raw,
        std::io::IsTerminal::is_terminal(&std::io::stdout()),
    ) {
        Ok(args) => args,
        Err((code, error)) => {
            if error.message == "Missing required --input <file.json>" {
                eprintln!("{}", error.message);
            } else {
                eprintln!("{}: {}; see --help", error.code, error.message);
            }
            return ExitCode::from(code);
        }
    };
    if system_healthcheck_original::accepts(&args) {
        let output = system_healthcheck_original::execute(&args);
        print!("{}", output.stdout);
        eprint!("{}", output.stderr);
        return ExitCode::from(output.exit);
    }
    // Offline regulatory presentation owns its source-compatible streams.
    if args.first().is_some_and(|s| s == "reg") {
        let o = regulatory::execute(&args);
        print!("{}",o.stdout); eprint!("{}",o.stderr); return ExitCode::from(o.exit);
    }
    if let Some(output) = original_discovery::execute(&args) {
        print!("{}", output.stdout);
        eprint!("{}", output.stderr);
        return ExitCode::from(output.exit);
    }
    if args.first().is_some_and(|s|s=="bookkeeping-batch") {
        if args==["bookkeeping-batch"] || args==["bookkeeping-batch","--help"] {println!("{}",bookkeeping_batch_original::help());return ExitCode::SUCCESS;}
        if !bookkeeping_batch_original::accepts(&args) {eprintln!("Unknown bookkeeping-batch command; see bookkeeping-batch --help");return ExitCode::from(2);}
        if let Err(o)=bookkeeping_batch_original::preflight(&args) {eprint!("{}",o.stderr);return ExitCode::from(o.exit);}
        if args.iter().any(|s|matches!(s.as_str(),"--help"|"--example")) {println!("{}",bookkeeping_batch_original::help());return ExitCode::SUCCESS;}
    }
    if args.first().is_some_and(|s|s=="accounting-draft") {
        if args==["accounting-draft"] || args==["accounting-draft","--help"] {println!("{}",accounting_draft_original::help());return ExitCode::SUCCESS;}
        if !accounting_draft_original::accepts(&args) {eprintln!("Unknown accounting-draft command; see accounting-draft --help");return ExitCode::from(2);}
        if let Err(o)=accounting_draft_original::preflight(&args) {eprint!("{}",o.stderr);return ExitCode::from(o.exit);}
        if args.iter().any(|s|s=="--help") {println!("{}",accounting_draft_original::help());return ExitCode::SUCCESS;}
    }
    if args.first().is_some_and(|s|s=="dimensions") {
        if args==["dimensions"] || args==["dimensions","--help"] {println!("{}",dimensions_original::help());return ExitCode::SUCCESS;}
        if !dimensions_original::accepts(&args) {eprintln!("Unknown dimensions command; see dimensions --help");return ExitCode::from(2);}
        if let Err(o)=dimensions_original::preflight(&args) {eprint!("{}",o.stderr);return ExitCode::from(o.exit);}
        if args.iter().any(|s|s=="--help") {println!("{}",dimensions_original::help());return ExitCode::SUCCESS;}
    }
    if args.first().is_some_and(|s|s=="supplier-commitment") {
        if args==["supplier-commitment"] || args==["supplier-commitment","--help"] {println!("{}",supplier_commitment_original::help());return ExitCode::SUCCESS;}
        if !supplier_commitment_original::accepts(&args) {eprintln!("Unknown supplier-commitment command; see supplier-commitment --help");return ExitCode::from(2);}
        if let Err(o)=supplier_commitment_original::preflight(&args) {eprint!("{}",o.stderr);return ExitCode::from(o.exit);}
        if args.iter().any(|s|s=="--help") {println!("{}",supplier_commitment_original::help());return ExitCode::SUCCESS;}
    }
    if args.first().is_some_and(|s|s=="asset") {
        if args==["asset"] || args==["asset","--help"] {println!("{}",asset_original::help());return ExitCode::SUCCESS;}
        if !asset_original::accepts(&args) {eprintln!("Unknown asset command; see asset --help");return ExitCode::from(2);}
        if let Err(o)=asset_original::preflight(&args) {print!("{}",o.stdout);eprint!("{}",o.stderr);return ExitCode::from(o.exit);}
        if args.iter().any(|s|s=="--help") {println!("{}",asset_original::help());return ExitCode::SUCCESS;}
    }
    if args.first().is_some_and(|s|s=="mileage") {
        if args==["mileage"] || args==["mileage","--help"] {println!("{}",mileage_original::help());return ExitCode::SUCCESS;}
        if !mileage_original::accepts(&args) {eprintln!("Unknown mileage command; see mileage --help");return ExitCode::from(2);}
        if let Err(o)=mileage_original::preflight(&args) {print!("{}",o.stdout);eprint!("{}",o.stderr);return ExitCode::from(o.exit);}
        if args.iter().any(|s|s=="--help") {println!("{}",mileage_original::help());return ExitCode::SUCCESS;}
    }
    if args.first().is_some_and(|s|s=="accrual") {
        if args==["accrual"] || args==["accrual","--help"] {println!("{}",accrual_original::help());return ExitCode::SUCCESS;}
        if !accrual_original::accepts(&args) {eprintln!("Unknown accrual command; see accrual --help");return ExitCode::from(2);}
        if let Err(o)=accrual_original::preflight(&args) {print!("{}",o.stdout);eprint!("{}",o.stderr);return ExitCode::from(o.exit);}
        if args.iter().any(|s|s=="--help") {println!("{}",accrual_original::help());return ExitCode::SUCCESS;}
    }
    if args.first().is_some_and(|s| s == "expense") {
        if args == ["expense"] || args == ["expense", "--help"] {
            println!("{}", expense_original::help());
            return ExitCode::SUCCESS;
        }
        if !expense_original::accepts(&args) {
            eprintln!("Unknown expense command; see expense --help");
            return ExitCode::from(2);
        }
        if let Err(output) = expense_original::preflight(&args) {
            eprint!("{}", output.stderr);
            return ExitCode::from(output.exit);
        }
        if args.iter().any(|s| s == "--help") {
            println!("{}", expense_original::help());
            return ExitCode::SUCCESS;
        }
    }
    if args == ["documents"] || args == ["documents", "--help"] {
        println!("{}", invoice_extraction_original::family_help());
        return ExitCode::SUCCESS;
    }
    if invoice_extraction_original::accepts(&args) {
        if let Err(output) = invoice_extraction_original::preflight(&args) {
            eprint!("{}", output.stderr);
            return ExitCode::from(output.exit);
        }
        if args.iter().any(|s| s == "--help") {
            println!("{}", invoice_extraction_original::help());
            return ExitCode::SUCCESS;
        }
    }
    let vat_purchase_input = if vat_purchase_original::accepts(&args) {
        match vat_purchase_original::preflight(&args) {
            Ok(Some(value)) => Some(value),
            Ok(None) => { println!("{}",vat_purchase_original::help()); return ExitCode::SUCCESS; }
            Err(output) => { eprint!("{}",output.stderr); return ExitCode::from(output.exit); }
        }
    } else { None };
    if args == ["vat"] || args == ["vat", "--help"] { let mut help=vat_original::help(); help["purchases"]=vat_purchase_original::help(); println!("{help}"); return ExitCode::SUCCESS; }
    if vat_original::accepts(&args) {
        if let Err(output) = vat_original::preflight(&args) { eprint!("{}", output.stderr); return ExitCode::from(output.exit); }
        if args.iter().any(|s|s=="--help") {println!("{}",vat_original::help());return ExitCode::SUCCESS;}
    }
    if args == ["budget"] || args == ["budget", "--help"] { println!("{}", budget_original::help()); return ExitCode::SUCCESS; }
    if budget_original::accepts(&args) {
        if let Err(output) = budget_original::preflight(&args) { eprint!("{}", output.stderr); return ExitCode::from(output.exit); }
        if args.iter().any(|s|s=="--help") {println!("{}",budget_original::help());return ExitCode::SUCCESS;}
    }
    if posting_rules_original::accepts(&args) {
        if let Err(output)=posting_rules_original::preflight(&args) {eprint!("{}",output.stderr);return ExitCode::from(output.exit);}
        if args.len()==1 || args.iter().any(|s|s=="--help") {println!("{}",posting_rules_original::help());return ExitCode::SUCCESS;}
    }
    if accounts_original::accepts(&args) && (args.len()==1 || args.iter().any(|s|s=="--help")) {
        println!("{}",accounts_original::help()); return ExitCode::SUCCESS;
    }
    if accounts_original::accepts(&args)
        && let Err(output)=accounts_original::preflight(&args) {eprint!("{}",output.stderr);return ExitCode::from(output.exit);}
    if document_party_original::accepts(&args) {
        if let Err(output) = document_party_original::preflight(&args) {
            eprint!("{}", output.stderr);
            return ExitCode::from(output.exit);
        }
        if args.iter().any(|s| s == "--help") {
            println!("{}", document_party_original::help());
            return ExitCode::SUCCESS;
        }
    }
    if documents_original::accepts(&args) {
        if let Err(output) = documents_original::preflight(&args) {
            eprint!("{}", output.stderr);
            return ExitCode::from(output.exit);
        }
        if args.iter().any(|s| s == "--help") {
            println!("{}", documents_original::help());
            return ExitCode::SUCCESS;
        }
    }
    if args
        .first()
        .is_some_and(|s| matches!(s.as_str(), "customer" | "vendor"))
    {
        if args.len() == 1 || args.get(1).is_some_and(|s| s == "--help") {
            println!("{}", master_data_original::help());
            return ExitCode::SUCCESS;
        }
        if !master_data_original::accepts(&args) {
            eprintln!("Unknown master-data command; see --help");
            return ExitCode::from(2);
        }
        if let Err(output) = master_data_original::preflight(&args) {
            print!("{}", output.stdout);
            eprint!("{}", output.stderr);
            return ExitCode::from(output.exit);
        }
        if args.iter().any(|s| s == "--help") {
            println!("{}", master_data_original::help());
            return ExitCode::SUCCESS;
        }
    }
    if args.first().is_some_and(|s| s == "exceptions") {
        if args.len() == 1 || args.iter().any(|s| s == "--help") {
            println!("{}", exceptions_original::help());
            return ExitCode::SUCCESS;
        }
        if !exceptions_original::accepts(&args) {
            eprintln!("Unknown exceptions command; see exceptions --help");
            return ExitCode::from(2);
        }
        if let Err(output) = exceptions_original::preflight(&args) {
            print!("{}", output.stdout);
            eprint!("{}", output.stderr);
            return ExitCode::from(output.exit);
        }
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
    if args.first().is_some_and(|s| s == "journal") {
        if args == ["journal"] || args == ["journal", "--help"] {
            println!("{}", journal_original::help());
            return ExitCode::SUCCESS;
        }
        if journal_original::accepts(&args) {
            if let Err(output) = journal_original::preflight(&args) {
                print!("{}", output.stdout);
                eprint!("{}", output.stderr);
                return ExitCode::from(output.exit);
            }
            if args.iter().any(|s| s == "--help") {
                println!("{}", journal_original::help());
                return ExitCode::SUCCESS;
            }
        } else if args.get(1).is_none_or(|s| s != "list") {
            eprintln!("Unknown journal command; see journal --help");
            return ExitCode::from(2);
        }
    }
    if audit_verify_original::accepts(&args) {
        match audit_verify_original::preflight(&args) {
            Ok(true) => { println!("{}",audit_verify_original::help()); return ExitCode::SUCCESS; }
            Ok(false) => {}
            Err(o) => { eprint!("{}",o.stderr); return ExitCode::from(o.exit); }
        }
    }
    if workspace_inbox_original::accepts(&args) {
        match workspace_inbox_original::preflight(&args) {
            Ok(true) => { println!("{}",workspace_inbox_original::help()); return ExitCode::SUCCESS; }
            Ok(false) => {}
            Err(o) => { print!("{}",o.stdout); eprint!("{}",o.stderr); return ExitCode::from(o.exit); }
        }
    }
    if party_original::accepts(&args) {
        match party_original::preflight(&args) {
            Ok(true) => { println!("{}",party_original::help()); return ExitCode::SUCCESS; }
            Ok(false) => {}
            Err(o) => { print!("{}",o.stdout); eprint!("{}",o.stderr); return ExitCode::from(o.exit); }
        }
    }
    if corporate_records_original::accepts(&args) {
        match corporate_records_original::preflight(&args) {
            Ok(true) => { println!("{}",corporate_records_original::help()); return ExitCode::SUCCESS; }
            Ok(false) => {}
            Err(o) => { print!("{}",o.stdout); eprint!("{}",o.stderr); return ExitCode::from(o.exit); }
        }
    }
    if ownership_original::accepts(&args) {
        match ownership_original::preflight(&args) {
            Ok(true) => { println!("{}",ownership_original::help()); return ExitCode::SUCCESS; }
            Ok(false) => {}
            Err(o) => { print!("{}",o.stdout); eprint!("{}",o.stderr); return ExitCode::from(o.exit); }
        }
    }
    if group_original::accepts(&args) {
        match group_original::preflight(&args) {
            Ok(true) => { println!("{}",group_original::help()); return ExitCode::SUCCESS; }
            Ok(false) => {}
            Err(o) => { print!("{}",o.stdout); eprint!("{}",o.stderr); return ExitCode::from(o.exit); }
        }
    }
    if import_original::accepts(&args) {
        match import_original::preflight(&args) {
            Ok(true) => { println!("{}",import_original::help()); return ExitCode::SUCCESS; }
            Ok(false) => {}
            Err(o) => { print!("{}",o.stdout); eprint!("{}",o.stderr); return ExitCode::from(o.exit); }
        }
    }
    if company_original::accepts(&args) {
        match company_original::preflight(&args) {
            Ok(true) => { println!("{}",company_original::help()); return ExitCode::SUCCESS; }
            Ok(false) => {}
            Err(o) => { print!("{}",o.stdout); eprint!("{}",o.stderr); return ExitCode::from(o.exit); }
        }
    }
    let registry_args = (import_original::accepts(&args) || group_original::accepts(&args) || company_original::accepts(&args) || workspace_inbox_original::accepts(&args) || party_original::accepts(&args) || corporate_records_original::accepts(&args) || ownership_original::accepts(&args)).then(|| args.clone());
    let audit_args = audit_verify_original::accepts(&args).then(|| args.clone());
    let args = match local_binding::prepare(args) {
        Ok(args) => args,
        Err((code, error)) => {
            if let Some(args) = &registry_args {
                let o = if import_original::accepts(args) { import_original::binding_refusal(args) } else if group_original::accepts(args) { group_original::binding_refusal(args) } else if company_original::accepts(args) { company_original::binding_refusal(args) } else if workspace_inbox_original::accepts(args) { workspace_inbox_original::binding_refusal(args) } else if party_original::accepts(args) { party_original::binding_refusal(args) } else if corporate_records_original::accepts(args) { corporate_records_original::binding_refusal(args) } else { ownership_original::binding_refusal(args) };
                print!("{}",o.stdout); eprint!("{}",o.stderr); return ExitCode::from(o.exit);
            }
            if let Some(args) = &audit_args {
                let o = audit_verify_original::refusal(args,"Audit verification unavailable: local binding refused");
                print!("{}",o.stdout); eprint!("{}",o.stderr); return ExitCode::from(o.exit);
            }
            if code == 2 && std::env::args().any(|s| s == "documents") {
                eprintln!("{}: {}", error.code, error.message);
            } else {
                println!(
                    "{}",
                    json!({"ok":false,"code":error.code,"errors":[error.message]})
                );
            }
            return ExitCode::from(code);
        }
    };
    if group_original::accepts(&args) {
        let o = group_original::execute(&args);
        print!("{}",o.stdout); eprint!("{}",o.stderr); return ExitCode::from(o.exit);
    }
    if import_original::accepts(&args) {
        let o = import_original::execute(&args);
        print!("{}",o.stdout); eprint!("{}",o.stderr); return ExitCode::from(o.exit);
    }
    if company_original::accepts(&args) {
        let o = company_original::execute(&args);
        print!("{}",o.stdout); eprint!("{}",o.stderr); return ExitCode::from(o.exit);
    }
    if audit_verify_original::accepts(&args) {
        let o = audit_verify_original::execute(&args);
        print!("{}",o.stdout); eprint!("{}",o.stderr); return ExitCode::from(o.exit);
    }
    if workspace_inbox_original::accepts(&args) {
        let o = workspace_inbox_original::execute(&args);
        print!("{}",o.stdout); eprint!("{}",o.stderr); return ExitCode::from(o.exit);
    }
    if party_original::accepts(&args) {
        let o = party_original::execute(&args);
        print!("{}",o.stdout); eprint!("{}",o.stderr); return ExitCode::from(o.exit);
    }
    if corporate_records_original::accepts(&args) {
        let o = corporate_records_original::execute(&args);
        print!("{}",o.stdout); eprint!("{}",o.stderr); return ExitCode::from(o.exit);
    }
    if ownership_original::accepts(&args) {
        let o = ownership_original::execute(&args);
        print!("{}",o.stdout); eprint!("{}",o.stderr); return ExitCode::from(o.exit);
    }
    if bookkeeping_batch_original::accepts(&args) {
        let output=bookkeeping_batch_original::execute(&args);
        print!("{}",output.stdout);eprint!("{}",output.stderr);return ExitCode::from(output.exit);
    }
    if accounting_draft_original::accepts(&args) {
        let output=accounting_draft_original::execute(&args);
        print!("{}",output.stdout);eprint!("{}",output.stderr);return ExitCode::from(output.exit);
    }
    if dimensions_original::accepts(&args) {
        let output=dimensions_original::execute(&args);
        print!("{}",output.stdout);eprint!("{}",output.stderr);return ExitCode::from(output.exit);
    }
    if supplier_commitment_original::accepts(&args) {
        let output=supplier_commitment_original::execute(&args);
        print!("{}",output.stdout);eprint!("{}",output.stderr);return ExitCode::from(output.exit);
    }
    if asset_original::accepts(&args) {
        let output=asset_original::execute(&args);
        print!("{}",output.stdout);eprint!("{}",output.stderr);return ExitCode::from(output.exit);
    }
    if mileage_original::accepts(&args) {
        let output=mileage_original::execute(&args);
        print!("{}",output.stdout);eprint!("{}",output.stderr);return ExitCode::from(output.exit);
    }
    if accrual_original::accepts(&args) {
        let output=accrual_original::execute(&args);
        print!("{}",output.stdout);eprint!("{}",output.stderr);return ExitCode::from(output.exit);
    }
    if expense_original::accepts(&args) {
        let output = expense_original::execute(&args);
        print!("{}", output.stdout);
        eprint!("{}", output.stderr);
        return ExitCode::from(output.exit);
    }
    if invoice_extraction_original::accepts(&args) {
        let output = invoice_extraction_original::execute(&args);
        print!("{}", output.stdout);
        eprint!("{}", output.stderr);
        return ExitCode::from(output.exit);
    }
    if let Some(input) = vat_purchase_input {
        let output=vat_purchase_original::execute(&args,input);
        print!("{}",output.stdout);eprint!("{}",output.stderr);return ExitCode::from(output.exit);
    }
    if vat_original::accepts(&args) {
        let output=vat_original::execute(&args);print!("{}",output.stdout);eprint!("{}",output.stderr);return ExitCode::from(output.exit);
    }
    if budget_original::accepts(&args) {
        let output=budget_original::execute(&args);print!("{}",output.stdout);eprint!("{}",output.stderr);return ExitCode::from(output.exit);
    }
    if posting_rules_original::accepts(&args) {let output=posting_rules_original::execute(&args);print!("{}",output.stdout);eprint!("{}",output.stderr);return ExitCode::from(output.exit);}
    if document_party_original::accepts(&args) {
        let output = document_party_original::execute(&args);
        print!("{}", output.stdout);
        eprint!("{}", output.stderr);
        return ExitCode::from(output.exit);
    }
    if documents_original::accepts(&args) {
        let output = documents_original::execute(&args);
        print!("{}", output.stdout);
        eprint!("{}", output.stderr);
        return ExitCode::from(output.exit);
    }
    if master_data_original::accepts(&args) {
        let output = master_data_original::execute(&args);
        print!("{}", output.stdout);
        eprint!("{}", output.stderr);
        return ExitCode::from(output.exit);
    }
    if exceptions_original::accepts(&args) {
        let output = exceptions_original::execute(&args);
        print!("{}", output.stdout);
        eprint!("{}", output.stderr);
        return ExitCode::from(output.exit);
    }
    if journal_original::accepts(&args) {
        let output = journal_original::execute(&args);
        print!("{}", output.stdout);
        eprint!("{}", output.stderr);
        return ExitCode::from(output.exit);
    }
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
    if accounts_original::accepts(&args) {
        let output=accounts_original::execute(&args);
        print!("{}",output.stdout); eprint!("{}",output.stderr);
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

#[cfg(test)]
#[path = "../../rentemester-client/tests/common/mod.rs"]
mod test_protocol;

#[cfg(test)]
mod task_series_dispatch_tests {
    #[test]
    fn original_root_dispatches_series_help_and_refuses_unknown_operations() {
        let boards=super::run(vec!["task-boards".into(),"--help".into()]).unwrap();assert_eq!(boards["inputSchemas"].as_object().unwrap().len(),3);assert!(boards["inputSchemas"]["preview"]["properties"].get("confirm").is_none());assert!(super::run(vec!["task-boards".into(),"run".into()]).is_err());
        let help = super::run(vec!["task-series".into(), "--help".into()]).unwrap();
        assert!(help["usage"].as_str().unwrap().starts_with("task-series list|save|project|materialize"));
        assert_eq!(help["inputSchemas"].as_object().unwrap().len(), 4);
        assert!(super::run(vec!["task-series".into(), "run".into()]).is_err());
        assert!(super::run(vec!["tasks".into(), "--help".into()]).unwrap()["inputSchemas"].get("create").is_some());
    }
}
