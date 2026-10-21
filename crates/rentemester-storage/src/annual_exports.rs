//! Annual preparation exports on the caller's single admitted read transaction.
use crate::invalid;
use rentemester_engine::{
    Error, financial_statements, ixbrl, money::Money, tax_return, trial_balance::Period,
};
use rusqlite::Transaction;
use serde_json::Value;
fn same(value: &Value, exact: &Money) -> Result<(), Error> {
    if crate::invoice_fx::amount(value)? != *exact {
        return Err(invalid(
            "Annual export amount exceeds exact numeric presentation",
        ));
    }
    Ok(())
}
/// Check native exact reducer totals before consuming the compatibility numeric annual DTO.
fn require_exact(
    tx: &Transaction<'_>,
    report: &Value,
    start: &str,
    end: &str,
) -> Result<(), Error> {
    let pl = financial_statements::profit_and_loss(&crate::trial_balance_owned(
        tx,
        Period::new(start, end)?,
    )?);
    let bs = financial_statements::balance_sheet(&crate::trial_balance_owned(
        tx,
        Period::new("0000-01-01", end)?,
    )?);
    for (v, m) in [
        (&report["aretsResultat"], &pl.result),
        (&report["profitAndLoss"]["totalIncome"], &pl.income.total),
        (&report["profitAndLoss"]["totalExpense"], &pl.expense.total),
        (&report["balanceSheet"]["totalAssets"], &bs.assets.total),
        (
            &report["balanceSheet"]["liabilities"]["total"],
            &bs.liabilities.total,
        ),
        (&report["balanceSheet"]["equity"]["total"], &bs.equity.total),
        (&report["balanceSheet"]["periodResult"], &bs.period_result),
        (
            &report["balanceSheet"]["totalLiabilitiesAndEquity"],
            &bs.total_liabilities_and_equity,
        ),
    ] {
        same(v, m)?;
    }
    let prior = &report["comparison"];
    if prior["available"] == true {
        let from = prior["fiscalYearStart"]
            .as_str()
            .ok_or_else(|| invalid("Prior annual range unavailable"))?;
        let to = prior["fiscalYearEnd"]
            .as_str()
            .ok_or_else(|| invalid("Prior annual range unavailable"))?;
        let pl = financial_statements::profit_and_loss(&crate::trial_balance_owned(
            tx,
            Period::new(from, to)?,
        )?);
        let bs = financial_statements::balance_sheet(&crate::trial_balance_owned(
            tx,
            Period::new("0000-01-01", to)?,
        )?);
        for (v, m) in [
            (&prior["totalIncome"], &pl.income.total),
            (&prior["totalExpense"], &pl.expense.total),
            (&prior["aretsResultat"], &pl.result),
            (&prior["totalAssets"], &bs.assets.total),
            (&prior["equity"], &bs.equity.total),
            (
                &prior["totalLiabilitiesAndEquity"],
                &bs.total_liabilities_and_equity,
            ),
        ] {
            same(v, m)?;
        }
    }
    Ok(())
}
pub fn read(
    tx: &Transaction<'_>,
    slug: &str,
    start: &str,
    end: &str,
    kind: &str,
) -> Result<Value, Error> {
    if !matches!(kind, "tax" | "ixbrl") {
        return Err(Error::new(
            "invalid_annual_export",
            "Annual export kind must be tax or ixbrl",
        ));
    }
    let source = crate::annual_report::read(tx, slug, start, end)?;
    let report = &source["report"];
    let ok = report["ok"]
        .as_bool()
        .ok_or_else(|| invalid("Annual report readiness unavailable"))?;
    if ok {
        require_exact(tx, report, start, end)?;
    }
    match kind {
        "ixbrl" => {
            let mut export = ixbrl::generate(report)?;
            export["annualReport"] = report.clone();
            Ok(export)
        }
        _ => {
            if !ok {
                return tax_return::prepare(report, &Money::default(), &Money::default());
            }
            let vat = crate::vat_report::range_report(tx, start, end)?;
            if !vat.errors.is_empty() {
                return Err(Error::new(
                    "tax_source_unavailable",
                    "Shared VAT report did not pass its source controls",
                ));
            }
            // Original warning is accumulated posted depreciation, deliberately not annual depreciation.
            let assets = crate::assets::list(tx, slug)?;
            let rows = assets["assets"]
                .as_array()
                .ok_or_else(|| invalid("Asset register unavailable"))?;
            let mut depreciation = Money::default();
            for asset in rows {
                let posted = asset["postedPeriods"]
                    .as_u64()
                    .ok_or_else(|| invalid("Asset period count unavailable"))?;
                if posted > 0 {
                    depreciation.add(&crate::invoice_fx::amount(
                        &asset["accumulatedDepreciation"],
                    )?);
                }
            }
            tax_return::prepare(report, &vat.representation_purchase_base, &depreciation)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn ledger() -> rusqlite::Connection {
        let mut c = rusqlite::Connection::open_in_memory().unwrap();
        let mut l = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic annual export").unwrap();
        l.execute_batch("UPDATE companies SET company_form='ApS' WHERE id=1;INSERT INTO accounts(id,account_no,name,type,normal_balance) VALUES(1,'B','Synthetic cash','asset','debit'),(2,'I','Synthetic income','income','credit'),(3,'R','Synthetic representation','expense','debit'),(4,'V','Synthetic input VAT','vat','debit');INSERT INTO journal_entries(id,entry_no,transaction_date,text,rule_version,entry_hash) VALUES(1,'S-1','2025-02-01','Synthetic revenue','fixture','hash1'),(2,'S-2','2025-03-01','Synthetic representation','fixture','hash2');INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount,vat_code) VALUES(1,1,100,0,NULL),(1,2,0,100,'DK_SALE_EXEMPT'),(2,3,0.06,0,'REPRESENTATION_SPECIAL'),(2,3,0.01,0,'REPRESENTATION_NON_DEDUCTIBLE_VAT'),(2,4,0.01,0,NULL),(2,1,0,0.08,NULL);").unwrap();
        l
    }
    fn call(db: &mut rusqlite::Connection, kind: &str) -> Result<Value, Error> {
        read(
            &db.transaction().unwrap(),
            "synthetic",
            "2025-01-01",
            "2025-12-31",
            kind,
        )
    }
    #[test]
    fn closed_year_tax_ixbrl_read_only_and_effective_reopen_gate() {
        let mut db = ledger();
        let refusal = call(&mut db, "tax").unwrap();
        assert_eq!(refusal["ok"], false);
        assert!(refusal["corporateTax"].is_null());
        assert_eq!(call(&mut db, "ixbrl").unwrap()["xhtml"], "");
        db.execute_batch("UPDATE companies SET cvr='87654321' WHERE id=1;INSERT INTO accounting_periods(id,period_start,period_end,kind,status,closed_at,closed_by)VALUES(1,'2025-01-01','2025-12-31','fiscal_year','closed','2026-01-01','user:synthetic')").unwrap();
        let before = db.total_changes();
        let tax = call(&mut db, "tax").unwrap();
        assert_eq!(tax["ok"], true);
        assert_eq!(tax["bookkeptResult"], 99.93);
        assert_eq!(tax["totalAdjustments"], 0.05);
        assert_eq!(tax["taxableIncome"], 99.98);
        assert_eq!(tax["corporateTax"], 22);
        let export = call(&mut db, "ixbrl").unwrap();
        assert_eq!(export["ok"], true);
        assert_eq!(export["annualReport"]["aretsResultat"], 99.93);
        assert_eq!(export, call(&mut db, "ixbrl").unwrap());
        assert_eq!(db.total_changes(), before);
        db.execute_batch("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor)VALUES('period_reopen','accounting_period','1','Synthetic reopen','user:synthetic')").unwrap();
        let r = call(&mut db, "ixbrl").unwrap();
        assert_eq!(r["ok"], false);
        assert_eq!(r["annualReport"]["ok"], false);
        assert_eq!(r["xhtml"], "");
        assert_eq!(call(&mut db, "tax").unwrap()["ok"], false);
    }
    #[test]
    fn shared_vat_failure_refuses_instead_of_guessing_zero_adjustment() {
        let mut db = ledger();
        db.execute_batch("UPDATE companies SET cvr='87654321' WHERE id=1;INSERT INTO accounting_periods(period_start,period_end,kind,status,closed_at,closed_by)VALUES('2025-01-01','2025-12-31','fiscal_year','closed','2026-01-01','user:synthetic');INSERT INTO journal_entries(id,entry_no,transaction_date,text,rule_version,entry_hash)VALUES(3,'S-3','2025-04-01','Synthetic unsupported VAT','fixture','hash3');INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount)VALUES(3,4,1,0),(3,1,0,1)").unwrap();
        assert_eq!(
            call(&mut db, "tax").unwrap_err().code,
            "tax_source_unavailable"
        );
        assert!(call(&mut db, "unknown").is_err());
    }
}
