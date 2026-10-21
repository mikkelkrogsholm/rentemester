//! Exact native VAT control and base reconciliation. No account-number conventions.
use crate::{Error, journal_rules::VAT_CODES, money::Money};
use num_bigint::BigInt;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

pub struct Line {
    pub entry_id: i64,
    pub account_no: String,
    pub account_type: String,
    pub vat_code: Option<String>,
    pub debit: Money,
    pub credit: Money,
    pub amount_side: Option<&'static str>,
    pub dedicated_reverse_charge: bool,
    pub settlement: bool,
    pub foreign: bool,
    pub historical: bool,
    /// Exact contained VAT from a verified immutable sales allocation.
    pub exact_sales_vat: Option<Money>,
}
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Rubrics {
    pub salgsmoms: Money,
    pub moms_af_varekob_udland: Money,
    pub moms_af_ydelseskob_udland: Money,
    pub kobsmoms: Money,
    pub momstilsvar: Money,
    pub rubrik_a: Money,
    pub rubrik_b: Money,
    pub rubrik_c: Money,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub output_vat: Money,
    pub output_vat_adjustment: Money,
    pub input_vat: Money,
    pub payable: Money,
    /// Shared canonical base for tax preparation; existing VAT wire stays unchanged.
    #[serde(skip)]
    pub representation_purchase_base: Money,
    pub oss_consumer_sales_base: Money,
    pub oss_consumer_sales_entry_count: usize,
    pub rubrikker: Rubrics,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}
