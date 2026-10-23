//! Read-only interpretation of storage-qualified Dinero history.
//! Qualification is supplied only by the storage attestation verifier; markers and
//! chart numbers alone are never authority. All amounts use the native accumulator.
use super::{Context, Report, accumulate};
use crate::{Error, money::Money};
use num_bigint::BigInt;
use std::collections::{BTreeMap, BTreeSet};

pub struct Row {
    pub date: String,
    pub line_id: i64,
    pub status: String,
    pub normal_balance: String,
    pub default_vat_code: Option<String>,
    pub line: super::Line,
}
fn net(row: &Row) -> Money {
    row.line.debit.subtract(&row.line.credit)
}
fn difference(a: &Money, b: &Money) -> usize {
    let d = a.subtract(b).0;
    if d == BigInt::from(0) {
        0
    } else if d == BigInt::from(1) || d == BigInt::from(-1) {
        1
    } else {
        2
    }
}
fn code(row: &Row) -> Option<&str> {
    row.line
        .vat_code
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
}
fn profit(row: &Row) -> bool {
    matches!(row.line.account_type.as_str(), "expense" | "income")
}
fn effective<'a>(row: &'a Row, inferred: &'a BTreeMap<i64, String>) -> Option<&'a str> {
    code(row)
        .or_else(|| inferred.get(&row.line_id).map(String::as_str))
        .or_else(|| {
            if profit(row) {
                row.default_vat_code
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
            } else {
                None
            }
        })
}
fn matches(rows: &[&Row], expected: &Money, side: &str) -> Vec<(String, Money)> {
    let mut amounts: BTreeMap<String, Money> = BTreeMap::new();
    for row in rows.iter().filter(|r| !profit(r)) {
        let amount = if side == "input" {
            net(row)
        } else {
            row.line.credit.subtract(&row.line.debit)
        };
        amounts
            .entry(row.line.account_no.clone())
            .or_default()
            .add(&amount);
    }
    amounts
        .into_iter()
        .filter(|(_, a)| *a != Money::default() && difference(a, expected) <= 1)
        .collect()
}
fn choose(
    candidates: Vec<(String, Money)>,
    known: &BTreeMap<String, &'static str>,
    side: &str,
) -> Option<(String, Money)> {
    let preferred: Vec<_> = candidates
        .iter()
        .filter(|(account, _)| known.get(account).copied() == Some(side))
        .collect();
    if preferred.len() == 1 {
        return Some((*preferred[0]).clone());
    }
    if candidates.len() == 1 {
        return candidates.into_iter().next();
    }
    None
}
fn register(
    known: &mut BTreeMap<String, &'static str>,
    account: &str,
    side: &'static str,
    id: i64,
    errors: &mut Vec<String>,
) -> bool {
    if known.get(account).is_some_and(|old| *old != side) {
        errors.push(format!(
            "Postering {id}: konto {account} har modstridende momsside"
        ));
        return false;
    }
    known.insert(account.into(), side);
    true
}
fn allowance(context: &mut Context, id: i64, amount: usize, side: &str) {
    let old = context.entry_tolerance.entry(id).or_default();
    *old = (*old).max(amount);
    match side {
        "input" => context.input_tolerance += amount,
        "output" => context.ordinary_tolerance += amount,
        _ => context.reverse_tolerance += amount,
    }
}

