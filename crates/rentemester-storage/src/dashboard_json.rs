//! Original company dashboard JSON from shared admitted, source-aware readers.
use crate::{invalid, legacy_money};
use rentemester_engine::{Error, fiscal_year::Date, money::Money, sales_invoice};
use rusqlite::{OptionalExtension, Transaction};
use serde_json::{Value, json};

fn unavailable_vat(registered: bool, reason: &str) -> Value {
    json!({"registered":registered,"available":false,"periodStart":null,"periodEnd":null,"periodLabel":null,"deadline":null,"periodStatus":null,"netVatPayable":null,"daysRemaining":null,"errors":[reason],"basis":"native_history_unavailable"})
}
/// Present the established native delivery state using original dashboard keys.
/// The shared reader verifies evidence; this adapter performs no delivery decisions.
fn peppol_status(tx: &Transaction<'_>, id: i64) -> Result<Value, Error> {
    let Some(mut state) = crate::public_einvoice::get(tx, id)? else {
        return Ok(Value::Null);
    };
    state["acknowledgedAt"] = if state["status"] == "acknowledged" {
        let observed = state["history"]
            .as_array()
            .and_then(|rows| rows.iter().rev().find(|r| r["eventType"] == "delivered"))
            .map(|r| r["at"].clone())
            .filter(|v| v.is_string());
        match observed {
            Some(value) => value,
            None if state["legacy"] == true => {
                let at: Option<String> = tx
                    .query_row(
                        "SELECT acknowledged_at FROM peppol_submissions WHERE id=?1",
                        [state["id"]
                            .as_i64()
                            .ok_or_else(|| invalid("PEPPOL identity unavailable"))?],
                        |r| r.get(0),
                    )
                    .optional()
                    .map_err(invalid)?
                    .flatten();
                json!(at)
            }
            None => return Err(invalid("Acknowledged delivery timestamp unavailable")),
        }
    } else {
        Value::Null
    };
    // Native 'prepared' means a retained provider document from the queued event.
    if state["status"] == "prepared" {
        state["nativeStatus"] = json!("prepared");
        state["status"] = json!("queued");
    }
    Ok(state)
}
fn invoice_row(tx: &Transaction<'_>, row: &Value, today: &str) -> Result<Value, Error> {
    let id = row["documentId"]
        .as_i64()
        .ok_or_else(|| invalid("invoice identity unavailable"))?;
    let (raw,date,gross,posted):(String,String,Money,bool)=tx.query_row("SELECT payload_json,invoice_date,amount_inc_vat,EXISTS(SELECT 1 FROM issued_invoice_postings p WHERE p.invoice_document_id=d.id) FROM documents d WHERE id=?1 AND document_type='issued_invoice'",[id],|r|Ok((r.get(0)?,r.get(1)?,legacy_money(r.get_ref(2)?).map_err(|_|rusqlite::Error::InvalidQuery)?,r.get(3)?))).map_err(invalid)?;
    Date::parse(&date)?;
    let payload: Value = serde_json::from_str(&raw).map_err(invalid)?;
    let ean = sales_invoice::recipient_ean(payload["buyer"]["eanNumber"].as_str())?;
    let claims = if posted {
        Some(crate::reminder_claims::read(tx, id, today)?)
    } else {
        None
    };
    Ok(
        json!({"documentId":id,"invoiceNumber":row["invoiceNo"],"invoiceDate":date,"customerName":payload["buyer"]["name"],"customerCvr":payload["buyer"]["vatOrCvr"],"buyerEanNumber":ean,"buyerPublicRecipient":payload["buyer"]["publicRecipient"]==true||ean.is_some(),"peppolStatus":peppol_status(tx,id)?,"grossAmount":sales_invoice::wire(&gross)?,"currency":row["currency"],"openBalance":row["openBalance"],"openBalanceDkk":row["openBalanceDkk"],"claimOpenBalance":claims.as_ref().map(|c|c["claimOpenBalance"].clone()).unwrap_or(Value::Null),"status":"open","effectiveDueDate":row["dueDate"],"isOverdue":row["overdueDays"].as_i64().is_some_and(|n|n>0),"overdueDays":row["overdueDays"],"currentClaimsAvailable":posted,"currentClaimsUnavailableReason":if posted{None}else{Some("ledger_not_posted")},"basis":{"principal":"dated_native_invoice","claims":"current_state","peppol":"current_state","currentStateDate":today}}),
    )
}

