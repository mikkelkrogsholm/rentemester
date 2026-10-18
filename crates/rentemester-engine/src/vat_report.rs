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
fn add_base(bases: &mut BTreeMap<String, Base>, code: &str, amount: Money, foreign: bool) {
    let base = bases.entry(code.into()).or_default();
    base.amount.add(&amount);
    base.count += 1;
    base.foreign += usize::from(foreign);
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
            if reverse_codes.contains(&code) {
                expected_reverse.add(&amount.percent("25")?);
            }
            add_base(&mut bases, code, amount, line.foreign);
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
    let mut expected_output = sales.percent("25")?;
    expected_output.add(&reverse_base.percent("25")?);
    expected_output = expected_output.subtract(&relief);
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
        &sales.percent("25")?.subtract(&relief),
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
        }
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