pub fn build(
    rows: &[Row],
    qualified_journal_ids: &BTreeSet<i64>,
    start: &str,
    end: &str,
) -> Result<Report, Error> {
    let mut context = Context::default();
    let invalid_entries: BTreeSet<_> = rows
        .iter()
        .filter(|row| !row.line.historical || row.status != "posted")
        .map(|row| row.line.entry_id)
        .collect();
    let mut entries: BTreeMap<i64, Vec<&Row>> = BTreeMap::new();
    let mut known = BTreeMap::new();
    for row in rows {
        if let Some(side) = row.line.amount_side {
            register(
                &mut known,
                &row.line.account_no,
                side,
                row.line.entry_id,
                &mut context.errors,
            );
        }
        if row.line.historical
            && qualified_journal_ids.contains(&row.line.entry_id)
            && !invalid_entries.contains(&row.line.entry_id)
        {
            entries.entry(row.line.entry_id).or_default().push(row);
            context.qualified.insert(row.line.entry_id);
        }
    }
    let mut inferred = BTreeMap::new();
    let mut dedicated = BTreeSet::new();
    let mut trusted_input = BTreeSet::new();
    let mut trusted_output = BTreeSet::new();
    // Reverse controls are stronger evidence than an account default.
    for (&id, entry) in &entries {
        let expenses: Vec<_> = entry
            .iter()
            .copied()
            .filter(|r| r.line.account_type == "expense")
            .collect();
        let output: Vec<_> = entry
            .iter()
            .copied()
            .filter(|r| r.line.account_no == "64040")
            .collect();
        let input: Vec<_> = entry
            .iter()
            .copied()
            .filter(|r| r.line.account_no == "64060")
            .collect();
        let dinero_pair = !output.is_empty() && !input.is_empty();
        if dinero_pair {
            let canonical = output
                .iter()
                .all(|r| r.line.account_type == "vat" && r.normal_balance == "credit")
                && input
                    .iter()
                    .all(|r| r.line.account_type == "vat" && r.normal_balance == "debit");
            let legacy = output
                .iter()
                .all(|r| r.line.account_type == "liability" && r.normal_balance == "credit")
                && input.iter().all(|r| {
                    r.line.account_type == "liability"
                        && matches!(r.normal_balance.as_str(), "credit" | "debit")
                });
            if !canonical && !legacy {
                context.errors.push(format!(
                    "Postering {id}: conflicting Dinero control account declarations"
                ));
                continue;
            }
            let mut out = Money::default();
            let mut inp = Money::default();
            for row in &output {
                out.add(&row.line.credit.subtract(&row.line.debit));
            }
            for row in &input {
                inp.add(&net(row));
            }
            let explicit: BTreeSet<_> = expenses.iter().filter_map(|r| code(r)).collect();
            let mut bases: Vec<_> = expenses
                .iter()
                .copied()
                .filter(|r| {
                    matches!(
                        code(r),
                        Some("EU_SERVICE_REVERSE_CHARGE" | "NON_EU_SERVICE_REVERSE_CHARGE")
                    )
                })
                .collect();
            let conflicting = explicit.len() > 1
                || explicit.iter().any(|c| {
                    !matches!(
                        *c,
                        "EU_SERVICE_REVERSE_CHARGE" | "NON_EU_SERVICE_REVERSE_CHARGE"
                    )
                });
            if bases.is_empty() && explicit.is_empty() {
                for row in &expenses {
                    let amount = net(row);
                    if amount != Money::default()
                        && amount.is_negative() == out.is_negative()
                        && difference(&amount.percent("25")?, &out) <= 1
                    {
                        bases.push(*row);
                    }
                }
                if bases.len() != 1 {
                    bases.clear();
                }
            }
            let mut base = Money::default();
            for row in &bases {
                base.add(&net(row));
            }
            if conflicting
                || bases.is_empty()
                || out == Money::default()
                || inp != out
                || difference(&base.percent("25")?, &out) > 1
            {
                context.errors.push(format!(
                    "Postering {id}: inconsistent Dinero reverse-charge control/base pattern"
                ));
                continue;
            }
            if !register(&mut known, "64040", "output", id, &mut context.errors) {
                continue;
            }
            if canonical && !register(&mut known, "64060", "input", id, &mut context.errors) {
                continue;
            }
            for row in &bases {
                if code(row).is_none() {
                    inferred.insert(row.line_id, "EU_SERVICE_REVERSE_CHARGE".into());
                }
            }
            // Legacy 64060 credit-normal liability is overridden only for these proven lines.
            for row in input {
                trusted_input.insert(row.line_id);
            }
            for row in output {
                trusted_output.insert(row.line_id);
            }
            dedicated.insert("64040".to_owned());
            if in_range(entry[0], start, end) {
                allowance(
                    &mut context,
                    id,
                    difference(&base.percent("25")?, &out),
                    "reverse",
                );
            }
            continue;
        }
        if expenses
            .iter()
            .filter_map(|r| code(r))
            .any(|c| c != "EU_SERVICE_REVERSE_CHARGE")
        {
            continue;
        }
        let explicit = expenses.iter().any(|r| code(r).is_some());
        let mut shapes = Vec::new();
        for base in expenses
            .iter()
            .filter(|r| !explicit || code(r) == Some("EU_SERVICE_REVERSE_CHARGE"))
        {
            let expected = net(base).percent("25")?;
            if expected == Money::default() {
                continue;
            }
            let inp = choose(matches(entry, &expected, "input"), &known, "input");
            let out = choose(matches(entry, &expected, "output"), &known, "output");
            if let (Some(inp), Some(out)) = (inp, out) {
                if inp.0 != out.0 && inp.1 == out.1 {
                    shapes.push((*base, expected, inp, out));
                }
            }
        }
        if shapes.len() > 1 {
            context
                .errors
                .push(format!("Postering {id}: ambiguous reverse-charge base"));
        }
        if shapes.len() != 1 {
            continue;
        }
        let (base, expected, inp, out) = &shapes[0];
        if !register(&mut known, &inp.0, "input", id, &mut context.errors)
            || !register(&mut known, &out.0, "output", id, &mut context.errors)
        {
            continue;
        }
        if code(base).is_none() {
            inferred.insert(base.line_id, "EU_SERVICE_REVERSE_CHARGE".into());
        }
        dedicated.insert(out.0.clone());
        if in_range(entry[0], start, end) {
            allowance(&mut context, id, difference(expected, &inp.1), "reverse");
        }
    }
    for (&id, entry) in &entries {
        for (category, side) in [("DK_PURCHASE_25", "input"), ("DK_SALE_25", "output")] {
            let codes: BTreeSet<_> = entry
                .iter()
                .filter_map(|r| effective(r, &inferred))
                .collect();
            if codes.len() != 1 || !codes.contains(category) {
                continue;
            }
            let mut base = Money::default();
            for row in entry
                .iter()
                .filter(|r| profit(r) && effective(r, &inferred) == Some(category))
            {
                base.add(&if side == "input" {
                    net(row)
                } else {
                    row.line.credit.subtract(&row.line.debit)
                });
            }
            let expected = base.percent("25")?;
            if expected == Money::default() {
                continue;
            }
            let candidates = matches(entry, &expected, side);
            let ambiguous = candidates.len() > 1;
            let Some(control) = choose(candidates, &known, side) else {
                if ambiguous {
                    context.errors.push(format!(
                        "Postering {id}: multiple accounts match VAT control"
                    ));
                }
                continue;
            };
            if register(&mut known, &control.0, side, id, &mut context.errors)
                && in_range(entry[0], start, end)
            {
                allowance(&mut context, id, difference(&expected, &control.1), side);
            }
        }
    }
    let mut lines = Vec::new();
    for row in rows.iter().filter(|r| in_range(r, start, end)) {
        let mut line = row.line.clone();
        if context.qualified.contains(&line.entry_id) && row.status == "posted" {
            line.vat_code = effective(row, &inferred).map(str::to_owned);
            if trusted_input.contains(&row.line_id) {
                line.amount_side = Some("input");
            } else if trusted_output.contains(&row.line_id) {
                line.amount_side = Some("output");
            } else if let Some(side) = known.get(&line.account_no) {
                line.amount_side = Some(*side);
            }
            line.dedicated_reverse_charge |= dedicated.contains(&line.account_no);
        }
        lines.push(line);
    }
    for (&id, entry) in &entries {
        if !in_range(entry[0], start, end) {
            continue;
        }
        let reps: Vec<_> = entry
            .iter()
            .filter(|r| {
                r.line.account_type == "expense" && code(r) == Some("REPRESENTATION_SPECIAL")
            })
            .collect();
        if reps.len() != 1
            || entry.iter().any(|r| {
                code(r) == Some("REPRESENTATION_NON_DEDUCTIBLE_VAT")
                    || (profit(r) && r.line_id != reps[0].line_id)
            })
        {
            continue;
        }
        let inputs: Vec<_> = lines
            .iter()
            .filter(|l| l.entry_id == id && l.amount_side == Some("input"))
            .collect();
        if inputs.len() != 1 {
            continue;
        }
        let expense = net(reps[0]);
        let input = inputs[0].debit.subtract(&inputs[0].credit);
        if expense.is_negative()
            || input.is_negative()
            || expense == Money::default()
            || input == Money::default()
        {
            continue;
        }
        let mut gross = expense.clone();
        gross.add(&input);
        let recovered = Money(crate::decimal::round_div(
            &(&gross.0 * BigInt::from(4)),
            &BigInt::from(5),
        )?);
        let full = recovered.percent("25")?;
        let expected_input = full.percent("25")?;
        let mut expected_expense = recovered.clone();
        expected_expense.add(&full.subtract(&expected_input));
        if difference(&expected_input, &input) > 1 || difference(&expected_expense, &expense) > 1 {
            context.errors.push(format!(
                "Postering {id}: collapsed representation does not reconcile"
            ));
            continue;
        }
        context.representation.insert(id, recovered);
        if difference(&expected_input, &input) == 1 {
            allowance(&mut context, id, 1, "input");
        }
    }
    accumulate(&lines, &context)
}
fn in_range(row: &Row, start: &str, end: &str) -> bool {
    row.date.as_str() >= start && row.date.as_str() <= end
}

