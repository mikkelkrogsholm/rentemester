//! Source-backed argv structure only. Family adapters own allowed flags and domain types.
use rentemester_engine::Error;

/// Split only the first equals sign; attached values are always literal data.
pub fn option(token: &str) -> (&str, Option<&str>) {
    token
        .split_once('=')
        .map_or((token, None), |(k, v)| (k, Some(v)))
}

pub fn value(args: &[String], index: usize) -> Option<(&str, usize)> {
    let (_, attached) = option(args.get(index)?);
    if let Some(value) = attached {
        return Some((value, index + 1));
    }
    args.get(index + 1)
        .filter(|v| !v.starts_with("--"))
        .map(|v| (v.as_str(), index + 2))
}

fn boolean(name: &str) -> bool {
    matches!(
        name,
        "--force"
            | "--help"
            | "--example"
            | "--json"
            | "--sign-with-ed25519"
            | "--archive"
            | "--enrich-cvr"
            | "--include-archived"
            | "--ixbrl-taxonomy"
            | "--after-retention-expiry"
            | "--no-vat"
            | "--no-open"
            | "--allow-loopback-http"
            | "--include-inactive"
    )
}

fn migrated(args: &[String]) -> bool {
    crate::exceptions_original::accepts(args)
        || crate::journal_original::accepts(args)
        || crate::period_original::accepts(args)
        || crate::recurring_original::accepts(args)
        || crate::invoice_financial_original::accepts(args)
        || crate::invoice_public_original::accepts(args)
        || crate::invoice_claims_original::accepts(args)
        || crate::invoice_lifecycle::accepts(args)
        || crate::invoice_original::accepts(args)
        || crate::payable_original::accepts(args)
        || crate::direct_bank_original::accepts(args)
        || crate::bank_original::accepts(args)
        || crate::bank_accounts_original::accepts(args)
        || crate::bank_import::accepts(args)
        || crate::bank_queries::accepts(args)
        || matches!(
            (
                args.first().map(String::as_str),
                args.get(1).map(String::as_str)
            ),
            (
                Some("report"),
                Some("trial-balance" | "profit-loss" | "balance" | "annual" | "tax")
            ) | (Some("journal"), Some("list"))
                | (
                    Some("system"),
                    Some("export-saft" | "export-authority" | "export-accountant")
                )
        )
}

/// Actual migrated company commands, never a request to choose a default company.
pub fn requires_company(args: &[String]) -> bool {
    let taxonomy = args.first().is_some_and(|s| s == "report")
        && args.get(1).is_some_and(|s| s == "annual")
        && args
            .iter()
            .skip(2)
            .take_while(|s| s.as_str() != "--")
            .any(|s| s == "--ixbrl-taxonomy");
    migrated(args)
        && !taxonomy
        && !matches!(
            (
                args.first().map(String::as_str),
                args.get(1).map(String::as_str)
            ),
            (Some("invoice"), Some("validate"))
                | (Some("recurring-invoice"), Some("run-workspace"))
        )
}

fn usage(message: String) -> (u8, Error) {
    (2, Error::new("invalid_input", message))
}

