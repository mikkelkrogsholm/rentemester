//! Exact roll-up of already admitted company cards; never discovers companies or consolidates ledgers.
use crate::{Error, fiscal_year::Date, money::Money, sales_invoice::wire};
use serde_json::{Value, json};
use std::collections::BTreeSet;
fn refused(message: &str) -> Error {
    Error::new("portfolio_refused", message)
}
fn sum(cards: &[Value], field: &str) -> Result<Value, Error> {
    let mut total = Money::default();
    let mut complete = true;
    for card in cards {
        match &card[field] {
            Value::Null => complete = false,
            Value::Number(n) => total.add(&Money::parse(&n.to_string())?),
            _ => {
                return Err(refused(
                    "portfolio monetary source must be numeric or unavailable",
                ));
            }
        }
    }
    if complete {
        wire(&total)
    } else {
        Ok(Value::Null)
    }
}
fn count(cards: &[Value], field: &str) -> Result<Value, Error> {
    let mut total = 0u64;
    for card in cards {
        if card[field].is_null() {
            return Ok(Value::Null);
        }
        let value = card[field]
            .as_u64()
            .ok_or_else(|| refused("invalid portfolio source count"))?;
        total = total
            .checked_add(value)
            .filter(|v| *v <= 9_007_199_254_740_991)
            .ok_or_else(|| refused("portfolio count exceeds safe wire range"))?;
    }
    Ok(json!(total))
}
pub fn render(workspace: &str, as_of: &str, companies: &[Value]) -> Result<Value, Error> {
    Date::parse(as_of)?;
    if workspace.trim().is_empty()
        || workspace.len() > 1024
        || workspace.chars().any(char::is_control)
    {
        return Err(refused("invalid workspace identity"));
    }
    let mut seen = BTreeSet::new();
    for card in companies {
        let slug = card["slug"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| refused("company identity required"))?;
        if !seen.insert(slug) || card["currency"] != "DKK" || card["asOf"] != as_of {
            return Err(refused(
                "duplicate, differently valued or differently dated company source",
            ));
        }
    }
    let known_bank = companies
        .iter()
        .filter(|c| !c["actualBankBalance"].is_null())
        .count();
    let mut totals = json!({});
    for field in [
        "openInvoiceCount",
        "overdueInvoiceCount",
        "unlinkedBankCount",
        "openExceptionCount",
    ] {
        totals[field] = count(companies, field)?;
    }
    totals["openInvoiceTotal"] = sum(companies, "openInvoiceTotal")?;
    totals["netVatPayable"] = sum(companies, "netVatPayable")?;
    let rollup = json!({"resultat":sum(companies,"resultat")?,"omsaetning":sum(companies,"omsaetning")?,"liquidity":sum(companies,"actualBankBalance")?,"vatPayable":sum(companies,"netVatPayable")?,"openTaskCount":count(companies,"openTaskCount")?,"bankBalanceComplete":known_bank==companies.len(),"knownBankCompanyCount":known_bank,"unknownBankCompanyCount":companies.len()-known_bank});
    let mut limitations = vec![
        "Porteføljen summerer de synlige virksomheders egne DKK-tal; mellemregninger og koncernelimineringer er ikke foretaget.",
        "Opgaver viser nuværende åben status for kilder frem til skæringsdatoen, ikke en rekonstrueret historisk opgavestatus.",
    ];
    if companies.iter().any(|card| {
        card["availability"]["invoices"]["reason"]
            == "native_invoices_only_imported_receivables_separate"
    }) {
        limitations.push("Fakturatal og fakturabeløb omfatter kun fakturaer udstedt i Rentemester; importerede tilgodehavender følges særskilt og indgår ikke i disse summer.");
    }
    Ok(
        json!({"ok":true,"portfolio":{"workspace":workspace,"asOf":as_of,"companyCount":companies.len(),"currency":"DKK","scope":"portfolio","companyScope":"active_accessible_companies","consolidated":false,"companies":companies,"rollup":rollup,"totals":totals,"limitations":limitations}}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    fn card(slug: &str, amount: Value, bank: Value) -> Value {
        json!({"slug":slug,"asOf":"2026-10-20","currency":"DKK","resultat":amount,"omsaetning":amount,"actualBankBalance":bank,"netVatPayable":0,"openTaskCount":0,"openInvoiceCount":0,"overdueInvoiceCount":0,"unlinkedBankCount":0,"openExceptionCount":0,"openInvoiceTotal":0})
    }
    #[test]
    fn exact_sum_and_unknown_bank_are_not_substituted_with_zero() {
        let response = render(
            "synthetic",
            "2026-10-20",
            &[
                card("a", json!(0.1), json!(0)),
                card("b", json!(0.2), Value::Null),
            ],
        )
        .unwrap();
        assert_eq!(response["portfolio"]["rollup"]["resultat"], 0.3);
        assert!(response["portfolio"]["rollup"]["liquidity"].is_null());
        assert_eq!(response["portfolio"]["rollup"]["knownBankCompanyCount"], 1);
        assert_eq!(
            response["portfolio"]["rollup"]["unknownBankCompanyCount"],
            1
        );
    }
    #[test]
    fn zero_bank_is_known_and_unavailable_financials_stay_unavailable() {
        let response = render(
            "synthetic",
            "2026-10-20",
            &[card("a", Value::Null, json!(0))],
        )
        .unwrap();
        assert!(response["portfolio"]["rollup"]["resultat"].is_null());
        assert_eq!(response["portfolio"]["rollup"]["liquidity"], 0.0);
        assert_eq!(response["portfolio"]["rollup"]["bankBalanceComplete"], true);
    }
    #[test]
    fn imported_company_native_invoice_subset_rolls_up_with_explicit_boundary() {
        let mut imported = card("imported", json!(10), json!(0));
        imported["openInvoiceCount"] = json!(1);
        imported["overdueInvoiceCount"] = json!(1);
        imported["openInvoiceTotal"] = json!(125);
        imported["availability"] = json!({"invoices":{"status":"partial","reason":"native_invoices_only_imported_receivables_separate"}});
        let mut native = card("native", json!(20), json!(0));
        native["openInvoiceCount"] = json!(2);
        native["openInvoiceTotal"] = json!(250);
        let response = render("synthetic", "2026-10-20", &[imported, native]).unwrap();
        let portfolio = &response["portfolio"];
        assert_eq!(portfolio["totals"]["openInvoiceCount"], 3);
        assert_eq!(portfolio["totals"]["overdueInvoiceCount"], 1);
        assert_eq!(portfolio["totals"]["openInvoiceTotal"], 375.0);
        assert_eq!(portfolio["companies"][0]["openInvoiceTotal"], 125);
        let limitations = portfolio["limitations"].as_array().unwrap();
        assert_eq!(limitations.len(), 3);
        assert!(
            limitations[2]
                .as_str()
                .unwrap()
                .contains("importerede tilgodehavender følges særskilt")
        );
        let native_only = render(
            "synthetic",
            "2026-10-20",
            &[card("native", json!(0), json!(0))],
        )
        .unwrap();
        assert_eq!(
            native_only["portfolio"]["limitations"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }
    #[test]
    fn different_scope_sources_refuse() {
        let a = card("a", json!(1), json!(1));
        assert!(render("synthetic", "2026-02-30", std::slice::from_ref(&a)).is_err());
        assert!(render("synthetic", "2026-10-20", &[a.clone(), a.clone()]).is_err());
        let mut b = a.clone();
        b["currency"] = json!("EUR");
        assert!(render("synthetic", "2026-10-20", &[b]).is_err());
        let mut b = a;
        b["asOf"] = json!("2026-10-21");
        assert!(render("synthetic", "2026-10-20", &[b]).is_err());
    }
}
