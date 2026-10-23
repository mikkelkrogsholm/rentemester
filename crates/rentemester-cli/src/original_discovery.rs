//! Root discovery is kept separate from transport and local profile selection.
use serde_json::{Value, json};

pub fn help() -> Value {
    json!({"ok":true,"localBinding":"Existing migrated commands may select --local-profile ABS_PRIVATE_PROFILE instead of --server/--allow-loopback-http. Join only: same server via private Unix socket with normal cookie/service authentication. Bounded company selection supports explicit slug or verified workspace path and RENTEMESTER_COMPANY fallback for migrated company commands only. Identity remains server-owned; no caller-selected actor. Whole-product parity is unfinished and the default launcher is unchanged.","commands":[
        {"name":"supplier-commitment plan|apply|list|change|match|matches|alerts","usage":"supplier-commitment --help","scope":"inline JSON, flat native source-bound proposals/receipts, explicit approval; native 90-day schedule, not offline pure TS hash parity"},
        {"name":"vat post-eu-service-purchase|post-representation-purchase","usage":"vat post-eu-service-purchase --input FILE.json","scope":"original confirmed-by-invocation purchase writes; representation omission requires unique active configured REPRESENTATION_SPECIAL expense account, never fixed 3070; optional actor asserts current credential only"},
        {"name":"vat report|eu-sales-list|oss-report|momsangivelse|filing","usage":"vat --help","scope":"read-only source VAT reports and closed-period filing preparation; no submission"},
        {"name":"budget set|list|vs-actual|forecast|forecast-13-week","usage":"budget --help","scope":"original flat budget revisions and read-only source cash planning; native aliases retained"},
        {"name":"asset register|depreciate|write-off|register-report","usage":"asset --help","scope":"original source asset lifecycle; native remote aliases retained"},
        {"name":"mileage log|list|report|export","usage":"mileage --help","scope":"original mileage log and local JSON/CSV export; native aliases retained"},
        {"name":"accrual register|recognize|register-report","usage":"accrual --help","scope":"original flat accrual journal writes and exposure report; native remote aliases retained"},
        {"name":"expense vat-preflight|book","usage":"expense --help","scope":"original document-local read/apply VAT evidence and atomic expense booking; six CLI treatments; native aliases under remote"},
        {"name":"customer create|list|validate-vat|cvr-lookup; vendor create|list","usage":"customer create --help; vendor create --help","scope":"original source-flat master data using current shared server writers/providers"},
        {"name":"bank list|reconcile bank|invoice issue|invoice create|invoice render|invoice post|invoice settle-bank|invoice credit-note|invoice refund-bank|invoice apply-payment|invoice repair-posting|invoice list|invoice find|invoice status|invoice overdue|invoice remind|invoice post-reminder|invoice compensation|invoice claim-compensation|invoice post-compensation|invoice interest|invoice claim-interest|invoice post-interest|invoice interest-correction|invoice post-interest-correction|invoice settle-claim-bank|invoice write-off-bad-debt|invoice imported-receivables|invoice validate|payable register|payable list|payable pay|bank direct-payable-plan|bank direct-payable-apply|bank suggest-matches|bank link-journal|bank correction-plan|bank correction-apply|bank-account list|bank-account add|bank-account update|bank import --file PATH","usage":"bank list --help","scope":"original canonical bank reads and reconciliation, unpaged CLI output"},
        {"name":"exceptions list|resolve","usage":"exceptions --help","scope":"original flat task reads and explicit review; server-owned identity"},
        {"name":"period readiness|review|status|close|reopen","usage":"period --help","scope":"original server-owned period evidence and lifecycle; no force bypass"},
        {"name":"recurring-invoice create|generate|list|run-workspace","usage":"recurring-invoice list --help; recurring-invoice run-workspace --help","scope":"server-owned templates and generation; explicit workspace runner, no local scheduler or automatic retry"},
        {"name":"system export-saft","usage":"system export-saft --help","scope":"one audited SAF-T export and verified fresh local package"},
        {"name":"system export-authority|export-accountant","usage":"system export-authority --help","scope":"one audited export and verified fresh local handover package"},
        {"name":"report annual|tax","usage":"report annual --help; report tax --help","scope":"closed-year preparation and verified optional XHTML; no submission"},
        {"name":"invoice export-public|invoice export-public-oioubl|invoice submit-public-peppol|invoice transmit-digisense","usage":"invoice export-public --help","scope":"original public invoice exports, metadata handoff and configured provider delivery; verified local artifacts"},
        {"name":"report trial-balance|profit-loss|balance","usage":"report --help","scope":"original dated numeric reports; explicit authenticated server, human/JSON streams"},
        {"name":"journal post|dry-run|reverse|list","usage":"journal --help","scope":"original numeric manual journal operations, rollback-only dry-run, server-side reversal selection; native journal list retained"},
        {"name":"dashboard","usage":"dashboard --server ORIGIN --cookie-file PATH --company SLUG --out FILE.html [--as-of YYYY-MM-DD] [--open]","scope":"authenticated server HTML, atomic local output"},
        {"name":"remote","usage":"remote --help","scope":"explicit authenticated Rust HTTP server; no local fallback"},
        {"name":"documents","usage":"documents --help","scope":"original intake/list/enrich/company-context/parse/parse-pending/parse-status/parsed-text plus seven party-link/internal decision commands and two source-flat invoice extraction commands (17 total); native extraction remains under remote"},
        {"name":"validate-journal","usage":"validate-journal --company PATH < request.json","scope":"experimental source-backed advisory manual validation; no writer authority"},
        {"name":"trial-balance","usage":"trial-balance --company PATH --from YYYY-MM-DD --to YYYY-MM-DD","scope":"experimental local read-only report"},
        {"name":"rehearse-company-baseline","usage":"rehearse-company-baseline --company PATH","scope":"experimental explicit known unversioned physical baseline copy; no adoption authority"},
        {"name":"rehearse-control-migration","usage":"rehearse-control-migration --workspace PATH","scope":"experimental private control copy; no source migration"},
        {"name":"rehearse-company-migration","usage":"rehearse-company-migration --company PATH","scope":"experimental full canonical company prefixes0–36 and bounded minimal suffix32–36; no source migration"},
        {"name":"verify-journal-hashes","usage":"verify-journal-hashes --company PATH","scope":"experimental journal canonical hashes; not full audit integrity"}
    ]})
}

