//! Deterministic preparation only: the owner/advisor reviews and files the return.
use crate::{Error, money::Money};
use serde_json::{Value, json};
const RULE: &str = "DK-TAX-RETURN-CORP-001";
const DISCLAIMER: &str = "Rentemester forbereder tallene til oplysningsskemaet (selskabets skattepligtige indkomst) ud fra det lukkede regnskabsår og de skattemæssige reguleringer systemet kan se deterministisk. Ejer eller revisor gennemgår tallene, afklarer needs-review-punkterne og indberetter selv via TastSelv Erhverv. Dette er ikke en fuldstændig skatteberegning.";
pub(crate) fn invalid() -> Error {
    Error::new(
        "annual_export_source_unavailable",
        "Annual export source is incomplete or exceeds exact numeric precision",
    )
}
pub(crate) fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str, Error> {
    v[key].as_str().ok_or_else(invalid)
}
pub(crate) fn amount(v: &Value) -> Result<Money, Error> {
    if let Some(s) = v.as_str() {
        return Money::parse_canonical(s);
    }
    if !v.is_number() {
        return Err(invalid());
    }
    let money = Money::parse(&v.to_string())?;
    if money.decimal().parse::<f64>().ok() != v.as_f64() {
        return Err(invalid());
    }
    Ok(money)
}
pub(crate) fn number(m: &Money) -> Result<Value, Error> {
    let raw = m.decimal();
    let f: f64 = raw.parse().map_err(|_| invalid())?;
    if !f.is_finite() || Money::parse(&f.to_string())? != *m {
        return Err(invalid());
    }
    let s = raw.trim_end_matches('0').trim_end_matches('.');
    Ok(Value::Number(s.parse().map_err(|_| invalid())?))
}
/// Consume an already gated annual report and exact shared VAT/asset projections.
pub fn prepare(
    annual: &Value,
    representation_base: &Money,
    book_depreciation: &Money,
) -> Result<Value, Error> {
    let start = text(annual, "fiscalYearStart")?;
    let end = text(annual, "fiscalYearEnd")?;
    crate::fiscal_year::Date::parse(start)?;
    crate::fiscal_year::Date::parse(end)?;
    let form = annual["company"]["companyForm"].as_str().unwrap_or("");
    let errors = annual["errors"].as_array().ok_or_else(invalid)?;
    let ok = annual["ok"].as_bool().ok_or_else(invalid)?;
    let mut result = json!({"ok":ok,"appliedRules":[RULE],"fiscalYearStart":start,"fiscalYearEnd":end,"companyForm":form,"bookkeptResult":0,"adjustments":[],"totalAdjustments":0,"taxableIncome":0,"corporateTaxRate":0.22,"corporateTax":null,"needsReview":[],"acontoTaxDeadlines":[{"installment":1,"dueDate":format!("{}-03-20",&end[..4])},{"installment":2,"dueDate":format!("{}-11-20",&end[..4])}],"preparedBy":"Rentemester","disclaimer":DISCLAIMER,"errors":errors});
    if !ok {
        return Ok(result);
    }
    if !errors.is_empty() {
        return Err(invalid());
    }
    let book = amount(&annual["aretsResultat"])?;
    let mut addback = Money::default();
    if !representation_base.is_negative() && *representation_base != Money::default() {
        let vat = representation_base.percent("25")?;
        let nondeductible = vat.subtract(&vat.percent("25")?);
        let mut booked = representation_base.clone();
        booked.add(&nondeductible);
        addback = booked.percent("75")?;
        result["adjustments"] = json!([{"kind":"non_deductible_representation","label":"Ikke-fradragsberettiget andel af repraesentation (75%, ligningsloven § 8, stk. 4)","amount":number(&addback)?,"explanation":"Repraesentationsudgifter er kun 25% skattemaessigt fradragsberettigede (ligningsloven § 8, stk. 4). 75% af den bogfoerte repraesentationsomkostning (netto-beloeb plus ikke-fradragsberettiget moms) laegges derfor til den skattepligtige indkomst."}]);
    }
    let mut taxable = book.clone();
    taxable.add(&addback);
    let mut reviews = vec![];
    if !book_depreciation.is_negative() && *book_depreciation != Money::default() {
        reviews.push(json!({"kind":"depreciation_difference","label":"Forskel mellem regnskabsmæssige og skattemæssige afskrivninger","requiredAction":"Opgør de skattemæssige afskrivninger (saldoafskrivning) og reguler for forskellen til de regnskabsmæssige (lineære) afskrivninger.","bookDepreciation":number(book_depreciation)?}));
    }
    if taxable.is_negative() {
        reviews.push(json!({"kind":"tax_loss_carry_forward","label":"Fremfoert skattemaessigt underskud","requiredAction":"Aarets skattepligtige indkomst er negativ. Underskuddet fremfoeres til modregning i senere aars positive indkomst — afklar fremfoerslen i oplysningsskemaet."}));
    }
    let aps = matches!(
        form.trim().to_lowercase().as_str(),
        "aps" | "anpartsselskab"
    );
    if !aps {
        reviews.push(json!({"kind":"company_form_out_of_scope","label":"Selskabsform uden for dette slice","requiredAction":format!("Dette slice beregner kun selskabsskat (22%) for et anpartsselskab (ApS). Den registrerede selskabsform er \"{}\" — afklar den korrekte skatteberegning med revisor.",if form.is_empty(){"ukendt"}else{form})}));
    }
    result["bookkeptResult"] = number(&book)?;
    result["totalAdjustments"] = number(&addback)?;
    result["taxableIncome"] = number(&taxable)?;
    result["needsReview"] = json!(reviews);
    if aps {
        result["corporateTax"] = number(&if taxable.is_negative() {
            Money::default()
        } else {
            taxable.percent("22")?
        })?;
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn annual(form: &str, result: i64) -> Value {
        json!({"ok":true,"fiscalYearStart":"2025-01-01","fiscalYearEnd":"2025-12-31","company":{"companyForm":form},"aretsResultat":result,"errors":[]})
    }
    #[test]
    fn exact_intermediate_representation_rounding() {
        let r = prepare(
            &annual(" aPs ", 100),
            &Money::parse("0.06").unwrap(),
            &Money::default(),
        )
        .unwrap();
        assert_eq!(r["totalAdjustments"], 0.05);
        assert_eq!(r["taxableIncome"], 100.05);
        assert_eq!(r["corporateTax"], 22.01);
        assert!(r["needsReview"].as_array().unwrap().is_empty());
    }
    #[test]
    fn loss_unknown_form_accumulated_review_and_precision_refusal() {
        let r = prepare(
            &annual("Unknown", -10),
            &Money::default(),
            &Money::parse("123.45").unwrap(),
        )
        .unwrap();
        assert!(r["corporateTax"].is_null());
        assert_eq!(r["needsReview"].as_array().unwrap().len(), 3);
        assert_eq!(r["needsReview"][0]["bookDepreciation"], 123.45);
        assert_eq!(
            prepare(&annual("ApS", -10), &Money::default(), &Money::default()).unwrap()["corporateTax"],
            0
        );
        assert!(number(&Money::parse("9999999999999999999999.99").unwrap()).is_err());
    }
}
