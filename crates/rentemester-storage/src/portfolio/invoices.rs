//! Dated invoice principal from explicit linked effects, never today's invoice-list total.
use super::*;
pub(super) fn snapshot(tx: &Transaction<'_>, as_of: &str) -> Result<Value, Error> {
    Date::parse(as_of)?;
    let functional: String = tx
        .query_row("SELECT currency FROM companies WHERE id=1", [], |r| {
            r.get(0)
        })
        .map_err(invalid)?;
    if functional != "DKK" {
        return Err(invalid(
            "dated invoices require verified DKK functional currency",
        ));
    }
    let mut q=tx.prepare("SELECT id,amount_inc_vat,payload_json,invoice_no FROM documents WHERE document_type='issued_invoice' AND invoice_date<=?1 ORDER BY id").map_err(invalid)?;
    let mut rows = q.query([as_of]).map_err(invalid)?;
    let mut projected = vec![];
    let mut oldest = 0;
    let mut count = 0;
    let mut overdue = 0;
    let mut total = Money::default();
    while let Some(row) = rows.next().map_err(invalid)? {
        let id: i64 = row.get(0).map_err(invalid)?;
        let payload: Value =
            serde_json::from_str(&row.get::<_, String>(2).map_err(invalid)?).map_err(invalid)?;
        let principal = crate::invoice_fx::dated_principal(tx, id, as_of)?;
        let open = principal.open;
        if !open.is_negative() && open != Money::default() {
            count += 1;
            total.add(&principal.carrying_dkk);
            let due = payload["dueDate"].as_str();
            let days = if let Some(due) = due {
                Date::parse(due)?;
                (vat_period::date(as_of)? - vat_period::date(due)?)
                    .whole_days()
                    .max(0)
            } else {
                0
            };
            if days > 0 {
                overdue += 1;
                oldest = oldest.max(days);
            }
            projected.push(json!({"documentId":id,"invoiceNo":row.get::<_,Option<String>>(3).map_err(invalid)?,"buyerName":payload["buyer"]["name"],"dueDate":due,"currency":principal.currency,"openBalance":wire(&open)?,"openBalanceDkk":wire(&principal.carrying_dkk)?,"overdueDays":days}));
        }
    }
    projected.sort_by(|a, b| {
        a["dueDate"]
            .as_str()
            .unwrap_or("9999")
            .cmp(b["dueDate"].as_str().unwrap_or("9999"))
            .then(a["invoiceNo"].as_str().cmp(&b["invoiceNo"].as_str()))
    });
    Ok(
        json!({"openInvoiceCount":count,"openInvoiceTotal":wire(&total)?,"overdueInvoiceCount":overdue,"oldestOverdueDays":oldest,"rows":projected}),
    )
}

pub(super) fn collect(tx: &Transaction<'_>, as_of: &str) -> Result<Value, Error> {
    let snapshot = snapshot(tx, as_of)?;
    Ok(
        json!({"openInvoiceCount":snapshot["openInvoiceCount"],"openInvoiceTotal":snapshot["openInvoiceTotal"],"overdueInvoiceCount":snapshot["overdueInvoiceCount"]}),
    )
}