/// Resolve root discovery before transport/profile selection. This is argv-only:
/// even explicitly supplied credential and profile paths are never opened here.
pub fn execute(args: &[String]) -> Option<crate::dated_reports::Output> {
    use crate::original_args::{option, value};
    // Recognized command roots belong to their existing dispatchers, including
    // native remote and experimental local dialects. Do not reinterpret help.
    let mut cursor = 0;
    while cursor < args.len() {
        let (name, attached) = option(&args[cursor]);
        if !name.starts_with("--") {
            if recognized(name) {
                return None;
            }
            break;
        }
        if matches!(
            name,
            "--help" | "--version" | "--json" | "--allow-loopback-http"
        ) || attached.is_some()
        {
            cursor += 1;
        } else {
            cursor = value(args, cursor).map_or(cursor + 1, |(_, next)| next);
        }
    }
    if args == ["--version"] {
        return Some(crate::dated_reports::Output {
            stdout: format!("rentemester-rust {}\n", env!("CARGO_PKG_VERSION")),
            stderr: String::new(),
            exit: 0,
        });
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut help_word = false;
    cursor = 0;
    while cursor < args.len() {
        let (name, attached) = option(&args[cursor]);
        if name == "help" && !help_word {
            help_word = true;
            cursor += 1;
            continue;
        }
        if !name.starts_with("--") {
            return Some(invalid(&format!("Unknown command {name:?}")));
        }
        if !seen.insert(name) && name != "--help" {
            return Some(invalid(&format!("Duplicate {name}")));
        }
        if matches!(name, "--help" | "--json" | "--allow-loopback-http") {
            if attached.is_some() {
                return Some(invalid(&format!("Flag {name} does not take a value")));
            }
            cursor += 1;
            continue;
        }
        if !matches!(
            name,
            "--local-profile"
                | "--server"
                | "--cookie-file"
                | "--service-key-file"
                | "--company"
                | "--workspace"
                | "--format"
        ) {
            return Some(invalid(&format!("Unknown root option {name}")));
        }
        let Some((raw, next)) = value(args, cursor) else {
            return Some(invalid(&format!("Flag {name} requires a value")));
        };
        if raw.is_empty() {
            return Some(invalid(&format!("Flag {name} requires a nonempty value")));
        }
        if name == "--local-profile" && !std::path::Path::new(raw).is_absolute() {
            return Some(invalid(
                "--local-profile requires an absolute private profile path",
            ));
        }
        if name == "--format" && !matches!(raw, "human" | "json") {
            return Some(invalid("--format must be json or human"));
        }
        cursor = next;
    }
    if seen.contains("--local-profile")
        && (seen.contains("--server") || seen.contains("--allow-loopback-http"))
    {
        return Some(invalid(
            "--local-profile is mutually exclusive with --server and --allow-loopback-http",
        ));
    }
    if seen.contains("--cookie-file") && seen.contains("--service-key-file") {
        return Some(invalid(
            "Choose exactly one --cookie-file/--service-key-file",
        ));
    }
    if !args.is_empty() && !help_word && !seen.contains("--help") {
        return Some(invalid("Expected a command, help or standalone --version"));
    }
    Some(crate::dated_reports::Output {
        stdout: format!("{}\n", help()),
        stderr: String::new(),
        exit: 0,
    })
}

fn invalid(message: &str) -> crate::dated_reports::Output {
    crate::dated_reports::Output {
        stdout: String::new(),
        stderr: format!("invalid_input: {message}; see rentemester-rust --help\n"),
        exit: 2,
    }
}

fn recognized(root: &str) -> bool {
    matches!(
        root,
        "supplier-commitment" | "vat" | "asset"
            | "accrual"
            | "mileage"
            | "budget"
            | "customer"
            | "expense"
            | "vendor"
            | "exceptions"
            | "period"
            | "bank"
            | "bank-account"
            | "reconcile"
            | "invoice"
            | "payable"
            | "recurring-invoice"
            | "system"
            | "report"
            | "journal"
            | "dashboard"
            | "remote"
            | "documents"
            | "validate-journal"
            | "trial-balance"
            | "rehearse-company-baseline"
            | "rehearse-control-migration"
            | "rehearse-company-migration"
            | "verify-journal-hashes"
    )
}
