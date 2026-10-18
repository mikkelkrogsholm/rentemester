//! Native close assurance supplements persisted journal hashes with structural invariants.
use super::readiness::rows;
use crate::{invalid, legacy_money};
use rentemester_engine::{Error, money::Money};
use rusqlite::Transaction;
use serde_json::{Value, json};
pub(super) fn verify(tx: &Transaction<'_>) -> Result<Vec<Value>, Error> {
    let mut errors = crate::journal_hash::verify(tx)?
        .errors
        .into_iter()
        .map(|error| json!({"error":error}))
        .collect::<Vec<_>>();
    errors.extend(rows(tx, "PRAGMA foreign_key_check", &[])?);
    for (code, sql) in [
        (
            "INVALID_JOURNAL_LINE",
            "SELECT id FROM journal_lines WHERE debit_amount<0 OR credit_amount<0 OR (debit_amount=0 AND credit_amount=0) OR (debit_amount>0 AND credit_amount>0)",
        ),
        (
            "INVALID_REVERSAL_STATUS",
            "SELECT id FROM journal_entries WHERE (status='posted' AND reversal_of_entry_id IS NOT NULL) OR (status='reversed' AND reversal_of_entry_id IS NULL) OR status NOT IN ('posted','reversed')",
        ),
        (
            "INVALID_REVERSAL_TARGET",
            "SELECT r.id FROM journal_entries r LEFT JOIN journal_entries o ON o.id=r.reversal_of_entry_id WHERE r.reversal_of_entry_id IS NOT NULL AND (o.id IS NULL OR o.id>=r.id OR o.status!='posted' OR o.reversal_of_entry_id IS NOT NULL OR r.document_id IS NOT o.document_id OR r.source_bank_transaction_id IS NOT o.source_bank_transaction_id OR r.currency IS NOT o.currency OR r.amount_foreign IS NOT o.amount_foreign OR r.amount_dkk IS NOT o.amount_dkk OR r.fx_rate_to_dkk IS NOT o.fx_rate_to_dkk)",
        ),
        (
            "MULTIPLE_REVERSALS",
            "SELECT reversal_of_entry_id AS id FROM journal_entries WHERE reversal_of_entry_id IS NOT NULL GROUP BY reversal_of_entry_id HAVING count(*)>1",
        ),
        (
            "MISSING_JOURNAL_AUDIT",
            "SELECT j.id FROM journal_entries j WHERE NOT EXISTS(SELECT 1 FROM audit_log a WHERE a.entity_type='journal_entry' AND a.entity_id=CAST(j.id AS TEXT))",
        ),
        (
            "BANK_MULTIPLE_RECONCILIATIONS",
            "SELECT bank_transaction_id AS id FROM bank_journal_reconciliations GROUP BY bank_transaction_id HAVING count(*)!=1",
        ),
        (
            "UNSUPPORTED_HISTORICAL_PROVENANCE",
            "SELECT id FROM journal_entries WHERE created_by_program='rentemester-import-postings'",
        ),
    ] {
        for value in rows(tx, sql, &[])? {
            errors.push(json!({"code":code,"source":value}));
        }
    }
    for table in [
        "bank_journal_reconciliation_links",
        "bank_reconciliation_correction_events",
        "direct_bank_purchase_payable_corrections",
        "invoice_bad_debt_writeoffs",
        "invoice_claim_payments",
        "invoice_compensation_claims",
        "invoice_interest_claims",
        "imported_receivable_headers",
        "import_archive_years",
    ] {
        let evidence = rows(tx, &format!("SELECT id FROM {table} LIMIT 1"), &[])?;
        if !evidence.is_empty() {
            errors.push(json!({"code":"AUDIT_WORKFLOW_NOT_PORTED","source":table}));
        }
    }
    for link in rows(
        tx,
        "SELECT r.bank_transaction_id,r.journal_entry_id,b.amount,b.amount_dkk,b.currency,a.ledger_account_no FROM bank_journal_reconciliations r JOIN bank_transactions b ON b.id=r.bank_transaction_id LEFT JOIN bank_accounts a ON a.id=b.bank_account_id",
        &[],
    )? {
        let Some(account) = link["ledger_account_no"].as_str() else {
            continue;
        };
        let id = link["journal_entry_id"]
            .as_i64()
            .ok_or_else(|| invalid("reconciliation journal reference"))?;
        let amount = if link["amount_dkk"].is_null() {
            &link["amount"]
        } else {
            &link["amount_dkk"]
        };
        let expected = Money::parse(&amount.to_string())?;
        let mut actual = Money::default();
        let mut q=tx.prepare("SELECT l.debit_amount,l.credit_amount FROM journal_lines l JOIN accounts a ON a.id=l.account_id WHERE l.journal_entry_id=?1 AND a.account_no=?2").map_err(invalid)?;
        let mut rs = q.query(rusqlite::params![id, account]).map_err(invalid)?;
        while let Some(row) = rs.next().map_err(invalid)? {
            actual.add(&legacy_money(row.get_ref(0).map_err(invalid)?)?);
            actual = actual.subtract(&legacy_money(row.get_ref(1).map_err(invalid)?)?);
        }
        if expected != actual {
            errors.push(json!({"code":"BANK_LEDGER_SOURCE_MISMATCH","bankTransactionId":link["bank_transaction_id"]}));
        }
    }
    let mut q = tx
        .prepare("SELECT id,reversal_of_entry_id FROM journal_entries ORDER BY id")
        .map_err(invalid)?;
    let mut rs = q.query([]).map_err(invalid)?;
    while let Some(r) = rs.next().map_err(invalid)? {
        let id: i64 = r.get(0).map_err(invalid)?;
        let reversal: Option<i64> = r.get(1).map_err(invalid)?;
        let mut lines=tx.prepare("SELECT account_id,debit_amount,credit_amount,vat_code FROM journal_lines WHERE journal_entry_id=?1 ORDER BY id").map_err(invalid)?;
        let mut source = lines.query([id]).map_err(invalid)?;
        let mut sum = Money::default();
        let mut actual = vec![];
        while let Some(line) = source.next().map_err(invalid)? {
            let debit = legacy_money(line.get_ref(1).map_err(invalid)?)?;
            let credit = legacy_money(line.get_ref(2).map_err(invalid)?)?;
            sum.add(&debit);
            sum = sum.subtract(&credit);
            actual.push((
                line.get::<_, i64>(0).map_err(invalid)?,
                debit,
                credit,
                line.get::<_, Option<String>>(3).map_err(invalid)?,
            ));
        }
        if sum != Money::default() || actual.len() < 2 {
            errors.push(json!({"code":"UNBALANCED_OR_EMPTY_JOURNAL","entryId":id}));
        }
        drop(source);
        if let Some(original) = reversal {
            let mut source = lines.query([original]).map_err(invalid)?;
            let mut expected = vec![];
            while let Some(line) = source.next().map_err(invalid)? {
                expected.push((
                    line.get::<_, i64>(0).map_err(invalid)?,
                    legacy_money(line.get_ref(2).map_err(invalid)?)?,
                    legacy_money(line.get_ref(1).map_err(invalid)?)?,
                    line.get::<_, Option<String>>(3).map_err(invalid)?,
                ));
            }
            if actual != expected {
                errors.push(json!({"code":"REVERSAL_NOT_EXACT_INVERSE","entryId":id}));
            }
        }
    }
    // Protected issued sequences detect valid-looking truncation of the journal tail.
    for sequence in rows(
        tx,
        "SELECT scope,value FROM sequences WHERE kind='journal_entry'",
        &[],
    )? {
        let scope = sequence["scope"]
            .as_str()
            .ok_or_else(|| invalid("journal sequence scope"))?;
        let fiscal = scope.rsplit(':').next().unwrap();
        let prefix = format!("{fiscal}-");
        let (count,maximum):(i64,i64)=tx.query_row("SELECT count(*),COALESCE(MAX(CAST(substr(entry_no,?1+1) AS INTEGER)),0) FROM journal_entries WHERE substr(entry_no,1,?1)=?2 AND length(entry_no)>?1 AND substr(entry_no,?1+1) NOT GLOB '*[^0-9]*'",rusqlite::params![prefix.len() as i64,prefix],|r|Ok((r.get(0)?,r.get(1)?))).map_err(invalid)?;
        let issued = sequence["value"]
            .as_i64()
            .ok_or_else(|| invalid("journal sequence value"))?;
        if maximum < issued || count < issued {
            errors.push(json!({"code":"JOURNAL_SEQUENCE_TRUNCATED","scope":scope}));
        }
    }
    Ok(errors)
}
pub(super) fn log_integrity(tx: &Transaction<'_>) -> Result<Vec<Value>, Error> {
    let (count, min, max): (i64, Option<i64>, Option<i64>) = tx
        .query_row("SELECT count(*),min(id),max(id) FROM audit_log", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .map_err(invalid)?;
    Ok(
        if min
            .zip(max)
            .is_some_and(|(min, max)| max - min + 1 != count)
        {
            vec![json!({"error":"audit log id sequence has gaps"})]
        } else {
            vec![]
        },
    )
}
