//! Dated invoice principal from explicit linked effects, never today's invoice-list total.
use super::*;
pub(super) fn collect(tx: &Transaction<'_>, as_of: &str) -> Result<Value, Error> {
    let unsupported:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM imported_receivable_headers) OR EXISTS(SELECT 1 FROM invoice_bad_debt_writeoffs) OR EXISTS(SELECT 1 FROM documents WHERE document_type='issued_invoice' AND invoice_date<=?1 AND currency!='DKK')",[as_of],|r|r.get(0)).map_err(invalid)?;
    if unsupported {
        return Ok(
            json!({"openInvoiceCount":null,"openInvoiceTotal":null,"overdueInvoiceCount":null}),
        );
    }
    let mut q=tx.prepare("SELECT id,amount_inc_vat,payload_json FROM documents WHERE document_type='issued_invoice' AND invoice_date<=?1 ORDER BY id").map_err(invalid)?;
    let mut rows = q.query([as_of]).map_err(invalid)?;
    let mut count = 0;
    let mut overdue = 0;
    let mut total = Money::default();
    while let Some(row) = rows.next().map_err(invalid)? {
        let id: i64 = row.get(0).map_err(invalid)?;
        let gross = legacy_money(row.get_ref(1).map_err(invalid)?)?;
        let payload: Value =
            serde_json::from_str(&row.get::<_, String>(2).map_err(invalid)?).map_err(invalid)?;
        crate::credit_note::credited(tx, id)?;
        crate::invoice_settlement::refunded(tx, id)?;
        let mut open = gross;
        for (sql, refund) in [
            (
                "SELECT p.amount,p.currency,j.status,EXISTS(SELECT 1 FROM journal_entries r WHERE r.reversal_of_entry_id=j.id) FROM invoice_payments p JOIN journal_entries j ON j.id=p.journal_entry_id WHERE p.invoice_document_id=?1 AND p.payment_date<=?2",
                false,
            ),
            (
                "SELECT p.booked_gross_dkk,c.currency,j.status,EXISTS(SELECT 1 FROM journal_entries r WHERE r.reversal_of_entry_id=j.id) FROM credit_note_postings p JOIN documents c ON c.id=p.credit_note_document_id JOIN journal_entries j ON j.id=p.journal_entry_id WHERE p.original_invoice_document_id=?1 AND j.transaction_date<=?2",
                false,
            ),
            (
                "SELECT p.amount,p.currency,j.status,EXISTS(SELECT 1 FROM journal_entries r WHERE r.reversal_of_entry_id=j.id) FROM invoice_refunds p JOIN journal_entries j ON j.id=p.journal_entry_id WHERE p.invoice_document_id=?1 AND p.refund_date<=?2",
                true,
            ),
        ] {
            let mut statement = tx.prepare(sql).map_err(invalid)?;
            let mut effects = statement.query(params![id, as_of]).map_err(invalid)?;
            while let Some(effect) = effects.next().map_err(invalid)? {
                if effect.get::<_, String>(1).map_err(invalid)? != "DKK"
                    || effect.get::<_, String>(2).map_err(invalid)? != "posted"
                    || effect.get::<_, bool>(3).map_err(invalid)?
                {
                    return Err(invalid("dated invoice effect requires resolution"));
                }
                let amount = legacy_money(effect.get_ref(0).map_err(invalid)?)?;
                if refund {
                    open.add(&amount)
                } else {
                    open = open.subtract(&amount)
                }
            }
        }
        if !open.is_negative() && open != Money::default() {
            count += 1;
            total.add(&open);
            if let Some(due) = payload["dueDate"].as_str() {
                Date::parse(due)?;
                if due < as_of {
                    overdue += 1;
                }
            }
        }
    }
    Ok(
        json!({"openInvoiceCount":count,"openInvoiceTotal":wire(&total)?,"overdueInvoiceCount":overdue}),
    )
}
