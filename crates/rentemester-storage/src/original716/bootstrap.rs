//! Fresh-only source onboarding, on the already owned connection; never opens paths.
use rentemester_engine::Error;
use rusqlite::{Connection, params};
use serde_json::{Value, json};
/// Source src/core/ledger.ts seedAccounts rows. No company-specific chart.
const CHART: &str = r#"[["1000", "Omsætning, ydelser", "income", "credit", null], ["1010", "Gebyr- og kompensationsindtægter", "income", "credit", null], ["1020", "Valutakursgevinst (realiseret)", "income", "credit", null], ["1100", "Debitorer", "asset", "debit", null], ["1200", "Salgsmoms", "vat", "credit", null], ["1300", "Forudbetalte omkostninger", "asset", "debit", null], ["2000", "Bank", "asset", "debit", null], ["3000", "Software og SaaS", "expense", "debit", "DK_PURCHASE_25"], ["3010", "AI-værktøjer", "expense", "debit", "EU_SERVICE_REVERSE_CHARGE"], ["3020", "Hosting og cloud", "expense", "debit", "EU_SERVICE_REVERSE_CHARGE"], ["3050", "Rejse og transport", "expense", "debit", "DK_PURCHASE_25"], ["3055", "Kørselsgodtgørelse", "expense", "debit", null], ["3070", "Repræsentation", "expense", "debit", "REPRESENTATION_SPECIAL"], ["3080", "Tab på debitorer", "expense", "debit", "DK_BAD_DEBT_25"], ["3100", "Husleje", "expense", "debit", "DK_PURCHASE_25"], ["3110", "El, vand og varme", "expense", "debit", "DK_PURCHASE_25"], ["3120", "Hardware og udstyr", "expense", "debit", "DK_PURCHASE_25"], ["3130", "Kontorartikler og småanskaffelser", "expense", "debit", "DK_PURCHASE_25"], ["3140", "Telefon og internet", "expense", "debit", "DK_PURCHASE_25"], ["3150", "Forsikringer", "expense", "debit", null], ["3160", "Revisor og bogføring", "expense", "debit", "DK_PURCHASE_25"], ["3170", "Advokat og rådgivning", "expense", "debit", "DK_PURCHASE_25"], ["3180", "Markedsføring og annoncering", "expense", "debit", "DK_PURCHASE_25"], ["3190", "Kontingenter og abonnementer", "expense", "debit", "DK_PURCHASE_25"], ["3200", "Porto og fragt", "expense", "debit", "DK_PURCHASE_25"], ["3300", "Gebyrer, bank og betalingskort", "expense", "debit", null], ["3310", "Renteudgifter", "expense", "debit", null], ["3320", "Valutakurstab (realiseret)", "expense", "debit", null], ["3500", "Lønninger", "expense", "debit", null], ["3510", "Pension", "expense", "debit", null], ["3520", "ATP og lovpligtige bidrag", "expense", "debit", null], ["3530", "Personaleomkostninger", "expense", "debit", "DK_PURCHASE_25"], ["4000", "Købsmoms", "vat", "debit", null], ["4500", "Momsafregning", "liability", "credit", null], ["5000", "Egenkapital", "equity", "credit", null], ["5010", "Privat hævning", "equity", "debit", null], ["5020", "Privat indskud", "equity", "credit", null], ["5800", "Driftsmidler og inventar", "asset", "debit", null], ["5810", "Akkumulerede afskrivninger", "asset", "credit", null], ["5820", "Afskrivninger", "expense", "debit", null], ["7000", "Leverandørgæld (kreditorer)", "liability", "credit", null], ["7100", "Skyldig A-skat", "liability", "credit", null], ["7110", "Skyldigt AM-bidrag", "liability", "credit", null], ["7120", "Skyldig ATP", "liability", "credit", null], ["7130", "Skyldig løn", "liability", "credit", null], ["7200", "Skyldig skat (skattekonto)", "liability", "credit", null], ["7300", "Skyldige omkostninger", "liability", "credit", null], ["7310", "Forudbetalt indtægt (udskudt omsætning)", "liability", "credit", null]]"#;
pub fn initialize(
    company: &mut Connection,
    settings: &crate::company_creation::Settings,
    actor: &str,
) -> Result<Value, Error> {
    let refuse = || {
        Error::new(
            "original716_init_refused",
            "fresh company setup refused; retain partial namespace, do not retry initialization",
        )
    };
    let tx = company
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|_| refuse())?;
    crate::validate(&tx)?;
    for table in ["accounts", "journal_entries", "audit_log"] {
        if tx
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| {
                r.get::<_, i64>(0)
            })
            .map_err(|_| refuse())?
            != 0
        {
            return Err(refuse());
        }
    }
    let rows: Vec<(String, String, String, String, Option<String>)> =
        serde_json::from_str(CHART).map_err(|_| refuse())?;
    for (no, name, kind, balance, vat) in &rows {
        crate::company_setup::create_account_typed(
            &tx,
            &crate::company_setup::Account {
                account_no: no.clone(),
                name: name.clone(),
                r#type: kind.clone(),
                normal_balance: balance.clone(),
                default_vat_code: vat.clone(),
            },
            true,
            actor,
        )?;
    }
    for (role, no) in [
        ("bank", "2000"),
        ("debtors", "1100"),
        ("creditors", "7000"),
        ("output_vat", "1200"),
        ("input_vat", "4000"),
        ("reverse_charge_vat", "1200"),
        ("vat_settlement", "4500"),
        ("operational_default", "3000"),
    ] {
        crate::company_setup::confirm_source_role(
            &tx,
            &crate::company_setup::Role {
                role: role.into(),
                account_no: no.into(),
                confirm: true,
            },
            actor,
        )?;
    }
    tx.execute(
        "UPDATE companies SET fiscal_year_start_month=?1,fiscal_year_label_strategy=?2 WHERE id=1",
        params![settings.month, settings.strategy],
    )
    .map_err(|_| refuse())?;
    let profile = crate::company_setup::update_profile(&tx, &settings.profile, actor)?;
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES('init','company','1','Initialized fresh source company',?1)",[actor]).map_err(|_| refuse())?;
    tx.commit().map_err(|_| {
        Error::new(
            "commit_outcome_unknown",
            "initialization commit outcome unknown; inspect namespace before any further action",
        )
    })?;
    Ok(json!({"profile":profile,"accountCount":rows.len()}))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original716_fresh_profile_chart_and_no_overwrite() {
        let mut control = Connection::open_in_memory().unwrap();
        let mut company = Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut company, "Synthetic").unwrap();
        let input: crate::company_creation::Input=serde_json::from_value(json!({"name":"Synthetic","slug":"synthetic","fiscalYearStartMonth":"3","fiscalYearLabelStrategy":"span","vatPeriodType":null,"address":"Synthetic street","paymentTermsDays":14})).unwrap();
        let settings = crate::company_creation::settings(&input).unwrap();
        let result = initialize(&mut company, &settings, "user:synthetic").unwrap();
        assert_eq!(result["profile"]["fiscalYearStartMonth"], 3);
        assert_eq!(result["profile"]["vatPeriodType"], Value::Null);
        assert!(result["accountCount"].as_u64().unwrap() > 40);
        assert!(initialize(&mut company, &settings, "user:synthetic").is_err());
    }
}
