//! Deterministic local class-B subset; prepared review document, not a Virk filing.
use crate::{
    Error,
    money::Money,
    tax_return::{amount, invalid, text},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
const NAME: &str = "Rentemester regnskabsklasse-B iXBRL subset (bounded)";
const VERSION: &str = "0.2.0";
const NAMESPACE: &str = "urn:rentemester:dk:arsrapport:v1";
const ELEMENTS: [(&str, &str, &str, &str, &str); 20] = [
    (
        "ar:CompanyName",
        "text",
        "Virksomhedsnavn",
        "company-info",
        "duration",
    ),
    (
        "ar:RegistreredCvr",
        "text",
        "CVR-nummer",
        "company-info",
        "duration",
    ),
    (
        "ar:ReportingPeriodStartDate",
        "text",
        "Regnskabsaar start",
        "company-info",
        "duration",
    ),
    (
        "ar:ReportingPeriodEndDate",
        "text",
        "Regnskabsaar slut",
        "company-info",
        "duration",
    ),
    (
        "ar:ReportingClass",
        "text",
        "Regnskabsklasse",
        "company-info",
        "duration",
    ),
    (
        "ar:Revenue",
        "monetary",
        "Nettoomsaetning",
        "income-statement",
        "duration",
    ),
    (
        "ar:OtherOperatingExpenses",
        "monetary",
        "Andre eksterne omkostninger",
        "income-statement",
        "duration",
    ),
    (
        "ar:GrossResult",
        "monetary",
        "Bruttoresultat",
        "income-statement",
        "duration",
    ),
    (
        "ar:ProfitLossFromOrdinaryOperatingActivities",
        "monetary",
        "Resultat af ordinaer drift",
        "income-statement",
        "duration",
    ),
    (
        "ar:ProfitLossForYear",
        "monetary",
        "Aarets resultat",
        "income-statement",
        "duration",
    ),
    (
        "ar:NoncurrentAssets",
        "monetary",
        "Anlaegsaktiver",
        "balance-sheet",
        "instant",
    ),
    (
        "ar:CurrentAssets",
        "monetary",
        "Omsaetningsaktiver",
        "balance-sheet",
        "instant",
    ),
    (
        "ar:Assets",
        "monetary",
        "Aktiver i alt",
        "balance-sheet",
        "instant",
    ),
    (
        "ar:ContributedCapital",
        "monetary",
        "Selskabskapital og reserver",
        "balance-sheet",
        "instant",
    ),
    (
        "ar:RetainedEarnings",
        "monetary",
        "Overfoert resultat",
        "balance-sheet",
        "instant",
    ),
    (
        "ar:Equity",
        "monetary",
        "Egenkapital i alt",
        "balance-sheet",
        "instant",
    ),
    (
        "ar:Liabilities",
        "monetary",
        "Gaeldsforpligtelser i alt",
        "balance-sheet",
        "instant",
    ),
    (
        "ar:LiabilitiesAndEquity",
        "monetary",
        "Passiver i alt",
        "balance-sheet",
        "instant",
    ),
    (
        "ar:ManagementStatement",
        "text",
        "Ledelsespaategning",
        "management-statement",
        "duration",
    ),
    (
        "ar:AccountingPolicies",
        "text",
        "Anvendt regnskabspraksis",
        "accounting-policies",
        "duration",
    ),
];
pub fn taxonomy() -> Value {
    json!({"name":NAME,"version":VERSION,"scope":"regnskabsklasse B (micro/small) — bounded subset","prefix":"ar","namespace":NAMESPACE,"elements":ELEMENTS.iter().map(|(name,kind,label,section,context)|json!({"name":name,"kind":kind,"label":label,"section":section,"context":context})).collect::<Vec<_>>()})
}
fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn money(v: &Value) -> Result<String, Error> {
    Ok(amount(v)?.decimal())
}
/// Generate only from the shared, successful annual report. All dynamic XML is escaped.
pub fn generate(report: &Value) -> Result<Value, Error> {
    let tax = json!({"name":NAME,"version":VERSION});
    let failure = |errors: Vec<Value>| json!({"ok":false,"appliedRules":["DK-ANNUAL-REPORT-IXBRL-002"],"xhtml":"","sha256":"","facts":[],"taxonomy":tax,"errors":errors});
    if report["ok"] != true {
        let mut errors = vec![json!(
            "cannot generate iXBRL: the annual report did not pass its prerequisites"
        )];
        errors.extend(
            report["errors"]
                .as_array()
                .ok_or_else(invalid)?
                .iter()
                .cloned(),
        );
        return Ok(failure(errors));
    }
    let start = text(report, "fiscalYearStart")?;
    let end = text(report, "fiscalYearEnd")?;
    crate::fiscal_year::Date::parse(start)?;
    crate::fiscal_year::Date::parse(end)?;
    let cvr = text(&report["company"], "cvr")?;
    let digits = cvr.strip_prefix("DK").unwrap_or(cvr);
    if digits.len() != 8 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Ok(failure(vec![json!(
            "cannot generate iXBRL: a registered 8-digit CVR is required for the xbrli context"
        )]));
    }
    let pl = &report["profitAndLoss"];
    let bs = &report["balanceSheet"];
    let gross = amount(&pl["totalIncome"])?.subtract(&amount(&pl["totalExpense"])?);
    let equity = amount(&bs["equity"]["total"])?;
    let carried = equity.subtract(&amount(&bs["periodResult"])?);
    let notes = report["notes"].as_array().ok_or_else(invalid)?;
    let note = |id: &str, key: &str, fallback: &str| -> Result<String, Error> {
        match notes.iter().find(|n| n["id"] == id) {
            Some(n) => Ok(text(n, key)?.to_owned()),
            None => Ok(fallback.into()),
        }
    };
    // The native section already includes all unclosed results. Do not add them again.
    // The inherited bounded subset has no non-current classifier: all assets stay current.
    let values = [
        text(&report["company"], "name")?.into(),
        cvr.into(),
        start.into(),
        end.into(),
        text(report, "regnskabsklasse")?.into(),
        money(&pl["totalIncome"])?,
        money(&pl["totalExpense"])?,
        gross.decimal(),
        gross.decimal(),
        money(&report["aretsResultat"])?,
        Money::default().decimal(),
        money(&bs["totalAssets"])?,
        money(&bs["totalAssets"])?,
        carried.decimal(),
        money(&bs["periodResult"])?,
        equity.decimal(),
        money(&bs["liabilities"]["total"])?,
        money(&bs["totalLiabilitiesAndEquity"])?,
        text(&report["ledelsespategning"], "text")?.into(),
        note(
            "accounting-policies",
            "body",
            "Anvendt regnskabspraksis udfyldes af ejer eller revisor.",
        )?,
    ];
    let declared = taxonomy();
    let facts: Vec<Value> = values
        .iter()
        .enumerate()
        .map(|(i, value)| json!({"element":declared["elements"][i],"value":value}))
        .collect();
    let mut blocks = vec![];
    for (section, heading) in [
        ("company-info", "Selskabsoplysninger"),
        ("income-statement", "Resultatopgoerelse"),
        ("balance-sheet", "Balance"),
        ("management-statement", "Ledelsespaategning"),
        ("accounting-policies", "Anvendt regnskabspraksis"),
    ] {
        let mut rows = vec![];
        for (i, (name, kind, label, group, context)) in ELEMENTS.iter().enumerate() {
            if *group != section {
                continue;
            }
            let value = escape(&values[i]);
            let fact = if *kind == "monetary" {
                format!(
                    "<ix:nonFraction name=\"{name}\" contextRef=\"{context}\" unitRef=\"DKK\" decimals=\"2\">{value}</ix:nonFraction>"
                )
            } else {
                format!(
                    "<ix:nonNumeric name=\"{name}\" contextRef=\"{context}\">{value}</ix:nonNumeric>"
                )
            };
            rows.push(format!(
                "    <tr><th>{}</th><td>{fact}</td></tr>",
                escape(label)
            ));
        }
        blocks.push(format!(
            "    <h2>{}</h2>\n    <table>\n{}\n    </table>",
            escape(heading),
            rows.join("\n")
        ));
    }
    let cmp = &report["comparison"];
    let available = cmp["available"].as_bool().ok_or_else(invalid)?;
    let mut comparison = format!(
        "    <h2>Sammenligningstal (foregaaende regnskabsaar {} — {})</h2>\n    <table>",
        escape(text(cmp, "fiscalYearStart")?),
        escape(text(cmp, "fiscalYearEnd")?)
    );
    for (label, key) in [
        ("Nettoomsaetning", "totalIncome"),
        ("Omkostninger", "totalExpense"),
        ("Aarets resultat", "aretsResultat"),
        ("Aktiver i alt", "totalAssets"),
        ("Egenkapital i alt", "equity"),
        ("Passiver i alt", "totalLiabilitiesAndEquity"),
    ] {
        let cell = if available {
            money(&cmp[key])?
        } else {
            "—".into()
        };
        comparison.push_str(&format!("\n    <tr><th>{label}</th><td>{cell}</td></tr>"));
    }
    comparison.push_str("\n    </table>");
    let employees = format!(
        "    <h2>Gennemsnitligt antal beskaeftigede</h2>\n    <table>\n    <tr><th>{}</th><td>{}</td></tr>\n    </table>",
        escape(&note(
            "average-employees",
            "title",
            "Gennemsnitligt antal beskaeftigede"
        )?),
        escape(&note(
            "average-employees",
            "body",
            "Udfyldes af ejer eller revisor."
        )?)
    );
    let note_rows = notes
        .iter()
        .map(|n| {
            Ok(format!(
                "    <tr><th>{}</th><td>{}</td></tr>",
                escape(text(n, "title")?),
                escape(text(n, "body")?)
            ))
        })
        .collect::<Result<Vec<_>, Error>>()?
        .join("\n");
    let xhtml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"
      xmlns:ix="http://www.xbrl.org/2013/inlineXBRL"
      xmlns:xbrli="http://www.xbrl.org/2003/instance"
      xmlns:iso4217="http://www.xbrl.org/2003/iso4217"
      xmlns:ar="{NAMESPACE}">
  <head>
    <title>Arsrapport (regnskabsklasse B) — forberedt af Rentemester</title>
    <meta name="rentemester-ixbrl-taxonomy" content="{NAME} v{VERSION}"/>
  </head>
  <body>
    <div style="display:none">
      <ix:header>
        <ix:references/>
        <ix:resources>
          <xbrli:context id="duration">
            <xbrli:entity>
              <xbrli:identifier scheme="http://www.cvr.dk">{digits}</xbrli:identifier>
            </xbrli:entity>
            <xbrli:period>
              <xbrli:startDate>{start}</xbrli:startDate>
              <xbrli:endDate>{end}</xbrli:endDate>
            </xbrli:period>
          </xbrli:context>
          <xbrli:context id="instant">
            <xbrli:entity>
              <xbrli:identifier scheme="http://www.cvr.dk">{digits}</xbrli:identifier>
            </xbrli:entity>
            <xbrli:period>
              <xbrli:instant>{end}</xbrli:instant>
            </xbrli:period>
          </xbrli:context>
          <xbrli:unit id="DKK">
            <xbrli:measure>iso4217:DKK</xbrli:measure>
          </xbrli:unit>
        </ix:resources>
      </ix:header>
    </div>
    <h1>Arsrapport {start} — {end}</h1>
    <p>Regnskabsklasse B (micro/small). Forberedt af Rentemester; ejer eller revisor gennemgar og indberetter.</p>
    <p>iXBRL-taksonomi: {NAME} v{VERSION} — et afgraenset udsnit under det rentemester-lokale `ar:`-namespace, IKKE Erhvervsstyrelsens officielle taksonomi. Dokumentet kan derfor IKKE indberettes digitalt til Erhvervsstyrelsen/Virk endnu; det er en forberedt arsrapport til gennemsyn.</p>
{sections}
{comparison}
{employees}
    <h2>Noter (skelet)</h2>
    <table>
{note_rows}
    </table>
  </body>
</html>
"#,
        sections = blocks.join("\n")
    );
    Ok(
        json!({"ok":true,"appliedRules":["DK-ANNUAL-REPORT-IXBRL-002"],"sha256":format!("{:x}",Sha256::digest(xhtml.as_bytes())),"xhtml":xhtml,"facts":facts,"taxonomy":tax,"errors":[]}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    fn report() -> Value {
        let mut r = json!({"ok":true,"errors":[],"fiscalYearStart":"2025-01-01","fiscalYearEnd":"2025-12-31","regnskabsklasse":"B","company":{"name":"Synthetic <company> & \"owner\"","cvr":"DK87654321"},"profitAndLoss":{"totalIncome":100,"totalExpense":25},"aretsResultat":75,"balanceSheet":{"equity":{"total":125},"periodResult":75,"totalAssets":150,"liabilities":{"total":25},"totalLiabilitiesAndEquity":150},"comparison":{"available":false,"fiscalYearStart":"2024-01-01","fiscalYearEnd":"2024-12-31"}});
        let placeholders = crate::annual_report::placeholders("2025-12-31");
        r["notes"] = placeholders["notes"].clone();
        r["ledelsespategning"] = placeholders["ledelsespategning"].clone();
        r
    }
    #[test]
    fn ordered_twenty_facts_context_units_escaping_hash_and_native_equity() {
        let r = report();
        let a = generate(&r).unwrap();
        assert_eq!(a, generate(&r).unwrap());
        let xml = a["xhtml"].as_str().unwrap();
        assert_eq!(a["sha256"], format!("{:x}", Sha256::digest(xml.as_bytes())));
        assert_eq!(a["facts"].as_array().unwrap().len(), 20);
        assert_eq!(a["facts"][1]["element"]["name"], "ar:RegistreredCvr");
        assert_eq!(a["facts"][13]["value"], "50.00");
        assert_eq!(a["facts"][15]["value"], "125.00");
        assert_eq!(a["facts"][10]["value"], "0.00");
        assert!(xml.contains("Synthetic &lt;company&gt; &amp; &quot;owner&quot;"));
        assert!(xml.contains("xmlns:iso4217=\"http://www.xbrl.org/2003/iso4217\""));
        assert_eq!(xml.matches("<xbrli:context id=").count(), 2);
        assert_eq!(xml.matches("</xbrli:context>").count(), 2);
        assert_eq!(xml.matches("contextRef=\"instant\"").count(), 8);
        assert_eq!(xml.matches("contextRef=\"duration\"").count(), 12);
        assert!(xml.contains("<xbrli:instant>2025-12-31</xbrli:instant>"));
        assert!(xml.contains("decimals=\"2\">100.00</ix:nonFraction>"));
    }
    #[test]
    fn failed_annual_or_missing_cvr_never_exports() {
        let mut r = report();
        r["ok"] = json!(false);
        r["errors"] = json!(["year reopened"]);
        let v = generate(&r).unwrap();
        assert_eq!(v["ok"], false);
        assert_eq!(v["xhtml"], "");
        assert_eq!(v["facts"], json!([]));
        r["ok"] = json!(true);
        r["company"]["cvr"] = json!("invalid");
        assert_eq!(generate(&r).unwrap()["ok"], false);
    }
}