#[derive(Default)]
struct Base {
    amount: Money,
    count: usize,
    foreign: usize,
}
fn add_base(
    bases: &mut BTreeMap<String, Base>,
    code: &str,
    amount: Money,
    foreign: bool,
    exact: bool,
) {
    let base = bases.entry(code.into()).or_default();
    base.amount.add(&amount);
    base.count += usize::from(!exact);
    base.foreign += usize::from(foreign && !exact);
}
fn sum(bases: &BTreeMap<String, Base>, codes: &[&str]) -> Money {
    let mut total = Money::default();
    for code in codes {
        if let Some(base) = bases.get(*code) {
            total.add(&base.amount);
        }
    }
    total
}
fn tolerance(bases: &BTreeMap<String, Base>, codes: &[&str]) -> usize {
    let mut count: usize = 0;
    let mut foreign = 0;
    for code in codes {
        if let Some(base) = bases.get(*code) {
            count += base.count;
            foreign += base.foreign;
        }
    }
    count.saturating_sub(1) + foreign
}
fn mismatch(
    errors: &mut Vec<String>,
    label: &str,
    booked: &Money,
    expected: &Money,
    tolerance: usize,
) {
    let diff = booked.subtract(expected);
    if diff.0 > BigInt::from(tolerance) || diff.0 < -BigInt::from(tolerance) {
        errors.push(format!(
            "{label}: bogført {}, forventet {} ud fra momsgrundlaget",
            booked.decimal(),
            expected.decimal()
        ));
    }
}
pub fn build(lines: &[Line]) -> Result<Report, Error> {
    let mut entries: BTreeMap<i64, Vec<&Line>> = BTreeMap::new();
    for line in lines {
        entries.entry(line.entry_id).or_default().push(line);
    }
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let mut output = Money::default();
    let mut input = Money::default();
    let mut bases = BTreeMap::new();
    let mut oss_entries = BTreeSet::new();
    let mut exact_sales_base = Money::default();
    let mut exact_sales_vat = Money::default();
    let mut allocated_reverse = Money::default();
    let mut total_expected_reverse = Money::default();
    let mut dedicated_present = BTreeSet::new();
    let reverse_codes = [
        "EU_SERVICE_REVERSE_CHARGE",
        "NON_EU_SERVICE_REVERSE_CHARGE",
        "EU_GOODS_ACQUISITION",
    ];
    for (id, lines) in entries {
        let has_amount = lines.iter().any(|l| l.amount_side.is_some());
        let is_settlement = has_amount
            && lines.iter().any(|l| l.settlement)
            && lines
                .iter()
                .all(|l| l.amount_side.is_some() || l.settlement);
        if is_settlement {
            continue;
        }
        if lines.iter().any(|l| l.historical) {
            errors.push(format!("Postering {id}: historisk import kræver den endnu ikke porterede kildevalidering før momsangivelse"));
        }
        let mut classified = false;
        let mut expected_reverse = Money::default();
        for line in &lines {
            let net = line.debit.subtract(&line.credit);
            match line.amount_side {
                Some("input") => input.add(&net),
                Some("output") => {
                    let amount = line.credit.subtract(&line.debit);
                    output.add(&amount);
                    if line.dedicated_reverse_charge {
                        allocated_reverse.add(&amount);
                        dedicated_present.insert(id);
                    }
                }
                _ => {}
            }
            let Some(code) = line.vat_code.as_deref() else {
                continue;
            };
            if !VAT_CODES.contains(&code) {
                errors.push(format!(
                    "Postering {id}, konto {}: ukendt momskode {code}",
                    line.account_no
                ));
                continue;
            }
            if !["expense", "income"].contains(&line.account_type.as_str()) {
                continue;
            }
            if !["DK_PURCHASE_EXEMPT", "REPRESENTATION_NON_DEDUCTIBLE_VAT"].contains(&code) {
                classified = true;
            }
            let amount = if [
                "DK_SALE_25",
                "DK_SALE_EXEMPT",
                "REVERSE_CHARGE_EXEMPT",
                "DOMESTIC_REVERSE_CHARGE_EXEMPT",
                "OSS_EU_CONSUMER",
            ]
            .contains(&code)
            {
                line.credit.subtract(&line.debit)
            } else {
                net
            };
            if code == "OSS_EU_CONSUMER" {
                oss_entries.insert(id);
            }
            if reverse_codes.contains(&code) {
                expected_reverse.add(&amount.percent("25")?);
            }
            if let Some(expected) = &line.exact_sales_vat {
                if code != "DK_SALE_25" || line.account_type != "income" {
                    return Err(Error::new(
                        "invalid_vat_source",
                        "exact sales VAT requires a verified taxable income counter",
                    ));
                }
                exact_sales_base.add(&amount);
                exact_sales_vat.add(expected);
            }
            add_base(
                &mut bases,
                code,
                amount,
                line.foreign,
                line.exact_sales_vat.is_some(),
            );
        }
        if has_amount && !classified {
            errors.push(format!(
                "Postering {id}: momskonto uden eksplicit klassificeret momsgrundlag"
            ));
        }
        total_expected_reverse.add(&expected_reverse);
        if !dedicated_present.contains(&id) {
            allocated_reverse.add(&expected_reverse);
        }
    }
    let reverse_base = sum(&bases, &reverse_codes);
    let purchases = sum(&bases, &["DK_PURCHASE_25"]);
    let sales = sum(&bases, &["DK_SALE_25"]);
    let relief = sum(&bases, &["DK_BAD_DEBT_25"]).percent("25")?;
    let representation = sum(&bases, &["REPRESENTATION_SPECIAL"])
        .percent("25")?
        .percent("25")?;
    let mut expected_ordinary = sales.subtract(&exact_sales_base).percent("25")?;
    expected_ordinary.add(&exact_sales_vat);
    expected_ordinary = expected_ordinary.subtract(&relief);
    let mut expected_output = expected_ordinary.clone();
    expected_output.add(&reverse_base.percent("25")?);
    let mut expected_input = purchases.percent("25")?;
    expected_input.add(&reverse_base.percent("25")?);
    expected_input.add(&representation);
    let output_codes = [
        "DK_SALE_25",
        "DK_BAD_DEBT_25",
        "EU_SERVICE_REVERSE_CHARGE",
        "NON_EU_SERVICE_REVERSE_CHARGE",
        "EU_GOODS_ACQUISITION",
    ];
    let input_codes = [
        "DK_PURCHASE_25",
        "REPRESENTATION_SPECIAL",
        "EU_SERVICE_REVERSE_CHARGE",
        "NON_EU_SERVICE_REVERSE_CHARGE",
        "EU_GOODS_ACQUISITION",
    ];
    mismatch(
        &mut errors,
        "Salgsmoms stemmer ikke",
        &output,
        &expected_output,
        tolerance(&bases, &output_codes),
    );
    mismatch(
        &mut errors,
        "Købsmoms stemmer ikke",
        &input,
        &expected_input,
        tolerance(&bases, &input_codes),
    );
    let ordinary = output.subtract(&allocated_reverse);
    mismatch(
        &mut errors,
        "Almindelig salgsmoms stemmer ikke",
        &ordinary,
        &expected_ordinary,
        tolerance(&bases, &["DK_SALE_25", "DK_BAD_DEBT_25"]),
    );
    // The dedicated category cannot be hidden by an opposite ordinary-VAT error.
    let reverse_foreign: usize = reverse_codes
        .iter()
        .filter_map(|c| bases.get(*c))
        .map(|b| b.foreign)
        .sum();
    mismatch(
        &mut errors,
        "Moms af udenlandske køb stemmer ikke",
        &allocated_reverse,
        &total_expected_reverse,
        reverse_foreign,
    );
    let exempt = sum(&bases, &["DK_SALE_EXEMPT"]);
    if !exempt.is_negative()
        && exempt != Money::default()
        && !input.is_negative()
        && input != Money::default()
    {
        warnings.push("Perioden har momsfri omsætning og fradraget købsmoms. Delvis fradragsret beregnes ikke automatisk; få fradragsprocenten vurderet.".into());
    }
    let goods_base = sum(&bases, &["EU_GOODS_ACQUISITION"]);
    let goods_vat = if !goods_base.is_negative() && goods_base != Money::default() {
        goods_base.percent("25")?
    } else {
        Money::default()
    };
    let mut gross_output = output.clone();
    gross_output.add(&relief);
    let payable = output.subtract(&input);
    Ok(Report {
        output_vat: gross_output,
        output_vat_adjustment: Money::default().subtract(&relief),
        input_vat: input.clone(),
        payable: payable.clone(),
        representation_purchase_base: sum(&bases, &["REPRESENTATION_SPECIAL"]),
        oss_consumer_sales_base: sum(&bases, &["OSS_EU_CONSUMER"]),
        oss_consumer_sales_entry_count: oss_entries.len(),
        rubrikker: Rubrics {
            salgsmoms: ordinary,
            moms_af_ydelseskob_udland: allocated_reverse.subtract(&goods_vat),
            moms_af_varekob_udland: goods_vat,
            kobsmoms: input,
            momstilsvar: payable,
            rubrik_a: sum(
                &bases,
                &["EU_SERVICE_REVERSE_CHARGE", "EU_GOODS_ACQUISITION"],
            ),
            rubrik_b: sum(&bases, &["REVERSE_CHARGE_EXEMPT"]),
            rubrik_c: sum(
                &bases,
                &["DK_SALE_EXEMPT", "DOMESTIC_REVERSE_CHARGE_EXEMPT"],
            ),
        },
        errors,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn line(
        id: i64,
        kind: &str,
        code: Option<&str>,
        debit: &str,
        credit: &str,
        side: Option<&'static str>,
    ) -> Line {
        Line {
            entry_id: id,
            account_no: format!("synthetic-{kind}"),
            account_type: kind.into(),
            vat_code: code.map(str::to_owned),
            debit: Money::parse(debit).unwrap(),
            credit: Money::parse(credit).unwrap(),
            amount_side: side,
            dedicated_reverse_charge: false,
            settlement: false,
            foreign: false,
            historical: false,
            exact_sales_vat: None,
        }
    }
    #[test]
    fn tax_representation_base_reuses_classification_without_changing_vat_wire() {
        let report = build(&[
            line(
                1,
                "expense",
                Some("REPRESENTATION_SPECIAL"),
                "0.06",
                "0",
                None,
            ),
            line(1, "vat", None, "0.01", "0", Some("input")),
        ])
        .unwrap();
        assert!(report.errors.is_empty());
        assert_eq!(report.representation_purchase_base.decimal(), "0.06");
        assert!(
            serde_json::to_value(report)
                .unwrap()
                .get("representationPurchaseBase")
                .is_none()
        );
    }
    #[test]
    fn oss_counts_distinct_classified_entries_and_keeps_danish_vat_separate() {
        let lines = vec![
            line(1, "income", Some("OSS_EU_CONSUMER"), "0", "400.01", None),
            line(1, "income", Some("OSS_EU_CONSUMER"), "0", "599.99", None),
            line(1, "asset", Some("OSS_EU_CONSUMER"), "1000", "0", None),
            line(2, "income", Some("DK_SALE_25"), "0", "100", None),
            line(2, "vat", None, "0", "25", Some("output")),
        ];
        let report = build(&lines).unwrap();
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(report.oss_consumer_sales_base.decimal(), "1000.00");
        assert_eq!(report.oss_consumer_sales_entry_count, 1);
        assert_eq!(report.output_vat.decimal(), "25.00");
        assert_eq!(report.rubrikker.rubrik_b, Money::default());
        assert_eq!(report.rubrikker.rubrik_c, Money::default());
    }
    #[test]
    fn oss_reversal_is_signed_and_expense_base_lines_are_counted() {
        let original = line(1, "income", Some("OSS_EU_CONSUMER"), "0", "1000", None);
        let reversal = line(2, "income", Some("OSS_EU_CONSUMER"), "1000", "0", None);
        let report = build(&[reversal]).unwrap();
        assert_eq!(report.oss_consumer_sales_base.decimal(), "-1000.00");
        assert_eq!(report.oss_consumer_sales_entry_count, 1);
        let report = build(&[
            original,
            line(2, "income", Some("OSS_EU_CONSUMER"), "1000", "0", None),
            line(3, "expense", Some("OSS_EU_CONSUMER"), "20.01", "0", None),
        ])
        .unwrap();
        assert!(report.errors.is_empty());
        assert_eq!(report.oss_consumer_sales_base.decimal(), "-20.01");
        assert_eq!(report.oss_consumer_sales_entry_count, 3);
    }
    #[test]
    fn verified_contained_sales_vat_is_exact_without_relaxing_ordinary_sales() {
        let mut sale = line(1, "income", Some("DK_SALE_25"), "0", "80.06", None);
        sale.exact_sales_vat = Some(Money::parse("20.01").unwrap());
        let mut lines = vec![sale, line(1, "vat", None, "0", "20.01", Some("output"))];
        assert!(build(&lines).unwrap().errors.is_empty());
        lines[1].credit = Money::parse("20.02").unwrap();
        assert!(
            build(&lines)
                .unwrap()
                .errors
                .iter()
                .any(|e| e.contains("Salgsmoms stemmer ikke"))
        );
        lines[1].credit = Money::parse("20.01").unwrap();
        lines.extend([
            line(2, "income", Some("DK_SALE_25"), "0", "100", None),
            line(2, "vat", None, "0", "25.01", Some("output")),
        ]);
        assert!(
            build(&lines)
                .unwrap()
                .errors
                .iter()
                .any(|e| e.contains("Salgsmoms stemmer ikke"))
        );
        lines[3].credit = Money::parse("25").unwrap();
        assert!(build(&lines).unwrap().errors.is_empty());
        let mut refund = line(3, "income", Some("DK_SALE_25"), "80.06", "0", None);
        refund.exact_sales_vat = Some(Money::parse("-20.01").unwrap());
        lines.extend([refund, line(3, "vat", None, "20.01", "0", Some("output"))]);
        let report = build(&lines).unwrap();
        assert!(report.errors.is_empty());
        assert_eq!(report.output_vat.decimal(), "25.00");
    }
    #[test]
    fn native_purchase_reversal_and_amount_only_error_are_not_confused() {
        let mut lines = vec![
            line(1, "expense", Some("DK_PURCHASE_25"), "100", "0", None),
            line(1, "vat", None, "25", "0", Some("input")),
        ];
        let report = build(&lines).unwrap();
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(report.input_vat.decimal(), "25.00");
        assert_eq!(report.payable.decimal(), "-25.00");
        let reversal = vec![
            line(2, "expense", Some("DK_PURCHASE_25"), "0", "100", None),
            line(2, "vat", None, "0", "25", Some("input")),
        ];
        assert_eq!(build(&reversal).unwrap().payable.decimal(), "25.00");
        lines.extend(reversal);
        let report = build(&lines).unwrap();
        assert!(report.errors.is_empty());
        assert_eq!(report.input_vat, Money::default());
        lines.push(line(3, "vat", None, "10", "0", Some("input")));
        let report = build(&lines).unwrap();
        assert!(report.errors.iter().any(|e| e.contains("uden eksplicit")));
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.contains("Købsmoms stemmer ikke"))
        );
    }
    #[test]
    fn dedicated_reverse_charge_cannot_cancel_an_ordinary_vat_error() {
        let mut reverse = line(1, "vat", None, "0", "24", Some("output"));
        reverse.dedicated_reverse_charge = true;
        let lines = vec![
            line(
                1,
                "expense",
                Some("EU_SERVICE_REVERSE_CHARGE"),
                "100",
                "0",
                None,
            ),
            line(1, "vat", None, "25", "0", Some("input")),
            reverse,
            line(2, "income", Some("DK_SALE_25"), "0", "100", None),
            line(2, "vat", None, "0", "26", Some("output")),
        ];
        let report = build(&lines).unwrap();
        assert_eq!(report.output_vat.decimal(), "50.00");
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.contains("Almindelig salgsmoms"))
        );
        assert!(report.errors.iter().any(|e| e.contains("udenlandske køb")));
    }
    #[test]
    fn settlement_transfer_is_not_new_vat_and_defaults_are_not_classification() {
        let mut settlement = line(1, "liability", None, "0", "25", None);
        settlement.settlement = true;
        let lines = vec![line(1, "vat", None, "25", "0", Some("output")), settlement];
        let report = build(&lines).unwrap();
        assert_eq!(report.payable, Money::default());
        assert!(report.errors.is_empty());
    }
}