pub fn read(tx: &Transaction<'_>, slug: &str, as_of: &str, today: &str) -> Result<Value, Error> {
    Date::parse(as_of)?;
    Date::parse(today)?;
    crate::journal_preview::require_current(tx)?;
    let audit = crate::journal_hash::verify(tx)?;
    if !audit.ok {
        return Err(Error::new(
            "dashboard_source_unavailable",
            "Current journal hash integrity differs",
        ));
    }
    let profile = crate::company_setup::profile(tx)?;
    crate::opening_balance::validate_history(tx)?;
    let basis = match crate::opening_balance::require_report_as_of(tx, as_of) {
        Ok(v) => Some(v),
        Err(e) if e.code == "archive_report_unsupported" => None,
        Err(e) => return Err(e),
    };
    let (invoices, overdue) = if let Some(basis) = &basis {
        let snapshot = crate::portfolio::dated_invoices(tx, as_of)?;
        let rows = snapshot["rows"]
            .as_array()
            .ok_or_else(|| invalid("dated invoice rows unavailable"))?
            .iter()
            .map(|r| invoice_row(tx, r, today))
            .collect::<Result<Vec<_>, _>>()?;
        let mut overdue = rows
            .iter()
            .filter(|r| r["isOverdue"] == true)
            .cloned()
            .collect::<Vec<_>>();
        // Canonical amounts are Money, never binary floating-point sort keys.
        let mut keys = std::collections::BTreeMap::new();
        for r in &overdue {
            keys.insert(
                r["documentId"]
                    .as_i64()
                    .ok_or_else(|| invalid("invoice identity unavailable"))?,
                crate::invoice_fx::amount(&r["openBalanceDkk"])?,
            );
        }
        overdue.sort_by(|a, b| {
            b["overdueDays"]
                .as_i64()
                .cmp(&a["overdueDays"].as_i64())
                .then_with(|| {
                    let left = &keys[&a["documentId"].as_i64().unwrap()];
                    let right = &keys[&b["documentId"].as_i64().unwrap()];
                    // Exact numeric comparison via Money difference.
                    let difference = right.subtract(left);
                    if difference == Money::default() {
                        std::cmp::Ordering::Equal
                    } else if difference.is_negative() {
                        std::cmp::Ordering::Less
                    } else {
                        std::cmp::Ordering::Greater
                    }
                })
                .then_with(|| {
                    a["effectiveDueDate"]
                        .as_str()
                        .cmp(&b["effectiveDueDate"].as_str())
                })
                .then_with(|| {
                    a["invoiceNumber"]
                        .as_str()
                        .cmp(&b["invoiceNumber"].as_str())
                })
        });
        (
            json!({"count":snapshot["openInvoiceCount"],"openTotal":snapshot["openInvoiceTotal"],"currency":"DKK","rows":rows,"available":true,"basis":basis}),
            json!({"count":overdue.len(),"rows":overdue,"available":true}),
        )
    } else {
        (
            json!({"count":null,"openTotal":null,"currency":"DKK","rows":null,"available":false,"reason":"native_history_unavailable"}),
            json!({"count":null,"rows":null,"available":false,"reason":"native_history_unavailable"}),
        )
    };
    let bank = crate::bank::read(tx, slug, today, None)?;
    let mut unmatched = 0u64;
    for year in bank["fiscalYears"]
        .as_array()
        .ok_or_else(|| invalid("bank fiscal inventory unavailable"))?
    {
        let selected = crate::bank::read(
            tx,
            slug,
            today,
            Some(
                year["label"]
                    .as_str()
                    .ok_or_else(|| invalid("bank year unavailable"))?,
            ),
        )?;
        unmatched = unmatched
            .checked_add(
                selected["unmatchedCount"]
                    .as_u64()
                    .ok_or_else(|| invalid("bank unmatched count unavailable"))?,
            )
            .ok_or_else(|| invalid("bank count overflow"))?;
    }
    let exceptions=crate::exceptions::rows(tx,"open")?.into_iter().map(|r|json!({"id":r["id"],"type":r["type"],"severity":r["severity"],"status":r["status"],"message":r["message"]})).collect::<Vec<_>>();
    let vat = if profile["vatPeriodType"].is_null() {
        json!({"registered":false,"available":true,"periodStart":null,"periodEnd":null,"periodLabel":null,"deadline":null,"periodStatus":null,"netVatPayable":0,"daysRemaining":null,"errors":[]})
    } else if basis.is_none() {
        unavailable_vat(
            true,
            "Native accounting history is unavailable at the selected date",
        )
    } else {
        let report = crate::vat_report::dashboard_period(tx, as_of)?;
        let from = report["periodStart"]
            .as_str()
            .ok_or_else(|| invalid("VAT period start unavailable"))?;
        let to = report["periodEnd"]
            .as_str()
            .ok_or_else(|| invalid("VAT period end unavailable"))?;
        match crate::opening_balance::require_report_range(tx, from, to) {
            Ok(_) => {
                json!({"registered":true,"available":true,"periodStart":from,"periodEnd":to,"periodLabel":report["periodLabel"],"deadline":report["deadline"],"periodStatus":report["periodStatus"],"netVatPayable":report["payable"],"daysRemaining":report["daysRemaining"],"errors":report["errors"],"hasUnreportedActivity":report["hasUnreportedActivity"],"basis":"selected_whole_vat_period"})
            }
            Err(e) if e.code == "archive_report_unsupported" => unavailable_vat(true, &e.message),
            Err(e) => return Err(e),
        }
    };
    let activity = crate::audit_log::read(
        tx,
        slug,
        &crate::audit_log::Query {
            limit: Some(10),
            ..Default::default()
        },
    )?;
    let company = json!({"name":profile["name"],"cvr":profile["cvr"],"country":profile["country"],"currency":profile["currency"],"fiscalYearStartMonth":profile["fiscalYearStartMonth"],"fiscalYearLabelStrategy":profile["fiscalYearLabelStrategy"]});
    Ok(
        json!({"slug":slug,"asOf":as_of,"company":company,"invoices":invoices,"overdueInvoices":overdue,"unlinkedBank":{"count":unmatched,"basis":"current_state"},"exceptions":{"count":exceptions.len(),"rows":exceptions,"basis":"current_state"},"vat":vat,"audit":{"ok":audit.ok,"entryCount":audit.entries_checked,"firstError":audit.errors.first(),"errors":audit.errors,"scope":"current_journal_hash_chain"},"recentActivity":activity["rows"],"importedReceivables":crate::imported_receivables::read(tx,as_of)?,"basis":{"principalAsOf":as_of,"currentStateDate":today,"recentActivity":"current_state","audit":"current_journal_hash_chain","claims":"current_state","peppol":"current_state"}}),
    )
}
#[cfg(test)]
mod tests;
