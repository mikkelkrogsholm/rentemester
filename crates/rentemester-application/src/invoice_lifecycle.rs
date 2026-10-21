//! Original lifecycle inside the already authorized native sales transaction.
use crate::access::Permission;
use rentemester_engine::{Error, fiscal_year::Date};
use rentemester_runtime::{auth::Clock, document::Prepared, namespace::NamespaceGuard};
use rentemester_storage::{
    idempotency::PrincipalKind,
    invoice_lifecycle::{self as storage, Kind},
    sales::Artifact,
};
use rusqlite::Transaction;
use serde_json::{Value, json};
pub(crate) fn permission(kind: Kind) -> Permission {
    match kind {
        Kind::Issue | Kind::Create => Permission::CompanyDraftWrite,
        Kind::Render => Permission::CompanyDocumentsRead,
        _ => Permission::CompanyLedgerPost,
    }
}
pub(crate) struct Context<'a> {
    pub slug: &'a str,
    pub subject: &'a str,
    pub principal: PrincipalKind,
    pub clock: &'a dyn Clock,
}
fn source_error() -> Error {
    Error::new(
        "invoice_evidence_unavailable",
        "immutable invoice evidence unavailable or integrity mismatch",
    )
}
fn verify_bank(tx: &Transaction<'_>, guard: &mut NamespaceGuard, id: i64) -> Result<(), Error> {
    let hash = rentemester_storage::bank_reconciliation::source_hash(tx, id)?;
    let bytes = guard
        .read_bank_statement(&hash)
        .map_err(|_| source_error())?;
    rentemester_storage::bank_reconciliation::verify_source(tx, id, &bytes)
}
pub(crate) fn apply(
    tx: &Transaction<'_>,
    guard: &mut NamespaceGuard,
    kind: Kind,
    body: &Value,
    context: Context<'_>,
) -> Result<Value, Error> {
    let result = inner(tx, guard, kind, body, context);
    match result {
        Err(e)
            if matches!(
                e.code,
                "invoice_lifecycle_input_invalid"
                    | "confirmation_required"
                    | "invoice_selector_not_found"
                    | "bank_selector_not_found"
                    | "invoice_account_mismatch"
                    | "account_selection_required"
            ) =>
        {
            Ok(json!({"ok":false,"errors":[e.message]}))
        }
        other => other,
    }
}
fn inner(
    tx: &Transaction<'_>,
    guard: &mut NamespaceGuard,
    kind: Kind,
    body: &Value,
    context: Context<'_>,
) -> Result<Value, Error> {
    storage::validate(kind, body)?;
    let Context {
        slug,
        subject,
        principal,
        clock,
    } = context;
    let ms = clock
        .now_unix_ms()
        .map_err(|_| Error::new("clock_unavailable", "trusted invoice clock unavailable"))?;
    let now = rentemester_engine::saft::timestamp(ms)?;
    let today =
        rentemester_runtime::posting_time::latest_date(ms, 0).map_err(|_| source_error())?;
    let latest = Date::parse(&today)?;
    if !crate::periods::verify_sources(tx, guard)?.is_empty() {
        return Err(source_error());
    }
    if matches!(kind, Kind::Issue | Kind::Create) {
        let (payload, computed, notes) = storage::prepare(tx, kind, body, &now)?;
        let issued =
            crate::sales::issue_payload_with_principal(tx, guard, &payload, subject, principal)?;
        let mut receipt = storage::issue_receipt(
            tx,
            issued["documentId"].as_i64().ok_or_else(source_error)?,
            slug,
        )?;
        if !notes.is_empty() {
            receipt["notes"] = json!(notes);
        }
        if matches!(kind, Kind::Create) {
            let t = &computed["totals"];
            receipt["computed"] = json!({"lines":computed["lines"],"netAmount":t["netAmount"],"vatRate":t["vatRate"],"vatRatePercent":body["payload"]["vatRatePercent"].as_number().cloned().unwrap_or_else(||25.into()),"vatAmount":t["vatAmount"],"grossAmount":t["grossAmount"]});
        }
        return Ok(storage::envelope(receipt));
    }
    let p = if matches!(kind, Kind::Render | Kind::Post) {
        body
    } else {
        &body["payload"]
    };
    let invoice = storage::resolve_invoice(
        tx,
        p,
        if matches!(kind, Kind::Render | Kind::Post) {
            "documentId"
        } else if matches!(kind, Kind::CreditNote) {
            "originalInvoiceDocumentId"
        } else {
            "invoiceDocumentId"
        },
        if matches!(kind, Kind::CreditNote) {
            "originalInvoiceNumber"
        } else {
            "invoiceNumber"
        },
    )?;
    crate::sales::verify_invoice(tx, guard, invoice)?;
    // Verify existing cash originals too, including histories without a bank link.
    let mut q=tx.prepare("SELECT bank_transaction_id FROM invoice_payments WHERE invoice_document_id=?1 AND bank_transaction_id IS NOT NULL UNION SELECT bank_transaction_id FROM invoice_refunds WHERE invoice_document_id=?1 AND bank_transaction_id IS NOT NULL UNION SELECT bank_transaction_id FROM invoice_claim_payments WHERE invoice_document_id=?1 AND bank_transaction_id IS NOT NULL").map_err(|_|source_error())?;
    for bank in q
        .query_map([invoice], |r| r.get::<_, i64>(0))
        .map_err(|_| source_error())?
    {
        verify_bank(tx, guard, bank.map_err(|_| source_error())?)?;
    }
    let value = match kind {
        Kind::Render => {
            let receipt = storage::issue_receipt(tx, invoice, slug)?;
            json!({"renderDocumentId":receipt["pdfDocumentId"],"invoiceNumber":receipt["invoiceNumber"],"storedPath":receipt["pdfStoredPath"],"sha256":receipt["pdfSha256"],"contentType":"application/pdf"})
        }
        Kind::Post => {
            let account=p["revenueAccountNo"].as_str().ok_or_else(||Error::new("account_selection_required","Select revenueAccountNo (CLI --revenue-account); an active income/credit account is required"))?;
            let input = rentemester_storage::sales::Post {
                invoice_document_id: invoice,
                revenue_account_no: account.trim().into(),
                confirm: true,
            };
            rentemester_storage::sales::post_with_principal(tx, &input, subject, latest, principal)?
        }
        Kind::CreditNote => {
            let input:rentemester_storage::credit_note::Input=serde_json::from_value(json!({"invoiceDocumentId":invoice,"issueDate":p["issueDate"],"reason":p["reason"],"grossAmount":p["grossAmount"],"creditNoteNumber":p["creditNoteNumber"],"confirm":true})).map_err(|_|Error::new("invoice_lifecycle_input_invalid","invalid credit note payload"))?;
            let credit_latest = rentemester_runtime::posting_time::latest_date(ms, 45)
                .map_err(|_| source_error())?;
            let mut value = credit(
                tx,
                guard,
                &input,
                &now,
                subject,
                Date::parse(&credit_latest)?,
                principal,
            )?;
            let id = value["documentId"].as_i64().ok_or_else(source_error)?;
            let hash: String = tx
                .query_row("SELECT sha256_hash FROM documents WHERE id=?1", [id], |r| {
                    r.get(0)
                })
                .map_err(|_| source_error())?;
            value["sha256"] = json!(hash);
            value["storedPath"] = json!(format!("/api/companies/{slug}/documents/{id}/file"));
            value
        }
        Kind::SettleBank | Kind::RefundBank => {
            let bank = storage::bank_id(tx, p)?;
            verify_bank(tx, guard, bank)?;
            storage::assert_accounts(tx, p, invoice, bank)?;
            if matches!(kind, Kind::RefundBank) {
                if p.get("paymentDate").is_some()
                    && p.get("refundDate").is_some()
                    && p["paymentDate"] != p["refundDate"]
                {
                    return Err(Error::new(
                        "invoice_lifecycle_input_invalid",
                        "paymentDate and refundDate disagree",
                    ));
                }
                let input = rentemester_storage::invoice_settlement::RefundInput {
                    invoice_document_id: invoice,
                    bank_transaction_id: Some(bank),
                    bank_transaction_reference: None,
                    refund_date: p
                        .get("refundDate")
                        .or_else(|| p.get("paymentDate"))
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    amount: p["amount"].as_number().cloned(),
                    confirm: true,
                };
                rentemester_storage::invoice_settlement::refund_with_principal(
                    tx, &input, subject, latest, principal,
                )?
            } else {
                let input = rentemester_storage::invoice_settlement::Input {
                    invoice_document_id: invoice,
                    bank_transaction_id: Some(bank),
                    bank_transaction_reference: None,
                    payment_date: p["paymentDate"].as_str().map(str::to_owned),
                    amount: p["amount"].as_number().cloned(),
                    confirm: true,
                };
                let mut v = rentemester_storage::invoice_settlement::settle_with_principal(
                    tx, &input, subject, latest, principal,
                )?;
                let status = rentemester_storage::invoice_original::read(
                    tx,
                    rentemester_storage::invoice_original::Kind::Status,
                    &rentemester_storage::invoice_original::Query {
                        document_id: Some(invoice),
                        ..Default::default()
                    },
                    &today,
                )?;
                v["claimOpenBalance"] = status["data"]["claimOpenBalance"].clone();
                v
            }
        }
        _ => unreachable!(),
    };
    Ok(storage::envelope(storage::journal_receipt(tx, value)?))
}
pub(crate) fn credit(
    tx: &Transaction<'_>,
    guard: &mut NamespaceGuard,
    input: &rentemester_storage::credit_note::Input,
    now: &str,
    subject: &str,
    latest: Date,
    principal: PrincipalKind,
) -> Result<Value, Error> {
    crate::sales::verify_invoice(tx, guard, input.invoice_document_id)?;
    let prepared = rentemester_storage::credit_note::prepare(tx, input, now)?;
    let no = prepared.payload["creditNoteNumber"]
        .as_str()
        .ok_or_else(source_error)?;
    let file = Prepared::new(
        format!("{no}.json"),
        serde_json::to_vec_pretty(&prepared.payload).map_err(|_| source_error())?,
    )
    .map_err(|_| source_error())?;
    let path = guard.publish_document(&file).map_err(|_| source_error())?;
    rentemester_storage::credit_note::register_and_post_with_principal(
        tx,
        prepared,
        Artifact {
            filename: file.filename(),
            path: &path,
            hash: file.hash(),
            mime: file.mime(),
        },
        subject,
        latest,
        principal,
    )
}
#[cfg(test)]
mod tests;