#[cfg(test)]
mod tests {
    use super::super::Line;
    use super::*;
    fn row(
        id: i64,
        line_id: i64,
        account: &str,
        kind: &str,
        code: Option<&str>,
        debit: &str,
        credit: &str,
    ) -> Row {
        Row {
            date: "2026-04-01".into(),
            line_id,
            status: "posted".into(),
            normal_balance: "credit".into(),
            default_vat_code: None,
            line: Line {
                entry_id: id,
                account_no: account.into(),
                account_type: kind.into(),
                vat_code: code.map(str::to_owned),
                debit: Money::parse(debit).unwrap(),
                credit: Money::parse(credit).unwrap(),
                amount_side: None,
                dedicated_reverse_charge: false,
                settlement: false,
                foreign: false,
                historical: true,
                exact_sales_vat: None,
            },
        }
    }
    fn purchase(id: i64) -> Vec<Row> {
        vec![
            row(
                id,
                id * 10,
                "cost",
                "expense",
                Some("DK_PURCHASE_25"),
                "100",
                "0",
            ),
            row(id, id * 10 + 1, "control", "liability", None, "25", "0"),
            row(id, id * 10 + 2, "finance", "asset", None, "0", "125"),
        ]
    }
    fn report(rows: &[Row], ids: &[i64]) -> Report {
        build(
            rows,
            &ids.iter().copied().collect(),
            "2026-04-01",
            "2026-06-30",
        )
        .unwrap()
    }
    #[test]
    fn ordinary_purchase_sale_and_reversal_share_accumulator() {
        let mut rows = purchase(1);
        rows.extend([
            row(2, 20, "sales", "income", Some("DK_SALE_25"), "0", "200"),
            row(2, 21, "sales-control", "liability", None, "0", "50"),
            row(2, 22, "finance", "asset", None, "250", "0"),
        ]);
        let r = report(&rows, &[1, 2]);
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        assert_eq!(r.canonical_bases["purchaseBase25"].decimal(), "100.00");
        assert_eq!(r.canonical_bases["salesBase25"].decimal(), "200.00");
        assert_eq!(r.payable.decimal(), "25.00");
        for row in &mut rows {
            std::mem::swap(&mut row.line.debit, &mut row.line.credit);
        }
        let r = report(&rows, &[1, 2]);
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        assert_eq!(r.payable.decimal(), "-25.00");
    }
    #[test]
    fn learned_outside_range_never_qualifies_unrelated_history_or_native_defaults() {
        let mut rows = purchase(1);
        for row in &mut rows {
            row.date = "2026-01-01".into();
        }
        rows.extend(purchase(2));
        rows[3].line.vat_code = None;
        rows[3].default_vat_code = Some("DK_PURCHASE_25".into());
        let r = report(&rows, &[1, 2]);
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        assert_eq!(r.input_vat.decimal(), "25.00");
        assert!(!report(&rows, &[1]).errors.is_empty());
        for row in &mut rows[3..] {
            row.line.historical = false;
        }
        rows[4].line.amount_side = Some("input");
        assert!(!report(&rows, &[1, 2]).errors.is_empty());
    }
    #[test]
    fn ambiguous_controls_and_conflicting_sides_fail_closed() {
        let mut rows = purchase(1);
        rows.push(row(1, 14, "second-control", "asset", None, "25", "0"));
        assert!(
            report(&rows, &[1])
                .errors
                .iter()
                .any(|e| e.contains("multiple accounts"))
        );
        let mut rows = purchase(1);
        rows[1].line.amount_side = Some("output");
        assert!(
            report(&rows, &[1])
                .errors
                .iter()
                .any(|e| e.contains("modstridende"))
        );
    }
    fn reverse() -> Vec<Row> {
        vec![
            row(1, 10, "cost", "expense", None, "100", "0"),
            row(1, 11, "64040", "liability", None, "0", "25"),
            row(1, 12, "64060", "liability", None, "25", "0"),
            row(1, 13, "finance", "asset", None, "0", "100"),
        ]
    }
    #[test]
    fn dinero_pair_requires_qualification_equal_controls_and_consistent_explicit_code() {
        let mut rows = reverse();
        rows[2].line.amount_side = Some("output"); // legacy credit-normal chart
        let r = report(&rows, &[1]);
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        assert_eq!(r.rubrikker.rubrik_a.decimal(), "100.00");
        assert_eq!(r.input_vat.decimal(), "25.00");
        assert!(!report(&rows, &[]).errors.is_empty());
        rows[0].line.vat_code = Some("NON_EU_SERVICE_REVERSE_CHARGE".into());
        let r = report(&rows, &[1]);
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        assert_eq!(
            r.canonical_bases["nonEuServiceReverseChargePurchaseBase"].decimal(),
            "100.00"
        );
        rows[2].line.debit = Money::parse("24.99").unwrap();
        assert!(!report(&rows, &[1]).errors.is_empty());
        rows[2].line.debit = Money::parse("25").unwrap();
        rows[0].line.vat_code = Some("DK_PURCHASE_25".into());
        assert!(!report(&rows, &[1]).errors.is_empty());
    }
    #[test]
    fn collapsed_representation_changes_only_reported_base() {
        let mut rows = vec![
            row(
                1,
                10,
                "cost",
                "expense",
                Some("REPRESENTATION_SPECIAL"),
                "118.75",
                "0",
            ),
            row(1, 11, "input", "vat", None, "6.25", "0"),
            row(1, 12, "finance", "asset", None, "0", "125"),
        ];
        rows[1].line.amount_side = Some("input");
        let r = report(&rows, &[1]);
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        assert_eq!(r.representation_purchase_base.decimal(), "100.00");
        assert_eq!(rows[0].line.debit.decimal(), "118.75");
        rows[1].line.debit = Money::parse("7").unwrap();
        assert!(!report(&rows, &[1]).errors.is_empty());
    }
    #[test]
    fn amount_only_is_qualified_only_and_settlement_is_excluded() {
        let mut rows = vec![
            row(1, 10, "input", "vat", None, "5", "0"),
            row(1, 11, "finance", "asset", None, "0", "5"),
        ];
        rows[0].line.amount_side = Some("input");
        let r = report(&rows, &[1]);
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        assert_eq!(r.input_vat.decimal(), "5.00");
        assert!(!r.warnings.is_empty());
        assert!(!report(&rows, &[]).errors.is_empty());
        for row in &mut rows {
            row.line.historical = false;
        }
        assert!(!report(&rows, &[1]).errors.is_empty());
        rows[1].line.settlement = true;
        let r = report(&rows, &[]);
        assert!(r.errors.is_empty());
        assert_eq!(r.payable, Money::default());
    }
    #[test]
    fn opposite_category_errors_and_out_of_period_tolerance_do_not_cancel() {
        let mut rows = reverse();
        rows[0].line.vat_code = Some("EU_SERVICE_REVERSE_CHARGE".into());
        // Establish dedicated meaning in an earlier, valid voucher.
        for row in &mut rows {
            row.date = "2026-01-01".into();
        }
        let mut later = reverse();
        for row in &mut later {
            row.line.entry_id = 2;
            row.line_id += 20;
        }
        later[0].line.vat_code = Some("EU_SERVICE_REVERSE_CHARGE".into());
        later[1].line.credit = Money::parse("24").unwrap();
        rows.extend(later);
        rows.extend([
            row(3, 40, "sale", "income", Some("DK_SALE_25"), "0", "100"),
            row(3, 41, "ordinary", "vat", None, "0", "26"),
        ]);
        rows.last_mut().unwrap().line.amount_side = Some("output");
        assert!(!report(&rows, &[1, 2, 3]).errors.is_empty());
        let mut rows = purchase(1);
        rows[1].line.debit = Money::parse("25.01").unwrap();
        for row in &mut rows {
            row.date = "2026-01-01".into();
        }
        rows.extend(purchase(2));
        rows[4].line.debit = Money::parse("25.02").unwrap();
        assert!(!report(&rows, &[1, 2]).errors.is_empty());
    }
}
