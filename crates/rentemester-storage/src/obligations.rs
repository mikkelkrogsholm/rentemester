//! Authorized read projection. Balances are exact; no payment or filing is inferred.
//! Deadline basis: https://skat.dk/erhverv/moms/frister-indberet-og-betal-moms
//! Annual applicability/class is unavailable; never assume a universal six-month deadline.
//! https://erhvervsstyrelsen.dk/vejledning-aarsrapporter (verified 2026-10-19).
use crate::{invalid, legacy_money};
use rentemester_engine::{Error, money::Money, sales_invoice::wire, vat_period};
use rusqlite::{OptionalExtension, Transaction, params};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub fn read(
    tx: &Transaction<'_>,
    slug: &str,
    today: &str,
    year: Option<&str>,
) -> Result<Value, Error> {
    let snapshot = crate::cockpit::read(tx, slug, today, year, None)?;
    let mut base = snapshot.trial_balance;
    let start = base["periodStart"]
        .as_str()
        .ok_or_else(|| invalid("fiscal start"))?
        .to_owned();
    let end = base["periodEnd"]
        .as_str()
        .ok_or_else(|| invalid("fiscal end"))?
        .to_owned();
    let as_of = today.min(end.as_str());
    let currency = base["company"]["currency"]
        .as_str()
        .ok_or_else(|| invalid("currency"))?;
    let country = base["company"]["country"].as_str().unwrap_or("");
    let mut excluded = vec![json!(
        "Betalingsfrister for almindelig gæld og årsrapportens pligt/fristen kræver supplerende oplysninger. Ingen skattemæssig frist udledes af kontonumre."
    )];
    let mut vat_accounts = BTreeSet::new();
    let mut roles = BTreeMap::new();
    let mut q = tx.prepare("SELECT role,account_no FROM account_role_mappings WHERE status='confirmed' ORDER BY role").map_err(invalid)?;
    for row in q
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(invalid)?
    {
        let (role, account) = row.map_err(invalid)?;
        if [
            "input_vat",
            "output_vat",
            "reverse_charge_vat",
            "vat_settlement",
        ]
        .contains(&role.as_str())
        {
            vat_accounts.insert(account.clone());
        }
        roles.insert(role, account);
    }
    let mut q = tx.prepare("SELECT a.account_no,a.name,a.type,je.currency,jl.debit_amount,jl.credit_amount FROM accounts a JOIN journal_lines jl ON jl.account_id=a.id JOIN journal_entries je ON je.id=jl.journal_entry_id WHERE a.type='liability' AND je.status IN ('posted','reversed') AND je.transaction_date<=?1 ORDER BY a.account_no,je.id,jl.id").map_err(invalid)?;
    let mut rs = q.query([as_of]).map_err(invalid)?;
    let mut balances: BTreeMap<String, (String, Money)> = BTreeMap::new();
    let mut foreign = BTreeSet::new();
    while let Some(r) = rs.next().map_err(invalid)? {
        let no: String = r.get(0).map_err(invalid)?;
        if vat_accounts.contains(&no) {
            continue;
        }
        let line_currency: String = r.get(3).map_err(invalid)?;
        if line_currency != currency {
            foreign.insert(no);
            continue;
        }
        let amount = legacy_money(r.get_ref(5).map_err(invalid)?)?
            .subtract(&legacy_money(r.get_ref(4).map_err(invalid)?)?);
        balances
            .entry(no)
            .or_insert((r.get(1).map_err(invalid)?, Money::default()))
            .1
            .add(&amount);
    }
    let mut rows = vec![];
    let mut total = Money::default();
    for (no, (name, amount)) in balances {
        if foreign.contains(&no) || amount.is_negative() || amount == Money::default() {
            continue;
        }
        total.add(&amount);
        rows.push(json!({"kind":if roles.get("creditors")==Some(&no){"creditors"}else{"other"},"label":name,"amount":wire(&amount)?,"dueDate":null,"daysRemaining":null,"accountNo":no,"settlementStatus":"ledger-balance"}));
    }
    if !foreign.is_empty() {
        excluded.push(json!(format!(
            "Konti med valuta forskellig fra virksomhedens valuta er udeladt uden valutakurs: {}",
            foreign.into_iter().collect::<Vec<_>>().join(", ")
        )));
    }
    let cadence: Option<String> = tx
        .query_row(
            "SELECT vat_period_type FROM companies WHERE id=1",
            [],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    if let Some(cadence) = cadence {
        if country != "DK" || currency != "DKK" {
            excluded.push(json!("Dansk moms er udeladt: virksomhedens land/valuta understøttes ikke af den danske momsberegning."));
        } else {
            excluded.push(json!("Moms vises før eventuelle betalinger. Betalinger kan ikke fordeles sikkert på momsperioder; indberettet betyder ikke betalt. Åbne perioder er foreløbige. Momsperioder med slutdato i regnskabsåret medtages i deres helhed."));
            let span = vat_period::span(&cadence)?;
            let first: i32 = start[..4].parse().map_err(invalid)?;
            let last: i32 = end[..4].parse().map_err(invalid)?;
            for year in first..=last {
                for month in (1..=12).step_by(span as usize) {
                    let w = vat_period::window(&format!("{year:04}-{month:02}-01"), &cadence)?;
                    if w.end < start || w.end > end || w.start.as_str() > as_of {
                        continue;
                    }
                    let report = crate::vat_report::range(tx, &w.start, w.end.as_str().min(as_of))?;
                    if report["errors"].as_array().is_none_or(|e| !e.is_empty()) {
                        excluded.push(json!(format!(
                            "Moms — {} er udeladt: momsgrundlaget kunne ikke valideres.",
                            w.label
                        )));
                        continue;
                    }
                    let amount = Money::parse(&report["payable"].to_string())?;
                    if amount.is_negative() || amount == Money::default() {
                        continue;
                    }
                    let period: Option<(i64, String)> = tx.query_row("SELECT id,status FROM accounting_periods WHERE kind IN ('vat_period','vat_quarter') AND period_start=?1 AND period_end=?2 ORDER BY CASE kind WHEN 'vat_period' THEN 0 ELSE 1 END,id DESC LIMIT 1",params![w.start,w.end],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(invalid)?;
                    let status = match period {
                        Some((id, status)) => crate::periods::effective(tx, id, status)?,
                        None => "open".into(),
                    };
                    total.add(&amount);
                    rows.push(json!({"kind":"vat","label":format!("Moms — {}",w.label),"amount":wire(&amount)?,"dueDate":w.deadline,"daysRemaining":(vat_period::date(&w.deadline)?-vat_period::date(today)?).whole_days(),"accountNo":null,"periodStart":w.start,"periodEnd":w.end,"periodStatus":status,"provisional":status=="open" || w.end.as_str()>as_of,"settlementStatus":"unknown","details":report}));
                }
            }
        }
    }
    // Applicability/reporting class is not stored. Six months is not universal (class D: four).
    rows.push(json!({"kind":"annual-report","label":format!("Årsrapport — regnskabsår {}",base["selectedYear"].as_str().unwrap_or("")),"amount":0,"dueDate":null,"daysRemaining":null,"accountNo":null,"note":"Indberetningspligt og regnskabsklasse mangler; kontrollér fristen hos Erhvervsstyrelsen. Dette er en indberetning, ikke gæld."}));
    rows.sort_by(|a, b| {
        let due = |v: &Value| v["dueDate"].as_str().unwrap_or("9999-99-99").to_owned();
        due(a)
            .cmp(&due(b))
            .then_with(|| a["label"].as_str().cmp(&b["label"].as_str()))
            .then_with(|| a["accountNo"].as_str().cmp(&b["accountNo"].as_str()))
    });
    let object = base
        .as_object_mut()
        .ok_or_else(|| invalid("obligations base"))?;
    for key in ["rows", "totalDebit", "totalCredit", "balanced"] {
        object.remove(key);
    }
    base["asOfDate"] = json!(as_of);
    base["obligations"] = json!(rows);
    base["totalOwed"] = wire(&total)?;
    base["coverage"] = json!({"complete":false,"notes":excluded});
    Ok(base)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, rusqlite::Connection) {
        let dir = tempfile::tempdir().unwrap();
        let mut control = rusqlite::Connection::open_in_memory().unwrap();
        let mut company = rusqlite::Connection::open(dir.path().join("company.sqlite")).unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut company, "Synthetic Obligations")
            .unwrap();
        company.execute_batch("UPDATE companies SET fiscal_year_start_month=7,fiscal_year_label_strategy='span',vat_period_type='quarter';
          INSERT INTO accounts(id,account_no,name,type,normal_balance) VALUES(1,'CASH','Cash','asset','debit'),(2,'OWED','Synthetic configured liability','liability','credit'),(3,'DEBIT','Debit is not debt','liability','credit'),(4,'VAT','VAT mapped liability','liability','credit'),(5,'SALES','Sales','income','credit'),(6,'FX','Mixed currency debt','liability','credit');
          INSERT INTO account_role_mappings(role,account_no,version,confirmed_by) VALUES('output_vat','VAT',1,'agent:synthetic');
          INSERT INTO journal_entries(id,entry_no,transaction_date,text,rule_version,entry_hash) VALUES(1,'S1','2025-09-01','Liability','fixture','hash1'),(2,'S2','2026-06-01','Liability cents','fixture','hash2'),(3,'S3','2026-07-01','Next fiscal year','fixture','hash3'),(4,'S4','2025-10-01','VAT sale','fixture','hash4');
          INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount) VALUES(1,1,10.01,0),(1,2,0,10.01),(2,1,0.02,0),(2,2,0,0.02),(2,3,4,0),(2,1,0,4),(3,1,70,0),(3,2,0,70),(4,1,125,0),(4,4,0,25);
          INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount,vat_code) VALUES(4,5,0,100,'DK_SALE_25');").unwrap();
        (dir, company)
    }
    #[test]
    fn shifted_cutoff_exact_balances_vat_exclusion_deadlines_and_restart() {
        let (dir, mut company) = fixture();
        let before = company.total_changes();
        let value = read(
            &company.transaction().unwrap(),
            "one",
            "2026-10-19",
            Some("2025/26"),
        )
        .unwrap();
        assert_eq!(company.total_changes(), before);
        assert_eq!(value["periodEnd"], "2026-06-30");
        assert_eq!(value["asOfDate"], "2026-06-30");
        assert_eq!(value["totalOwed"], 35.03);
        let rows = value["obligations"].as_array().unwrap();
        assert_eq!(rows.len(), 3);
        assert!(
            !rows
                .iter()
                .any(|r| ["VAT", "DEBIT"].contains(&r["accountNo"].as_str().unwrap_or("")))
        );
        let vat = rows.iter().find(|r| r["kind"] == "vat").unwrap();
        assert_eq!(vat["periodStart"], "2025-10-01");
        assert_eq!(vat["dueDate"], "2026-03-02");
        assert_eq!(vat["settlementStatus"], "unknown");
        let annual = rows.iter().find(|r| r["kind"] == "annual-report").unwrap();
        assert!(annual["dueDate"].is_null());
        assert_eq!(annual["amount"], 0);
        assert!(!value["coverage"]["complete"].as_bool().unwrap());
        drop(company);
        let mut again = rusqlite::Connection::open(dir.path().join("company.sqlite")).unwrap();
        assert_eq!(
            read(
                &again.transaction().unwrap(),
                "one",
                "2026-10-19",
                Some("2025/26")
            )
            .unwrap(),
            value
        );
        let current = read(
            &again.transaction().unwrap(),
            "one",
            "2026-10-19",
            Some("2026/27"),
        )
        .unwrap();
        assert_eq!(current["totalOwed"], 80.03);
        assert!(
            read(
                &again.transaction().unwrap(),
                "one",
                "2026-10-19",
                Some("2030/31")
            )
            .is_err()
        );
    }
    #[test]
    fn unvalidated_vat_and_mixed_currency_never_become_zero_or_unconverted_debt() {
        let (_dir, mut company) = fixture();
        company.execute_batch("INSERT INTO journal_entries(id,entry_no,transaction_date,text,rule_version,entry_hash,currency) VALUES(5,'S5','2026-01-01','Foreign','fixture','hash5','EUR');INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount) VALUES(5,1,9,0),(5,6,0,9); INSERT INTO journal_entries(id,entry_no,transaction_date,text,rule_version,entry_hash) VALUES(6,'S6','2025-10-02','Unvalidated VAT','fixture','hash6');INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount) VALUES(6,4,0,1),(6,1,1,0);").unwrap();
        let value = read(
            &company.transaction().unwrap(),
            "one",
            "2026-10-19",
            Some("2025/26"),
        )
        .unwrap();
        assert_eq!(value["totalOwed"], 10.03);
        assert!(
            !value["obligations"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["kind"] == "vat" || r["accountNo"] == "FX")
        );
        let notes = value["coverage"]["notes"].to_string();
        assert!(notes.contains("FX") && notes.contains("kunne ikke valideres"));
    }
}
