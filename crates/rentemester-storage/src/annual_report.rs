//! Read-only annual preparation over the shared exact statement reducers and effective locks.
use crate::{cockpit::number, invalid};
use rentemester_engine::{
    Error, annual_report, financial_statements as statements, fiscal_year::Date,
    trial_balance::Period,
};
use rusqlite::Transaction;
use serde_json::{Value, json};
fn section(s: &statements::Section) -> Result<Value, Error> {
    let rows = s
        .lines
        .iter()
        .map(|l| Ok(json!({"accountNo":l.account_no,"name":l.name,"amount":number(&l.amount)?})))
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(json!({"total":number(&s.total)?,"lines":rows}))
}
pub fn read(tx: &Transaction<'_>, slug: &str, start: &str, end: &str) -> Result<Value, Error> {
    crate::validate(tx)?;
    if Date::parse(start)? > Date::parse(end)? {
        return Err(invalid("fiscalYearStart must not exceed fiscalYearEnd"));
    }
    // This validates the immutable opening proof as well as current live coverage.
    // An archive without explicit adoption cannot supply a live annual report.
    let basis = crate::opening_balance::require_report_range(tx, start, end)?;
    let prior_start = annual_report::prior_date(start)?;
    let prior_end = annual_report::prior_date(end)?;
    let company = crate::company_setup::profile(tx)?;
    let mut errors = vec![];
    let cvr = company["cvr"].as_str().unwrap_or("");
    let digits = cvr.strip_prefix("DK").unwrap_or(cvr);
    if digits.len() != 8 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        errors.push("Virksomhedens CVR-nummer mangler eller er ugyldigt — registrér et 8-cifret CVR-nummer.".to_string());
    }
    let mut periods=tx.prepare("SELECT id,status FROM accounting_periods WHERE period_start<=?1 AND period_end>=?2 ORDER BY id").map_err(invalid)?;
    let candidates = periods
        .query_map([start, end], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(invalid)?;
    let mut locked = false;
    for candidate in candidates {
        let (id, status) = candidate.map_err(invalid)?;
        if matches!(
            crate::periods::effective(tx, id, status)?.as_str(),
            "closed" | "reported"
        ) {
            locked = true;
        }
    }
    if !locked {
        errors.push(format!("Regnskabsåret {start}..{end} er ikke låst: luk en regnskabsperiode, der dækker hele regnskabsåret, under Periodelås."));
    }
    let tb = crate::trial_balance_owned(tx, Period::new(start, end)?)?;
    if !tb.balanced {
        errors.push("Bøgerne balancerer ikke for regnskabsåret.".into());
    }
    let pl = statements::profit_and_loss(&tb);
    let bs = statements::balance_sheet(&crate::trial_balance_owned(
        tx,
        Period::new("0000-01-01", end)?,
    )?);
    if tb.balanced && !bs.balanced {
        errors.push(
            "Balancen på balancedagen stemmer ikke; kontrollér også tidligere års posteringer."
                .into(),
        );
    }
    let prior_covered = match basis["cutOverDate"].as_str() {
        Some(cutover) => Date::parse(&prior_start)? >= Date::parse(cutover)?,
        None => true,
    };
    let comparison = if prior_covered {
        let prior_tb = crate::trial_balance_owned(tx, Period::new(&prior_start, &prior_end)?)?;
        let prior_pl = statements::profit_and_loss(&prior_tb);
        let prior_bs = statements::balance_sheet(&crate::trial_balance_owned(
            tx,
            Period::new("0000-01-01", &prior_end)?,
        )?);
        // Match live report equity: the shared reducer already includes unclosed cumulative results.
        let available = prior_pl.income.total != Default::default()
            || prior_pl.expense.total != Default::default()
            || prior_bs.assets.total != Default::default()
            || prior_bs.total_liabilities_and_equity != Default::default();
        json!({"fiscalYearStart":prior_start,"fiscalYearEnd":prior_end,"available":available,"totalIncome":number(&prior_pl.income.total)?,"totalExpense":number(&prior_pl.expense.total)?,"aretsResultat":number(&prior_pl.result)?,"totalAssets":number(&prior_bs.assets.total)?,"totalLiabilitiesAndEquity":number(&prior_bs.total_liabilities_and_equity)?,"equity":number(&prior_bs.equity.total)?})
    } else {
        // The historical range is not fully represented by the adopted live ledger.
        // Do not reduce a partial prior year or manufacture zero comparison figures.
        json!({"fiscalYearStart":prior_start,"fiscalYearEnd":prior_end,"available":false,"reason":"prior_range_precedes_explicit_cutover","totalIncome":null,"totalExpense":null,"aretsResultat":null,"totalAssets":null,"totalLiabilitiesAndEquity":null,"equity":null})
    };
    let ok = errors.is_empty();
    let placeholders = annual_report::placeholders(end);
    let report = json!({"ok":ok,"appliedRules":["DK-ANNUAL-REPORT-CLASS-B-001"],"regnskabsklasse":"B","fiscalYearStart":start,"fiscalYearEnd":end,"company":company,
        "profitAndLoss":{"ok":true,"income":section(&pl.income)?,"expense":section(&pl.expense)?,"totalIncome":number(&pl.income.total)?,"totalExpense":number(&pl.expense.total)?,"result":number(&pl.result)?,"errors":[]},
        "balanceSheet":{"ok":true,"assets":section(&bs.assets)?,"liabilities":section(&bs.liabilities)?,"equity":section(&bs.equity)?,"periodResult":number(&bs.period_result)?,"totalAssets":number(&bs.assets.total)?,"totalLiabilitiesAndEquity":number(&bs.total_liabilities_and_equity)?,"balanced":bs.balanced,"errors":[]},
        "aretsResultat":number(&pl.result)?,"comparison":comparison,
        "notes":if ok {placeholders["notes"].clone()} else {json!([])},"ledelsespategning":if ok {placeholders["ledelsespategning"].clone()} else {json!({"text":"","placeholder":true})},
        "preparedBy":"Rentemester","disclaimer":"Rentemester forbereder denne årsrapport ud fra det lukkede regnskabsår. Ejer eller revisor gennemgår og er ansvarlig for indberetning til Erhvervsstyrelsen.","errors":errors});
    Ok(
        json!({"slug":slug,"company":company,"fiscalYearStart":start,"fiscalYearEnd":end,"report":report}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn blockers_effective_reopen_shifted_comparison_and_restart_are_read_only() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("ledger.sqlite");
        let mut control = rusqlite::Connection::open_in_memory().unwrap();
        let mut db = rusqlite::Connection::open(&path).unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic Annual").unwrap();
        db.execute_batch("UPDATE companies SET fiscal_year_start_month=7,fiscal_year_label_strategy='span' WHERE id=1;INSERT INTO accounts(id,account_no,name,type,normal_balance) VALUES(1,'A','Synthetic cash','asset','debit'),(2,'I','Synthetic revenue','income','credit'),(3,'X','Synthetic cost','expense','debit');INSERT INTO journal_entries(id,entry_no,transaction_date,text,rule_version,entry_hash) VALUES(1,'SYN-1','2024-08-01','Synthetic prior','fixture','hash1'),(2,'SYN-2','2025-08-01','Synthetic current','fixture','hash2');INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount) VALUES(1,1,200.02,0),(1,2,0,200.02),(2,3,40.01,0),(2,1,0,40.01);").unwrap();
        let call = |db: &mut rusqlite::Connection| {
            read(
                &db.transaction().unwrap(),
                "synthetic",
                "2025-07-01",
                "2026-06-30",
            )
            .unwrap()
        };
        let first = call(&mut db);
        assert_eq!(first["report"]["errors"].as_array().unwrap().len(), 2);
        db.execute("UPDATE companies SET cvr='87654321' WHERE id=1", [])
            .unwrap();
        assert_eq!(
            call(&mut db)["report"]["errors"].as_array().unwrap().len(),
            1
        );
        // Historical closed row plus later reopen: current audit state must win.
        db.execute_batch("INSERT INTO accounting_periods(id,period_start,period_end,kind,status,closed_at,closed_by) VALUES(1,'2025-07-01','2026-06-30','fiscal_year','closed','2026-07-01','user:synthetic');").unwrap();
        let changes = db.total_changes();
        let prepared = call(&mut db);
        assert_eq!(prepared["report"]["ok"], true);
        assert_eq!(db.total_changes(), changes);
        {
            let tx = db.transaction().unwrap();
            let live = crate::cockpit::read(&tx, "synthetic", "2026-10-19", Some("2025/26"), None)
                .unwrap();
            let prior = crate::cockpit::read(&tx, "synthetic", "2026-10-19", Some("2024/25"), None)
                .unwrap();
            let report = &prepared["report"];
            assert_eq!(
                report["profitAndLoss"]["result"],
                live.income_statement["result"]
            );
            assert_eq!(
                report["balanceSheet"]["totalAssets"],
                live.balance["totalAssets"]
            );
            assert_eq!(
                report["balanceSheet"]["equity"]["total"],
                live.balance["equity"]["total"]
            );
            assert_eq!(
                report["comparison"]["aretsResultat"],
                prior.income_statement["result"]
            );
            assert_eq!(
                report["comparison"]["equity"],
                prior.balance["equity"]["total"]
            );
            assert_eq!(report["comparison"]["fiscalYearStart"], "2024-07-01");
            assert_eq!(report["comparison"]["available"], true);
        }
        db.execute_batch("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES('period_reopen','accounting_period','1','Synthetic reopen','user:synthetic');").unwrap();
        assert_eq!(call(&mut db)["report"]["ok"], false);
        db.execute_batch("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES('period_close','accounting_period','1','Synthetic close','user:synthetic');").unwrap();
        assert_eq!(call(&mut db), prepared);
        drop(db);
        let mut db = rusqlite::Connection::open(&path).unwrap();
        assert_eq!(call(&mut db), prepared);
        let tx = db.transaction().unwrap();
        assert!(read(&tx, "synthetic", "2026-02-29", "2026-12-31").is_err());
        assert!(read(&tx, "synthetic", "2026-12-31", "2026-01-01").is_err());
        assert_eq!(
            tx.query_row::<i64, _, _>("SELECT count(*) FROM journal_entries", [], |r| r.get(0))
                .unwrap(),
            2
        );
    }
    #[test]
    fn incomplete_coverage_and_unbalanced_books_do_not_prepare() {
        let mut c = rusqlite::Connection::open_in_memory().unwrap();
        let mut l = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic Annual").unwrap();
        l.execute_batch("UPDATE companies SET cvr='DK87654321' WHERE id=1;INSERT INTO accounting_periods(period_start,period_end,kind,status,closed_at,closed_by) VALUES('2025-01-01','2025-06-30','custom','closed','2026-01-01','user:synthetic');INSERT INTO accounts(id,account_no,name,type,normal_balance) VALUES(1,'A','Synthetic asset','asset','debit');INSERT INTO journal_entries(id,entry_no,transaction_date,text,rule_version,entry_hash) VALUES(1,'SYN','2025-05-01','Synthetic broken','fixture','hash');INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount) VALUES(1,1,10,0);").unwrap();
        let result = read(
            &l.transaction().unwrap(),
            "synthetic",
            "2025-01-01",
            "2025-12-31",
        )
        .unwrap();
        assert_eq!(result["report"]["ok"], false);
        assert_eq!(result["report"]["errors"].as_array().unwrap().len(), 2);
        assert_eq!(result["report"]["notes"], json!([]));
        // A balanced empty current year must not hide a broken opening balance.
        l.execute_batch("INSERT INTO accounting_periods(period_start,period_end,kind,status,closed_at,closed_by) VALUES('2026-01-01','2026-12-31','fiscal_year','closed','2027-01-01','user:synthetic');").unwrap();
        let result = read(
            &l.transaction().unwrap(),
            "synthetic",
            "2026-01-01",
            "2026-12-31",
        )
        .unwrap();
        assert_eq!(result["report"]["ok"], false);
        assert_eq!(result["report"]["errors"].as_array().unwrap().len(), 1);
        assert!(
            result["report"]["errors"][0]
                .as_str()
                .unwrap()
                .contains("balancedagen")
        );
    }
}