pub fn prepare(args: Vec<String>) -> Result<Vec<String>, (u8, Error)> {
    // Locate command words without mistaking option values for commands. Retain
    // their indices so only command words move, not search words or literal data.
    let mut words = Vec::new();
    let mut cursor = 0;
    let mut terminated = false;
    while cursor < args.len() {
        let token = &args[cursor];
        if !terminated && token == "--" {
            terminated = true;
            cursor += 1;
            continue;
        }
        if !terminated && token.starts_with("--") {
            let (name, attached) = option(token);
            cursor = if attached.is_some() || boolean(name) {
                cursor + 1
            } else {
                value(&args, cursor).map_or(cursor + 1, |(_, next)| next)
            };
        } else {
            words.push(cursor);
            cursor += 1;
        }
    }
    let command: Vec<String> = words.iter().take(2).map(|&i| args[i].clone()).collect();
    let root_help = command.len() <= 1
        && command.first().is_some_and(|s| {
            matches!(
                s.as_str(),
                "exceptions"
                    | "period"
                    | "journal"
                    | "invoice"
                    | "bank"
                    | "bank-account"
                    | "reconcile"
                    | "payable"
                    | "recurring-invoice"
                    | "report"
                    | "system"
            )
        });
    if !migrated(&command) && !root_help {
        return Ok(args); // Native remote and experimental local dialects are opaque.
    }
    let command_indices: Vec<usize> = words.into_iter().take(2).collect();
    let period = command.first().is_some_and(|s| s == "period");
    let mut seen = std::collections::BTreeSet::new();
    let mut output = command;
    let mut format = None;
    let mut json = false;
    let mut cursor = 0;
    while cursor < args.len() {
        if command_indices.contains(&cursor) {
            cursor += 1;
            continue;
        }
        let token = &args[cursor];
        if token == "--" {
            output.extend(
                args.iter()
                    .enumerate()
                    .skip(cursor)
                    .filter(|(index, _)| !command_indices.contains(index))
                    .map(|(_, token)| token.clone()),
            );
            break;
        }
        if !token.starts_with("--") {
            output.push(token.clone());
            cursor += 1;
            continue;
        }
        let (name, attached) = option(token);
        if period && !seen.insert(name) {
            return Err(usage(format!("Duplicate {name}")));
        }
        if boolean(name) {
            if attached.is_some() {
                return Err(usage(format!("Flag {name} does not take a value")));
            }
            if name == "--json" {
                json = true;
            } else {
                output.push(token.clone());
            }
            cursor += 1;
        } else {
            let (raw, next) = value(&args, cursor)
                .ok_or_else(|| usage(format!("Flag {name} requires a value")))?;
            if name == "--format" {
                if !matches!(raw, "json" | "human") {
                    return Err(usage("--format must be json or human".into()));
                }
                format = Some(raw);
            } else {
                output.extend_from_slice(&args[cursor..next]);
            }
            cursor = next;
        }
    }
    // Source precedence: --json wins regardless of its position; the last
    // --format otherwise wins. Human is the adapters' existing no-json mode.
    if json || format == Some("json") {
        let end = output
            .iter()
            .position(|s| s == "--")
            .unwrap_or(output.len());
        output.insert(end, "--json".into());
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| (*v).into()).collect()
    }
    #[test]
    fn literal_values_split_once_and_advance_exactly() {
        assert_eq!(option("--note=a=b"), ("--note", Some("a=b")));
        let input = args(&[
            "--note=--literal",
            "--empty=",
            "--company",
            "synthetic",
            "--missing",
            "--json",
        ]);
        assert_eq!(value(&input, 0), Some(("--literal", 1)));
        assert_eq!(value(&input, 1), Some(("", 2)));
        assert_eq!(value(&input, 2), Some(("synthetic", 4)));
        assert_eq!(value(&input, 4), None);
        assert_eq!(value(&input, 6), None);
    }
    #[test]
    fn metadata_order_and_terminator_preserve_search_data() {
        let input = args(&[
            "--company=s",
            "invoice",
            "--server",
            "https://example.invalid",
            "find",
            "first",
            "--customer=--literal",
            "--",
            "--json",
            "last",
        ]);
        assert_eq!(
            prepare(input).unwrap(),
            args(&[
                "invoice",
                "find",
                "--company=s",
                "--server",
                "https://example.invalid",
                "first",
                "--customer=--literal",
                "--",
                "--json",
                "last"
            ])
        );
    }
    #[test]
    fn journal_list_metadata_moves_without_changing_help() {
        for input in [
            vec!["--company", "synthetic", "journal", "list", "--help"],
            vec!["journal", "--company", "synthetic", "list", "--help"],
            vec!["journal", "list", "--company", "synthetic", "--help"],
        ] {
            let prepared = prepare(args(&input)).unwrap();
            assert_eq!(
                prepared,
                args(&["journal", "list", "--company", "synthetic", "--help"])
            );
            assert!(requires_company(&prepared));
        }
        assert_eq!(
            prepare(args(&["journal", "list", "--help"])).unwrap(),
            args(&["journal", "list", "--help"])
        );
    }
    #[test]
    fn terminator_command_words_move_once_and_remaining_words_stay_literal() {
        for (input, expected) in [
            (
                vec!["--", "invoice", "find", "literal"],
                vec!["invoice", "find", "--", "literal"],
            ),
            (
                vec![
                    "--company=s",
                    "--",
                    "invoice",
                    "find",
                    "--json",
                    "--",
                    "last",
                ],
                vec![
                    "invoice",
                    "find",
                    "--company=s",
                    "--",
                    "--json",
                    "--",
                    "last",
                ],
            ),
            (
                vec!["invoice", "--", "find", "--format=csv", "invoice", "find"],
                vec!["invoice", "find", "--", "--format=csv", "invoice", "find"],
            ),
        ] {
            assert_eq!(prepare(args(&input)).unwrap(), args(&expected));
        }
        let native = args(&["--", "remote", "report-export", "--format=csv"]);
        assert_eq!(prepare(native.clone()).unwrap(), native);
    }
    #[test]
    fn format_precedence_and_usage_are_pure() {
        assert_eq!(
            prepare(args(&["--format=human", "invoice", "list", "--json"])).unwrap(),
            args(&["invoice", "list", "--json"])
        );
        assert_eq!(
            prepare(args(&[
                "invoice",
                "list",
                "--format",
                "json",
                "--format=human"
            ]))
            .unwrap(),
            args(&["invoice", "list"])
        );
        for flags in [
            vec!["--format=csv"],
            vec!["--format"],
            vec!["--json=false"],
            vec!["--company"],
        ] {
            let mut input = args(&["invoice", "list"]);
            input.extend(args(&flags));
            assert_eq!(prepare(input).unwrap_err().0, 2);
        }
    }
    #[test]
    fn native_dialects_are_opaque_and_company_predicate_is_bounded() {
        for input in [
            vec!["remote", "report-export", "--format", "csv"],
            vec!["trial-balance", "--company=x", "--format=pdf"],
            vec!["dashboard", "--json=false"],
        ] {
            let input = args(&input);
            assert_eq!(prepare(input.clone()).unwrap(), input);
            assert!(!requires_company(&input));
        }
        for input in [
            vec!["invoice", "list"],
            vec!["bank", "list"],
            vec!["report", "annual"],
            vec!["recurring-invoice", "generate"],
        ] {
            assert!(requires_company(&args(&input)));
        }
        for input in [
            vec!["invoice", "validate"],
            vec!["recurring-invoice", "run-workspace"],
            vec!["report", "annual", "--ixbrl-taxonomy"],
            vec![
                "report",
                "annual",
                "--ixbrl-taxonomy",
                "--company",
                "/private/not-opened",
            ],
            vec!["--help"],
            vec!["invoice", "unknown"],
        ] {
            assert!(!requires_company(&args(&input)));
        }
    }
}
