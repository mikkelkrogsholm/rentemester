//! Original invoice reads over current validated native histories. asOf ages only.
use crate::{invalid, legacy_money};
use rentemester_engine::{
    Error,
    fiscal_year::Date,
    money::Money,
    sales_invoice::{self, wire},
};
use rusqlite::{OptionalExtension, Transaction};
use serde_json::{Number, Value, json};
#[derive(Clone, Copy)]
pub enum Kind {
    List,
    Find,
    Status,
    Overdue,
}
#[derive(Default)]
pub struct Query {
    pub status: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub customer_cvr: Option<String>,
    pub customer: Option<String>,
    pub invoice_number: Option<String>,
    pub query: Option<String>,
    pub as_of: Option<String>,
    pub min_amount: Option<Number>,
    pub max_amount: Option<Number>,
    pub amount: Option<Number>,
    pub min_days: Option<u64>,
    pub document_id: Option<i64>,
}
fn refused(message: impl Into<String>) -> Error {
    Error::new("invoice_original_source_invalid", message)
}
fn normalize(s: Option<&str>) -> Option<String> {
    s.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}
fn folded(value: Option<&str>, needle: Option<&str>) -> bool {
    let needle = needle.unwrap_or("").trim().to_lowercase();
    value.unwrap_or("").trim().to_lowercase().contains(&needle)
}
fn envelope(mut value: Value) -> Value {
    value.as_object_mut().unwrap().remove("ok");
    value.as_object_mut().unwrap().remove("errors");
    json!({"ok":true,"data":value,"errors":[]})
}
fn rows(tx: &Transaction<'_>, sql: &str, id: i64) -> Result<Vec<Value>, Error> {
    crate::bookkeeping_batch::rows(tx, sql, [id])
}
fn peppol(tx: &Transaction<'_>, id: i64) -> Result<Value, Error> {
    let Some(value) = crate::public_einvoice::get(tx, id)? else {
        return Ok(Value::Null);
    };
    let legacy_ack: Option<String> = if value["legacy"] == true {
        tx.query_row(
            "SELECT acknowledged_at FROM peppol_submissions WHERE id=?1",
            [value["id"]
                .as_i64()
                .ok_or_else(|| refused("PEPPOL identity missing"))?],
            |r| r.get(0),
        )
        .map_err(invalid)?
    } else {
        None
    };
    Ok(peppol_summary(&value, legacy_ack.as_deref()))
}
fn peppol_summary(value: &Value, legacy_ack: Option<&str>) -> Value {
    let history = value["history"].as_array();
    let latest = history.and_then(|rows| rows.last());
    let status = if value["status"] == "acknowledged" {
        "acknowledged"
    } else if value["status"] == "failed" {
        "failed"
    } else if latest.is_some_and(|e| e["eventType"] == "delivery_reserved") {
        "in_progress"
    } else if latest.is_some_and(|e| {
        e["eventType"] == "delivery_failed" && e["status"] != "pre-acceptance-failed"
    }) {
        "uncertain"
    } else if value["transmissionId"].is_string()
        && latest
            .is_some_and(|e| matches!(e["eventType"].as_str(), Some("queued" | "status_observed")))
    {
        "queued"
    } else {
        "retryable"
    };
    let acknowledged = if status == "acknowledged" {
        history
            .and_then(|rows| rows.iter().rev().find(|e| e["eventType"] == "delivered"))
            .and_then(|e| e["at"].as_str())
            .or(legacy_ack)
    } else {
        None
    };
    json!({"status":status,"submissionReference":value["submissionReference"],"transmissionId":value["transmissionId"],"acknowledgedAt":acknowledged})
}
fn status(tx: &Transaction<'_>, id: i64, as_of: &str) -> Result<(Value, Value), Error> {
    let raw:Option<(String,String,String,String,Money)>=tx.query_row("SELECT invoice_no,invoice_date,currency,payload_json,amount_inc_vat FROM documents WHERE id=?1 AND document_type='issued_invoice'",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,legacy_money(r.get_ref(4)?).map_err(|_|rusqlite::Error::InvalidQuery)?))).optional().map_err(invalid)?;
    let (number, date, currency, raw, gross) =
        raw.ok_or_else(|| refused("issued invoice source missing"))?;
    let payload: Value = serde_json::from_str(&raw).map_err(invalid)?;
    if payload["invoiceNumber"] != number
        || payload["issueDate"] != date
        || payload["currency"] != currency
        || sales_invoice::project(&payload)?.gross != gross
    {
        return Err(refused("invoice header differs from immutable snapshot"));
    }
    let posted: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM issued_invoice_postings WHERE invoice_document_id=?1)",
            [id],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    let claims = if posted {
        crate::credit_note::verify_existing(tx, id)?;
        crate::reminder_claims::read(tx, id, as_of)?
    } else {
        let effects:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM journal_entries WHERE document_id=?1) OR EXISTS(SELECT 1 FROM invoice_payments WHERE invoice_document_id=?1) OR EXISTS(SELECT 1 FROM invoice_refunds WHERE invoice_document_id=?1) OR EXISTS(SELECT 1 FROM invoice_bad_debt_writeoffs WHERE invoice_document_id=?1) OR EXISTS(SELECT 1 FROM invoice_reminders WHERE invoice_document_id=?1) OR EXISTS(SELECT 1 FROM invoice_compensation_claims WHERE invoice_document_id=?1) OR EXISTS(SELECT 1 FROM invoice_interest_claims WHERE invoice_document_id=?1) OR EXISTS(SELECT 1 FROM invoice_interest_corrections WHERE invoice_document_id=?1) OR EXISTS(SELECT 1 FROM invoice_claim_payments WHERE invoice_document_id=?1) OR EXISTS(SELECT 1 FROM credit_note_postings WHERE original_invoice_document_id=?1) OR EXISTS(SELECT 1 FROM documents c WHERE c.document_type='credit_note' AND c.payment_details=?2)",rusqlite::params![id,number],|r|r.get(0)).map_err(invalid)?;
        if effects {
            return Err(refused("unposted invoice has unexplained related effects"));
        }
        json!({"principalOpenBalance":wire(&gross)?,"claimOpenBalance":if currency=="DKK"{wire(&gross)?}else{Value::Null},"registeredClaimBalanceDkk":0,"totalReminderFees":0,"totalCompensationDkk":0,"totalInterestDkk":0,"totalInterestCorrectionsDkk":0,"totalClaimPayments":0})
    };
    let p = crate::sales::project_invoice(tx, id, &payload, &gross)?;
    if claims["principalOpenBalance"] != wire(&p.open)? {
        return Err(refused(
            "current principal projection differs from validated ledger history",
        ));
    }
    let due = rentemester_engine::reminder_claims::effective_due_date(
        &date,
        payload["dueDate"].as_str(),
    )?;
    let days = if p.open.is_negative() || p.open == Money::default() {
        0
    } else {
        (rentemester_engine::vat_period::date(as_of)? - rentemester_engine::vat_period::date(&due)?)
            .whole_days()
            .max(0)
    };
    let state = if p.open.is_negative() {
        "overpaid"
    } else if p.open != Money::default() {
        "open"
    } else if p.written.gross != Money::default() {
        "written_off"
    } else if p.refunds != Money::default() {
        "refunded"
    } else if p.credited == gross && p.paid == Money::default() {
        "credited"
    } else {
        "paid"
    };
    let customer = normalize(payload["buyer"]["name"].as_str());
    let cvr = normalize(payload["buyer"]["vatOrCvr"].as_str()).map(|s| s.to_uppercase());
    let ean = sales_invoice::recipient_ean(payload["buyer"]["eanNumber"].as_str())?;
    let row = json!({"documentId":id,"invoiceNumber":number,"invoiceDate":date,"customerName":customer,"customerCvr":cvr,"buyerEanNumber":ean,"buyerPublicRecipient":payload["buyer"]["publicRecipient"]==true||ean.is_some(),"peppolStatus":peppol(tx,id)?,"grossAmount":wire(&gross)?,"currency":currency,"openBalance":wire(&p.open)?,"claimOpenBalance":claims["claimOpenBalance"],"status":state,"effectiveDueDate":due,"isOverdue":days>0,"overdueDays":days});
    let interest_net = Money::parse(&claims["totalInterestDkk"].to_string())?.subtract(
        &Money::parse(&claims["totalInterestCorrectionsDkk"].to_string())?,
    );
    let mut view = json!({"invoiceDocumentId":id,"invoiceNumber":number,"grossAmount":wire(&gross)?,"creditedAmount":wire(&p.credited)?,"paidAmount":wire(&p.paid)?,"openBalance":wire(&p.open)?,"claimOpenBalance":claims["claimOpenBalance"],"currency":currency,"principalCurrency":currency,"claimCurrency":"DKK","registeredClaimBalanceDkk":claims["registeredClaimBalanceDkk"],"asOfDate":as_of,"effectiveDueDate":due,"isOverdue":days>0,"overdueDays":days,"status":state,
 "payments":rows(tx,"SELECT id AS paymentId,payment_date AS paymentDate,amount,bank_transaction_id AS bankTransactionId,journal_entry_id AS journalEntryId,note FROM invoice_payments WHERE invoice_document_id=?1 ORDER BY id",id)?,
 "creditNotes":rows(tx,"SELECT c.id AS documentId,c.invoice_no AS creditNoteNumber,c.amount_inc_vat AS amount,c.invoice_date AS issueDate FROM credit_note_postings p JOIN documents c ON c.id=p.credit_note_document_id WHERE p.original_invoice_document_id=?1 ORDER BY c.id",id)?,
 "refunds":rows(tx,"SELECT id AS refundId,refund_date AS refundDate,amount,bank_transaction_id AS bankTransactionId,journal_entry_id AS journalEntryId,note FROM invoice_refunds WHERE invoice_document_id=?1 ORDER BY id",id)?,
 "claimPayments":rows(tx,"SELECT id AS claimPaymentId,payment_date AS paymentDate,amount,bank_transaction_id AS bankTransactionId,journal_entry_id AS journalEntryId,note FROM invoice_claim_payments WHERE invoice_document_id=?1 ORDER BY id",id)?,
 "badDebtWriteOffs":rows(tx,"SELECT id AS writeOffId,writeoff_date AS writeOffDate,gross_amount AS grossAmount,net_amount AS netAmount,vat_amount AS vatAmount,journal_entry_id AS journalEntryId,note FROM invoice_bad_debt_writeoffs WHERE invoice_document_id=?1 ORDER BY id",id)?,
 "reminders":rows(tx,"SELECT r.id AS reminderId,r.reminder_date AS reminderDate,r.fee_amount AS feeAmount,r.note,p.journal_entry_id AS journalEntryId FROM invoice_reminders r LEFT JOIN invoice_reminder_postings p ON p.reminder_id=r.id WHERE r.invoice_document_id=?1 ORDER BY r.reminder_date,r.id",id)?,
 "compensationClaims":rows(tx,"SELECT c.id AS claimId,c.claim_date AS claimDate,c.amount_dkk AS amountDkk,c.note,p.journal_entry_id AS journalEntryId FROM invoice_compensation_claims c LEFT JOIN invoice_compensation_postings p ON p.compensation_claim_id=c.id WHERE c.invoice_document_id=?1 ORDER BY c.claim_date,c.id",id)?,
 "interestClaims":rows(tx,"SELECT c.id AS claimId,c.claim_date AS claimDate,c.amount_dkk AS amountDkk,c.reference_rate_percent AS referenceRatePercent,c.annual_interest_rate_percent AS annualInterestRatePercent,c.overdue_days AS overdueDays,c.note,p.journal_entry_id AS journalEntryId FROM invoice_interest_claims c LEFT JOIN invoice_interest_postings p ON p.interest_claim_id=c.id WHERE c.invoice_document_id=?1 ORDER BY c.claim_date,c.id",id)?,
 "interestCorrections":rows(tx,"SELECT id AS correctionId,correction_date AS correctionDate,amount_dkk AS amountDkk,journal_entry_id AS journalEntryId FROM invoice_interest_corrections WHERE invoice_document_id=?1 ORDER BY id",id)?,
 "totalReminderFees":claims["totalReminderFees"],"totalCompensationClaims":claims["totalCompensationDkk"],"totalInterestClaims":wire(&interest_net)?,"totalInterestCorrections":claims["totalInterestCorrectionsDkk"],"totalClaimPayments":claims["totalClaimPayments"],"totalBadDebtWrittenOff":wire(&p.written.gross)?});
    if let Some(due) = payload["dueDate"].as_str() {
        view["dueDate"] = json!(due);
    }
    Ok((row, view))
}
pub fn read(tx: &Transaction<'_>, kind: Kind, q: &Query, today: &str) -> Result<Value, Error> {
    let as_of = q.as_of.as_deref().unwrap_or(today);
    Date::parse(as_of)?;
    if matches!(kind, Kind::Status) {
        let id = if let Some(id) = q.document_id {
            Some(id)
        } else if let Some(number) = normalize(q.invoice_number.as_deref()) {
            tx.query_row("SELECT id FROM documents WHERE document_type='issued_invoice' AND invoice_no=?1 ORDER BY id LIMIT 1",[number],|r|r.get(0)).optional().map_err(invalid)?
        } else {
            None
        };
        let Some(id) = id else {
            return Ok(
                json!({"ok":false,"errors":["invoice could not be resolved from documentId/invoiceNumber"]}),
            );
        };
        let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM documents WHERE id=?1 AND document_type='issued_invoice')",[id],|r|r.get(0)).map_err(invalid)?;
        if !exists {
            return Ok(
                json!({"ok":false,"errors":[format!("invoice document {id} does not exist")]}),
            );
        }
        return status(tx, id, as_of).map(|(_, view)| envelope(view));
    }
    let requested = if matches!(kind, Kind::Overdue) {
        "overdue"
    } else {
        q.status.as_deref().unwrap_or("all")
    };
    if !matches!(
        requested,
        "all" | "open" | "paid" | "credited" | "refunded" | "overpaid" | "written_off" | "overdue"
    ) {
        return Ok(json!({"ok":false,"errors":["invalid invoice status filter"]}));
    }
    if let Some(from) = &q.from {
        Date::parse(from)?;
    }
    if let Some(to) = &q.to {
        Date::parse(to)?;
    }
    let bound = |n: &Number| {
        n.as_f64()
            .filter(|n| n.is_finite())
            .ok_or_else(|| refused("finite invoice amount filter required"))
    };
    let min = q
        .amount
        .as_ref()
        .or(q.min_amount.as_ref())
        .map(bound)
        .transpose()?;
    let max = q
        .amount
        .as_ref()
        .or(q.max_amount.as_ref())
        .map(bound)
        .transpose()?;
    let number = normalize(q.invoice_number.as_deref());
    let cvr = normalize(q.customer_cvr.as_deref()).map(|s| s.to_uppercase());
    let needle = normalize(q.query.as_deref());
    let mut statement=tx.prepare("SELECT id FROM documents WHERE document_type='issued_invoice' ORDER BY invoice_date,id").map_err(invalid)?;
    let ids = statement
        .query_map([], |r| r.get::<_, i64>(0))
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    let mut result = vec![];
    for id in ids {
        let (row, _) = status(tx, id, as_of)?;
        let date = row["invoiceDate"].as_str();
        let gross = row["grossAmount"]
            .as_f64()
            .ok_or_else(|| refused("numeric original invoice gross required"))?;
        if q.from
            .as_deref()
            .is_some_and(|bound| date.is_none_or(|d| d < bound))
            || q.to
                .as_deref()
                .is_some_and(|bound| date.is_none_or(|d| d > bound))
            || number.as_ref().is_some_and(|n| row["invoiceNumber"] != *n)
            || cvr.as_ref().is_some_and(|n| row["customerCvr"] != *n)
            || !folded(row["customerName"].as_str(), q.customer.as_deref())
            || needle.as_deref().is_some_and(|n| {
                !folded(row["customerName"].as_str(), Some(n))
                    && !folded(row["invoiceNumber"].as_str(), Some(n))
            })
            || min.as_ref().is_some_and(|n| gross < *n)
            || max.as_ref().is_some_and(|n| gross > *n)
        {
            continue;
        }
        if requested == "overdue" {
            if row["isOverdue"] != true
                || row["overdueDays"].as_u64().unwrap() < q.min_days.unwrap_or(0)
            {
                continue;
            }
        } else if requested != "all" && row["status"] != requested {
            continue;
        }
        result.push(row);
    }
    result.sort_by(|a, b| {
        let base = || {
            a["effectiveDueDate"]
                .as_str()
                .cmp(&b["effectiveDueDate"].as_str())
                .then_with(|| {
                    a["invoiceNumber"]
                        .as_str()
                        .cmp(&b["invoiceNumber"].as_str())
                })
                .then_with(|| a["documentId"].as_i64().cmp(&b["documentId"].as_i64()))
        };
        if requested == "overdue" {
            b["overdueDays"]
                .as_i64()
                .cmp(&a["overdueDays"].as_i64())
                .then_with(|| {
                    let left =
                        Money::parse(&a["openBalance"].to_string()).expect("validated balance");
                    let right =
                        Money::parse(&b["openBalance"].to_string()).expect("validated balance");
                    if right == left {
                        std::cmp::Ordering::Equal
                    } else if right.subtract(&left).is_negative() {
                        std::cmp::Ordering::Less
                    } else {
                        std::cmp::Ordering::Greater
                    }
                })
                .then_with(base)
        } else {
            base()
        }
    });
    let mut value = json!({"count":result.len(),"status":requested,"asOfDate":as_of,"rows":result});
    if let Some(needle) = needle {
        value["query"] = json!(needle);
    }
    Ok(envelope(value))
}
#[cfg(test)]
mod tests;
