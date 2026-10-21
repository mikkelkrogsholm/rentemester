//! External source records. These are never native issued invoices or permission to collect twice.
use crate::{invalid, legacy_money};
use rentemester_engine::{
    Error, fiscal_year::Date, imported_receivables::ValidatedSchedule, money::Money,
    sales_invoice::wire,
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde_json::{Value, json};
use std::collections::BTreeMap;
fn refused(message: impl Into<String>) -> Error {
    Error::new("imported_receivables_refused", message)
}
/// Only the source import transaction calls this, after establishing its accepted
/// provenance attempt and validating actual ledger/source control balances.
pub(crate) fn persist(
    tx: &Transaction<'_>,
    attempt: i64,
    schedule: &ValidatedSchedule,
) -> Result<(), Error> {
    crate::journal_post::require_current(tx)?;
    let accepted:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM dinero_import_attempts WHERE id=?1 AND outcome='accepted' AND parser_contract LIKE 'rentemester-native-dinero/%')",[attempt],|r|r.get(0)).map_err(invalid)?;
    if !accepted {
        return Err(refused("native accepted source attempt required"));
    }
    let prior:Option<String>=tx.query_row("SELECT schedule_hash FROM imported_receivable_headers WHERE dinero_import_attempt_id=?1 LIMIT 1",[attempt],|r|r.get(0)).optional().map_err(invalid)?;
    if prior.is_some() {
        return verify_schedule(tx, attempt, schedule);
    }
    for invoice in &schedule.invoices {
        let id:i64=tx.query_row("INSERT INTO imported_receivable_headers(dinero_import_attempt_id,external_invoice_id,source_document_hash,customer_external_id,customer_name,invoice_date,due_date,gross_amount,control_account_no,source_recognition_ref,schedule_hash) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11) RETURNING id",params![attempt,invoice.id,invoice.document_hash,invoice.customer_id,invoice.customer_name,invoice.invoice_date.to_string(),invoice.due_date.map(|d|d.to_string()),crate::journal_post::money(&invoice.gross_amount)?,invoice.control_account_no,invoice.recognition_ref,schedule.hash],|r|r.get(0)).map_err(invalid)?;
        for event in &invoice.events {
            tx.execute("INSERT INTO imported_receivable_events(receivable_id,external_event_id,event_kind,effective_date,amount,source_event_ref,source_document_hash,schedule_hash) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![id,event.id,event.event_kind,event.payment_date.to_string(),crate::journal_post::money(&event.amount)?,event.payment_ref,event.document_hash,schedule.hash]).map_err(invalid)?;
        }
    }
    verify_schedule(tx, attempt, schedule)
}
/// Exact row/order/value match to the original parsed schedule, not just its
/// declared hash. This is complementary to independent physical-source admission.
pub(crate) fn verify_schedule(
    tx: &Connection,
    attempt: i64,
    schedule: &ValidatedSchedule,
) -> Result<(), Error> {
    let mut q=tx.prepare("SELECT id,external_invoice_id,source_document_hash,customer_external_id,customer_name,invoice_date,due_date,gross_amount,control_account_no,source_recognition_ref,schedule_hash FROM imported_receivable_headers WHERE dinero_import_attempt_id=?1 ORDER BY id").map_err(invalid)?;
    let mut rs = q.query([attempt]).map_err(invalid)?;
    for invoice in &schedule.invoices {
        let r = rs
            .next()
            .map_err(invalid)?
            .ok_or_else(|| refused("accepted schedule header missing"))?;
        let actual = json!([
            r.get::<_, String>(1).map_err(invalid)?,
            r.get::<_, String>(2).map_err(invalid)?,
            r.get::<_, Option<String>>(3).map_err(invalid)?,
            r.get::<_, Option<String>>(4).map_err(invalid)?,
            r.get::<_, String>(5).map_err(invalid)?,
            r.get::<_, Option<String>>(6).map_err(invalid)?,
            legacy_money(r.get_ref(7).map_err(invalid)?)?.decimal(),
            r.get::<_, String>(8).map_err(invalid)?,
            r.get::<_, String>(9).map_err(invalid)?,
            r.get::<_, String>(10).map_err(invalid)?
        ]);
        let expected = json!([
            invoice.id,
            invoice.document_hash,
            invoice.customer_id,
            invoice.customer_name,
            invoice.invoice_date.to_string(),
            invoice.due_date.map(|d| d.to_string()),
            invoice.gross_amount.decimal(),
            invoice.control_account_no,
            invoice.recognition_ref,
            schedule.hash
        ]);
        if actual != expected {
            return Err(refused(
                "accepted schedule header differs from retained source",
            ));
        }
        let id: i64 = r.get(0).map_err(invalid)?;
        let mut eq=tx.prepare("SELECT external_event_id,event_kind,effective_date,amount,source_event_ref,source_document_hash,schedule_hash FROM imported_receivable_events WHERE receivable_id=?1 ORDER BY id").map_err(invalid)?;
        let mut events = eq.query([id]).map_err(invalid)?;
        for event in &invoice.events {
            let r = events
                .next()
                .map_err(invalid)?
                .ok_or_else(|| refused("accepted schedule event missing"))?;
            let actual = json!([
                r.get::<_, String>(0).map_err(invalid)?,
                r.get::<_, String>(1).map_err(invalid)?,
                r.get::<_, String>(2).map_err(invalid)?,
                legacy_money(r.get_ref(3).map_err(invalid)?)?.decimal(),
                r.get::<_, String>(4).map_err(invalid)?,
                r.get::<_, String>(5).map_err(invalid)?,
                r.get::<_, String>(6).map_err(invalid)?
            ]);
            let expected = json!([
                event.id,
                event.event_kind,
                event.payment_date.to_string(),
                event.amount.decimal(),
                event.payment_ref,
                event.document_hash,
                schedule.hash
            ]);
            if actual != expected {
                return Err(refused(
                    "accepted schedule event differs from retained source",
                ));
            }
        }
        if events.next().map_err(invalid)?.is_some() {
            return Err(refused("unexpected accepted schedule event"));
        }
    }
    if rs.next().map_err(invalid)?.is_some() {
        return Err(refused("unexpected accepted schedule invoice"));
    }
    Ok(())
}
pub fn read(tx: &Transaction<'_>, as_of: &str) -> Result<Value, Error> {
    let date = Date::parse(as_of)?;
    let headers = crate::audit_invoice::imported_rows(tx)?;
    let mut out = vec![];
    let mut total = Money::default();
    for h in headers {
        let invoice_date = Date::parse(
            h["invoice_date"]
                .as_str()
                .ok_or_else(|| refused("invoice date unavailable"))?,
        )?;
        if invoice_date > date {
            continue;
        }
        let id = h["id"]
            .as_i64()
            .ok_or_else(|| refused("invoice identity unavailable"))?;
        let gross = crate::invoice_fx::amount(&h["gross_amount"])?;
        let mut paid = Money::default();
        let mut q=tx.prepare("SELECT amount FROM imported_receivable_events WHERE receivable_id=?1 AND effective_date<=?2 ORDER BY id").map_err(invalid)?;
        let mut events = q.query(params![id, as_of]).map_err(invalid)?;
        while let Some(r) = events.next().map_err(invalid)? {
            paid.add(&legacy_money(r.get_ref(0).map_err(invalid)?)?);
        }
        let open = gross.subtract(&paid);
        if open.is_negative() {
            return Err(refused("imported payments exceed original gross"));
        }
        total.add(&open);
        out.push(json!({"source":"imported","externalInvoiceId":h["external_invoice_id"],"customerExternalId":h["customer_external_id"],"customerName":h["customer_name"],"invoiceDate":h["invoice_date"],"dueDate":h["due_date"],"grossAmount":wire(&gross)?,"paidAmount":wire(&paid)?,"openBalance":wire(&open)?,"controlAccountNo":h["control_account_no"],"sourceRecognitionRef":h["source_recognition_ref"],"sourceDocumentHash":h["source_document_hash"],"scheduleHash":h["schedule_hash"],"archiveBoundary":"Imported source record; use invoice list for Rentemester-issued invoices."}));
    }
    Ok(
        json!({"ok":true,"asOfDate":as_of,"boundary":"Imported source records only; native Rentemester invoices are deliberately separate to avoid duplicate claims.","count":out.len(),"totalOpen":wire(&total)?,"rows":out,"errors":[]}),
    )
}
pub(crate) fn balances(
    tx: &Transaction<'_>,
    as_of: &str,
) -> Result<BTreeMap<String, Money>, Error> {
    let list = read(tx, as_of)?;
    let mut out = BTreeMap::<String, Money>::new();
    for r in list["rows"]
        .as_array()
        .ok_or_else(|| refused("imported source rows unavailable"))?
    {
        let account = r["controlAccountNo"]
            .as_str()
            .ok_or_else(|| refused("control account unavailable"))?;
        out.entry(account.into())
            .or_default()
            .add(&crate::invoice_fx::amount(&r["openBalance"])?);
    }
    Ok(out)
}
