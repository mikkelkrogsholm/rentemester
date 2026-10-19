//! Mandatory current native readers; no injected success callbacks or whole-year substitution.
use super::*;
pub fn final_checks(tx: &Transaction<'_>, scope: &Scope) -> Result<Value, Error> {
    let audit = crate::journal_hash::verify(tx)?;
    let tb = crate::trial_balance_owned(
        tx,
        rentemester_engine::trial_balance::Period::new(
            &scope.accounting_from,
            &scope.accounting_to,
        )?,
    )?;
    let reconciliation = range_reconciliation(tx, &scope.bank_from, &scope.bank_to)?;
    let vat = crate::vat_report::range(tx, &scope.accounting_from, &scope.accounting_to)?;
    Ok(
        json!([{"name":"audit_chain","ok":audit.ok,"detail":audit},{"name":"trial_balance","ok":tb.ok&&tb.balanced,"detail":tb},{"name":"reconciliation","ok":reconciliation["ok"],"detail":reconciliation},{"name":"vat","ok":vat["errors"].as_array().is_some_and(Vec::is_empty),"detail":vat}]),
    )
}
fn range_reconciliation(tx: &Transaction<'_>, from: &str, to: &str) -> Result<Value, Error> {
    let rows = rows(
        tx,
        "SELECT bt.id,br.journal_entry_id,br.reconciliation_id,EXISTS(SELECT 1 FROM journal_entries r WHERE r.reversal_of_entry_id=br.journal_entry_id) AS reversed FROM bank_transactions bt LEFT JOIN bank_journal_reconciliations br ON br.bank_transaction_id=bt.id WHERE bt.transaction_date BETWEEN ?1 AND ?2 ORDER BY bt.id",
        params![from, to],
    )?;
    let unmatched = rows
        .iter()
        .filter(|r| r["journal_entry_id"].is_null() || r["reversed"] == 1)
        .collect::<Vec<_>>();
    // Exact range eligibility and current canonical relation are the mandatory source batch check.
    // A statement closing balance is a separate fact: report unavailable explicitly, never assume zero.
    let statement = crate::bank::statement_balance(tx, to)?;
    Ok(
        json!({"ok":unmatched.is_empty(),"periodStart":from,"periodEnd":to,"total":rows.len(),"matched":rows.len()-unmatched.len(),"unmatched":unmatched,"statementBalance":statement.balance,"statementStatus":statement.status,"statementProblem":statement.problem}),
    )
}
