use crate::{invalid, legacy_money};
use rentemester_engine::{
    Error,
    money::Money,
    sales_invoice::{refused, wire},
};
use rusqlite::{ToSql, Transaction, params, types::ValueRef};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
pub(super) fn rows(
    tx: &Transaction<'_>,
    sql: &str,
    params: &[&dyn ToSql],
) -> Result<Vec<Value>, Error> {
    let mut q = tx.prepare(sql).map_err(invalid)?;
    let columns = q
        .column_names()
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let mut source = q.query(params).map_err(invalid)?;
    let mut values = vec![];
    while let Some(row) = source.next().map_err(invalid)? {
        let mut object = serde_json::Map::new();
        for (i, name) in columns.iter().enumerate() {
            let value = match row.get_ref(i).map_err(invalid)? {
                ValueRef::Null => Value::Null,
                ValueRef::Integer(v) => json!(v),
                ValueRef::Real(v) if v.is_finite() => json!(v),
                ValueRef::Text(v) => json!(std::str::from_utf8(v).map_err(invalid)?),
                _ => return Err(invalid("unsupported readiness source value")),
            };
            object.insert(name.clone(), value);
        }
        values.push(Value::Object(object));
    }
    Ok(values)
}
fn digest(value: &Value) -> String {
    format!("{:x}", Sha256::digest(value.to_string().as_bytes()))
}
fn item(code: &str, result: Result<Vec<Value>, Error>, warning: bool, waivable: bool) -> Value {
    match result {
        Ok(mut evidence) => {
            evidence.sort_by_cached_key(Value::to_string);
            json!({"code":code,"status":if evidence.is_empty(){"passed"}else if warning{"warning"}else{"blocked"},"waivable":waivable,"count":evidence.len(),"amount":0,"sourceHash":digest(&json!(evidence)),"evidence":evidence})
        }
        Err(error) => {
            json!({"code":code,"status":"unavailable","waivable":false,"count":1,"amount":0,"sourceHash":digest(&json!({"error":error.code})),"evidence":[{"error":error.message}]})
        }
    }
}
fn control_balance(tx: &Transaction<'_>, account: &str, end: &str) -> Result<Money, Error> {
    let mut total = Money::default();
    let mut q=tx.prepare("SELECT l.debit_amount,l.credit_amount FROM journal_lines l JOIN journal_entries j ON j.id=l.journal_entry_id JOIN accounts a ON a.id=l.account_id WHERE a.account_no=?1 AND j.transaction_date<=?2").map_err(invalid)?;
    let mut rs = q.query(params![account, end]).map_err(invalid)?;
    while let Some(r) = rs.next().map_err(invalid)? {
        total.add(&legacy_money(r.get_ref(0).map_err(invalid)?)?);
        total = total.subtract(&legacy_money(r.get_ref(1).map_err(invalid)?)?);
    }
    Ok(total)
}
fn dkk_controls(tx: &Transaction<'_>, end: &str) -> Result<Vec<Value>, Error> {
    let mut evidence = vec![];
    let mut source = std::collections::BTreeMap::new();
    for role in ["bank", "debtors", "creditors"] {
        let matches = rows(
            tx,
            "SELECT m.account_no,a.type,a.normal_balance FROM account_role_mappings m JOIN accounts a ON a.account_no=m.account_no WHERE m.role=?1 AND m.status='confirmed' AND a.active=1",
            &[&role],
        )?;
        if matches.len() != 1 {
            return Err(refused(format!(
                "confirmed unique {role} account required for close control"
            )));
        }
        let account = matches[0]["account_no"].as_str().unwrap().to_owned();
        if matches[0]["type"]
            != if role == "creditors" {
                "liability"
            } else {
                "asset"
            }
            || matches[0]["normal_balance"]
                != if role == "creditors" {
                    "credit"
                } else {
                    "debit"
                }
        {
            return Err(refused("incompatible DKK control-account mapping"));
        }
        source.insert(role, (account, Money::default()));
    }
    let statement = crate::bank::statement_balance(tx, end)?;
    source.get_mut("bank").unwrap().1 = if let Some(value) = statement.balance {
        value
    } else {
        let any: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM bank_transactions WHERE transaction_date<=?1)",
                [end],
                |r| r.get(0),
            )
            .map_err(invalid)?;
        if any {
            return Err(refused(statement.problem.unwrap_or_else(|| {
                "independent bank statement balance required".into()
            })));
        }
        Money::default()
    };
    // Imported open items require their provenance/valuation port; do not infer them from totals.
    if !rows(
        tx,
        "SELECT id FROM imported_receivable_headers LIMIT 1",
        &[],
    )?
    .is_empty()
    {
        return Err(refused(
            "historical import control evidence is not ported for native period close",
        ));
    }
    let invoices = rows(
        tx,
        "SELECT d.id,p.booked_gross_dkk FROM documents d JOIN issued_invoice_postings p ON p.invoice_document_id=d.id JOIN accounts a ON a.id=p.receivable_account_id WHERE d.invoice_date<=?1 AND a.account_no=?2",
        &[&end, &source["debtors"].0],
    )?;
    for invoice in invoices {
        let id = invoice["id"]
            .as_i64()
            .ok_or_else(|| invalid("invoice reference"))?;
        let mut amount = Money::parse(&invoice["booked_gross_dkk"].to_string())?;
        for (sql, sign) in [
            (
                "SELECT amount FROM invoice_payments WHERE invoice_document_id=?1 AND payment_date<=?2",
                -1,
            ),
            (
                "SELECT amount FROM invoice_refunds WHERE invoice_document_id=?1 AND refund_date<=?2",
                1,
            ),
        ] {
            let mut q = tx.prepare(sql).map_err(invalid)?;
            let mut rs = q.query(params![id, end]).map_err(invalid)?;
            while let Some(r) = rs.next().map_err(invalid)? {
                let value = legacy_money(r.get_ref(0).map_err(invalid)?)?;
                if sign > 0 {
                    amount.add(&value)
                } else {
                    amount = amount.subtract(&value)
                }
            }
        }
        let mut q=tx.prepare("SELECT p.booked_gross_dkk FROM credit_note_postings p JOIN documents d ON d.id=p.credit_note_document_id JOIN journal_entries j ON j.id=p.journal_entry_id WHERE p.original_invoice_document_id=?1 AND j.transaction_date<=?2").map_err(invalid)?;
        let mut rs = q.query(params![id, end]).map_err(invalid)?;
        while let Some(r) = rs.next().map_err(invalid)? {
            amount = amount.subtract(&legacy_money(r.get_ref(0).map_err(invalid)?)?);
        }
        source.get_mut("debtors").unwrap().1.add(&amount);
    }
    let bills = crate::payables::list(tx, "close-control", end, "all")?;
    // A payable is tied to its original liability account, not today's role.
    for bill in bills["rows"].as_array().unwrap() {
        let id = bill["payableId"].as_i64().unwrap();
        let uses:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM payables p JOIN journal_lines l ON l.journal_entry_id=p.journal_entry_id JOIN accounts a ON a.id=l.account_id WHERE p.id=?1 AND a.account_no=?2 AND l.credit_amount=p.gross_amount)",params![id,source["creditors"].0],|r|r.get(0)).map_err(invalid)?;
        if uses {
            source.get_mut("creditors").unwrap().1 = source["creditors"]
                .1
                .subtract(&Money::parse(&bill["openBalance"].to_string())?);
        }
    }
    for (role, (account, expected)) in source {
        let ledger = control_balance(tx, &account, end)?;
        if ledger != expected {
            evidence.push(json!({"role":role,"accountNo":account,"ledgerBalanceDkk":wire(&ledger)?,"sourceBalanceDkk":wire(&expected)?,"differenceDkk":wire(&ledger.subtract(&expected))?}));
        }
    }
    Ok(evidence)
}
pub fn compute(
    tx: &Transaction<'_>,
    start: &str,
    end: &str,
    file_integrity: Result<Vec<Value>, Error>,
) -> Result<Value, Error> {
    super::bounds(start, end)?;
    let mut items = vec![];
    let lifecycle = (|| {
        let mut result = vec![];
        for row in rows(
            tx,
            "SELECT id,period_start,period_end,status,kind FROM accounting_periods WHERE NOT(period_end<?1 OR period_start>?2) ORDER BY id",
            &[&start, &end],
        )? {
            if super::effective(
                tx,
                row["id"].as_i64().unwrap(),
                row["status"].as_str().unwrap().into(),
            )? != "open"
            {
                result.push(row);
            }
        }
        Ok(result)
    })();
    items.push(item("PERIOD_LIFECYCLE", lifecycle, false, false));
    for (code, sql, params, warning, waivable) in [
        (
            "BANK_UNRECONCILED",
            "SELECT b.id,b.transaction_date,b.amount,b.currency FROM bank_transactions b LEFT JOIN bank_journal_reconciliations r ON r.bank_transaction_id=b.id WHERE r.journal_entry_id IS NULL AND (b.transaction_date BETWEEN ?1 AND ?2 OR b.transaction_date IS NULL) ORDER BY b.id",
            vec![&start as &dyn ToSql, &end],
            false,
            true,
        ),
        (
            "EXCEPTIONS_OPEN",
            "SELECT e.id,e.type,e.severity FROM exceptions e LEFT JOIN bank_transactions b ON b.id=e.related_bank_transaction_id LEFT JOIN documents d ON d.id=e.related_document_id WHERE e.status='open' AND e.severity IN ('high','medium') AND (b.transaction_date BETWEEN ?1 AND ?2 OR d.invoice_date BETWEEN ?1 AND ?2) ORDER BY e.id",
            vec![&start as &dyn ToSql, &end],
            false,
            true,
        ),
        (
            "EXCEPTION_SCOPE_UNKNOWN",
            "SELECT e.id,e.type,e.severity FROM exceptions e LEFT JOIN bank_transactions b ON b.id=e.related_bank_transaction_id LEFT JOIN documents d ON d.id=e.related_document_id WHERE e.status='open' AND e.severity IN ('high','medium') AND b.transaction_date IS NULL AND d.invoice_date IS NULL ORDER BY e.id",
            vec![],
            false,
            false,
        ),
        (
            "BATCH_UNPOSTED_OR_FAILED",
            "SELECT r.id AS run_id,rev.id AS revision_id,a.id AS attempt_id,(SELECT e.event_type FROM bookkeeping_batch_apply_events_v2 e WHERE e.apply_attempt_id=a.id ORDER BY e.id DESC LIMIT 1) AS final_event FROM bookkeeping_batch_runs r LEFT JOIN bookkeeping_batch_revisions rev ON rev.id=(SELECT id FROM bookkeeping_batch_revisions WHERE run_id=r.id ORDER BY id DESC LIMIT 1) LEFT JOIN bookkeeping_batch_apply_attempts_v2 a ON a.id=(SELECT id FROM bookkeeping_batch_apply_attempts_v2 WHERE revision_id=rev.id ORDER BY id DESC LIMIT 1) WHERE NOT(r.accounting_to<?1 OR r.accounting_from>?2) AND (a.id IS NULL OR final_event IS NULL OR final_event!='completed' OR EXISTS(SELECT 1 FROM bookkeeping_batch_apply_events_v2 e WHERE e.apply_attempt_id=a.id AND e.event_type IN ('item_failed','source_stale')) OR EXISTS(SELECT 1 FROM bookkeeping_batch_final_checks_v2 c WHERE c.apply_attempt_id=a.id AND c.ok=0)) ORDER BY r.id",
            vec![&start as &dyn ToSql, &end],
            false,
            true,
        ),
        (
            "DOCUMENT_OUTSTANDING",
            "SELECT d.id,d.status,d.invoice_date FROM documents d WHERE (d.status IN ('pending','failed','needs_review') OR (d.document_type IN ('purchase_sale','cash_register_receipt','issued_invoice') AND d.amount_inc_vat>0 AND NOT EXISTS(SELECT 1 FROM journal_entries j WHERE j.document_id=d.id))) AND (d.invoice_date BETWEEN ?1 AND ?2 OR d.invoice_date IS NULL) ORDER BY d.id",
            vec![&start as &dyn ToSql, &end],
            false,
            true,
        ),
        (
            "PAYABLE_OUTSTANDING",
            "SELECT p.id,p.due_date,p.gross_amount,COALESCE(SUM(pp.amount),0) AS paid_amount FROM payables p LEFT JOIN payable_payments pp ON pp.payable_id=p.id AND pp.payment_date<=?1 WHERE p.due_date<=?1 GROUP BY p.id HAVING COALESCE(SUM(pp.amount),0)<p.gross_amount ORDER BY p.id",
            vec![&end as &dyn ToSql],
            true,
            true,
        ),
        (
            "RECEIVABLE_OUTSTANDING",
            "SELECT d.id,d.invoice_date,d.amount_inc_vat,COALESCE(SUM(p.amount),0) AS paid_amount FROM documents d LEFT JOIN invoice_payments p ON p.invoice_document_id=d.id AND p.payment_date<=?1 WHERE d.document_type='issued_invoice' AND d.invoice_date<=?1 GROUP BY d.id HAVING COALESCE(SUM(p.amount),0)<COALESCE(d.amount_inc_vat,0) ORDER BY d.id",
            vec![&end as &dyn ToSql],
            true,
            true,
        ),
    ] {
        items.push(item(code, rows(tx, sql, &params), warning, waivable));
    }
    items.push(item(
        "DKK_CONTROL_ACCOUNTS",
        dkk_controls(tx, end),
        false,
        false,
    ));
    items.push(item(
        "LEDGER_AUDIT_CHAIN",
        super::audit::verify(tx),
        false,
        false,
    ));
    items.push(item(
        "DOCUMENT_ORIGINAL_INTEGRITY",
        file_integrity,
        false,
        false,
    ));
    items.push(item(
        "AUDIT_LOG_INTEGRITY",
        super::audit::log_integrity(tx),
        false,
        false,
    ));
    let trial = (|| {
        let report = crate::cockpit::read(tx, "close-control", end, None, None)?;
        Ok(if report.trial_balance["balanced"] == true {
            vec![]
        } else {
            vec![json!({"error":"unbalanced trial balance"})]
        })
    })();
    items.push(item("TRIAL_BALANCE", trial, false, false));
    let vat = (|| {
        let report = crate::vat_report::range(tx, start, end)?;
        Ok(report["errors"]
            .as_array()
            .ok_or_else(|| invalid("VAT errors"))?
            .iter()
            .map(|e| json!({"error":e}))
            .collect())
    })();
    items.push(item("VAT_PREFLIGHT", vat, false, false));
    let integrity = (|| {
        let rs = rows(tx, "PRAGMA integrity_check", &[])?;
        Ok(if rs.len() == 1 && rs[0]["integrity_check"] == "ok" {
            vec![]
        } else {
            rs
        })
    })();
    items.push(item("SQLITE_INTEGRITY", integrity, false, false));
    items.sort_by_key(|i| i["code"].as_str().unwrap().to_owned());
    let blockers = items
        .iter()
        .filter(|i| i["status"] == "blocked" || i["status"] == "unavailable")
        .count();
    let warnings = items.iter().filter(|i| i["status"] == "warning").count();
    // Bind a review to the real source revision, including balanced new postings
    // which do not change any warning/control status.
    let revision = json!({"journal":rows(tx,"SELECT id,entry_hash FROM journal_entries ORDER BY id DESC LIMIT 1",&[])?,"journalCount":rows(tx,"SELECT count(*) AS n FROM journal_entries",&[])?,"audit":rows(tx,"SELECT count(*) AS n,max(id) AS head FROM audit_log",&[])?});
    let mut packet = json!({"version":4,"format":"rentemester.native-period-close.v1","periodStart":start,"periodEnd":end,"cutoff":end,"controlsRun":items.iter().map(|i|&i["code"]).collect::<Vec<_>>(),"items":items,"blockers":blockers,"warnings":warnings,"sourceRevision":revision});
    packet["hash"] = json!(digest(&packet));
    Ok(packet)
}
