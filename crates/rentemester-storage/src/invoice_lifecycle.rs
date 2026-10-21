//! Original invoice commands mapped to the single native writer, inside its held transaction.
use crate::invalid;
use rentemester_engine::{
    Error,
    sales_invoice::{self, Input},
};
use rusqlite::{OptionalExtension, Transaction};
use serde_json::{Value, json};
#[derive(Clone, Copy)]
pub enum Kind {
    Issue,
    Create,
    Render,
    Post,
    SettleBank,
    CreditNote,
    RefundBank,
}
fn input_error(message: impl Into<String>) -> Error {
    Error::new("invoice_lifecycle_input_invalid", message)
}
fn fields(value: &Value, allowed: &[&str]) -> Result<(), Error> {
    let map = value
        .as_object()
        .ok_or_else(|| input_error("object required"))?;
    if map.keys().any(|k| !allowed.contains(&k.as_str())) {
        return Err(input_error("unknown invoice field"));
    }
    Ok(())
}
fn bounded(value: &Value) -> Result<(), Error> {
    match value {
        Value::Null => return Err(input_error("null invoice fields are not accepted")),
        Value::String(s) if s.len() > 16_384 || s.chars().any(char::is_control) => {
            return Err(input_error("invoice text must be bounded without controls"));
        }
        Value::Array(a) => {
            if a.len() > 200 {
                return Err(input_error("too many invoice items"));
            }
            for v in a {
                bounded(v)?;
            }
        }
        Value::Object(m) => {
            for (k, v) in m {
                if [
                    "name",
                    "address",
                    "vatOrCvr",
                    "eanNumber",
                    "description",
                    "invoiceType",
                    "vatTreatment",
                    "issueDate",
                    "invoiceNumber",
                    "dueDate",
                    "deliveryDate",
                    "currency",
                    "unitCode",
                    "reverseChargeBasis",
                    "reverseChargeNote",
                    "deliveryPeriodStart",
                    "deliveryPeriodEnd",
                    "buyerReference",
                    "orderReference",
                    "vatComputationBasis",
                    "reason",
                    "creditNoteNumber",
                    "bankTransactionReference",
                    "bankAccountNo",
                    "receivableAccountNo",
                    "paymentDate",
                    "refundDate",
                    "originalInvoiceNumber",
                    "revenueAccountNo",
                ]
                .contains(&k.as_str())
                    && !v.is_string()
                {
                    return Err(input_error("invoice text field must be a string"));
                }
                if [
                    "quantity",
                    "unitPriceExVat",
                    "lineTotalExVat",
                    "netAmount",
                    "vatRate",
                    "vatAmount",
                    "grossAmount",
                    "fxRateToDkk",
                    "netAmountDkk",
                    "vatAmountDkk",
                    "grossAmountDkk",
                    "vatRatePercent",
                    "amount",
                ]
                .contains(&k.as_str())
                    && !v.is_number()
                {
                    return Err(input_error("invoice numeric field required"));
                }
                if k == "publicRecipient" && !v.is_boolean() {
                    return Err(input_error("publicRecipient must be boolean"));
                }
                bounded(v)?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn id(v: Option<&Value>) -> Result<(), Error> {
    if let Some(v) = v
        && !v
            .as_i64()
            .is_some_and(|n| (1..=9_007_199_254_740_991).contains(&n))
    {
        return Err(input_error(
            "positive safe document/bank/customer id required",
        ));
    }
    Ok(())
}
fn string(v: Option<&Value>) -> Result<(), Error> {
    if let Some(v) = v
        && v.as_str()
            .is_none_or(|s| s.trim().is_empty() || s.len() > 2000)
    {
        return Err(input_error("nonblank invoice selector/text required"));
    }
    Ok(())
}
pub fn validate(kind: Kind, body: &Value) -> Result<(), Error> {
    bounded(body)?;
    let top = match kind {
        Kind::Issue | Kind::Create => &["payload", "customerId", "confirm"][..],
        Kind::Render => &["documentId", "invoiceNumber", "confirm"],
        Kind::Post => &["documentId", "invoiceNumber", "revenueAccountNo", "confirm"],
        _ => &["payload", "confirm"],
    };
    fields(body, top)?;
    if body["confirm"] != true {
        return Err(Error::new("confirmation_required", "confirm:true required"));
    }
    id(body.get("customerId"))?;
    match kind {
        Kind::Render | Kind::Post => {
            id(body.get("documentId"))?;
            string(body.get("invoiceNumber"))?;
            string(body.get("revenueAccountNo"))?;
            if body.get("documentId").is_none() && body.get("invoiceNumber").is_none() {
                return Err(input_error("documentId or invoiceNumber required"));
            }
        }
        Kind::Issue => {
            let p = &body["payload"];
            fields(
                p,
                &[
                    "invoiceType",
                    "vatTreatment",
                    "issueDate",
                    "invoiceNumber",
                    "seller",
                    "buyer",
                    "lines",
                    "totals",
                    "reverseChargeBasis",
                    "reverseChargeNote",
                    "currency",
                    "dueDate",
                    "deliveryDate",
                    "deliveryPeriodStart",
                    "deliveryPeriodEnd",
                    "unitCode",
                ],
            )?;
            if !matches!(p["invoiceType"].as_str(), Some("full" | "simplified")) {
                return Err(input_error("invoiceType required"));
            }
            for key in ["seller", "buyer"] {
                if let Some(v) = p.get(key) {
                    fields(
                        v,
                        if key == "seller" {
                            &["name", "address", "vatOrCvr"]
                        } else {
                            &[
                                "name",
                                "address",
                                "vatOrCvr",
                                "eanNumber",
                                "publicRecipient",
                                "buyerReference",
                                "orderReference",
                            ]
                        },
                    )?;
                }
            }
            if let Some(lines) = p.get("lines") {
                for line in lines
                    .as_array()
                    .ok_or_else(|| input_error("lines array required"))?
                {
                    fields(
                        line,
                        &[
                            "description",
                            "quantity",
                            "unitPriceExVat",
                            "lineTotalExVat",
                            "unitCode",
                            "taxClassification",
                            "vatRate",
                            "reverseChargeBasis",
                        ],
                    )?;
                }
            }
            if let Some(t) = p.get("totals") {
                fields(
                    t,
                    &[
                        "netAmount",
                        "vatRate",
                        "vatAmount",
                        "grossAmount",
                        "fxRateToDkk",
                        "netAmountDkk",
                        "vatAmountDkk",
                        "grossAmountDkk",
                        "vatComputationBasis",
                    ],
                )?;
            }
        }
        Kind::Create => {
            let _: Input = serde_json::from_value(body["payload"].clone())
                .map_err(|_| input_error("invalid native invoice essentials"))?;
        }
        Kind::CreditNote => {
            let p = &body["payload"];
            fields(
                p,
                &[
                    "originalInvoiceDocumentId",
                    "originalInvoiceNumber",
                    "issueDate",
                    "reason",
                    "grossAmount",
                    "creditNoteNumber",
                ],
            )?;
            id(p.get("originalInvoiceDocumentId"))?;
            string(p.get("originalInvoiceNumber"))?;
        }
        Kind::SettleBank | Kind::RefundBank => {
            let p = &body["payload"];
            fields(
                p,
                if matches!(kind, Kind::RefundBank) {
                    &[
                        "invoiceDocumentId",
                        "invoiceNumber",
                        "bankTransactionId",
                        "bankTransactionReference",
                        "paymentDate",
                        "refundDate",
                        "amount",
                        "bankAccountNo",
                        "receivableAccountNo",
                    ]
                } else {
                    &[
                        "invoiceDocumentId",
                        "invoiceNumber",
                        "bankTransactionId",
                        "bankTransactionReference",
                        "paymentDate",
                        "amount",
                        "bankAccountNo",
                        "receivableAccountNo",
                    ]
                },
            )?;
            id(p.get("invoiceDocumentId"))?;
            id(p.get("bankTransactionId"))?;
            for key in [
                "invoiceNumber",
                "bankTransactionReference",
                "bankAccountNo",
                "receivableAccountNo",
                "paymentDate",
                "refundDate",
            ] {
                string(p.get(key))?;
            }
            if p.get("bankTransactionId").is_some() == p.get("bankTransactionReference").is_some() {
                return Err(input_error("exactly one bank selector required"));
            }
        }
    }
    let payload = body.get("payload").unwrap_or(body);
    for key in ["amount", "grossAmount"] {
        if payload.get(key).is_some_and(|v| !v.is_number()) {
            return Err(input_error("numeric monetary field required"));
        }
    }
    for key in ["issueDate", "reason", "creditNoteNumber"] {
        string(payload.get(key))?;
    }
    Ok(())
}
pub fn resolve_invoice(
    tx: &Transaction<'_>,
    value: &Value,
    id_key: &str,
    number_key: &str,
) -> Result<i64, Error> {
    if let Some(id) = value[id_key].as_i64() {
        let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM documents WHERE id=?1 AND document_type='issued_invoice')",[id],|r|r.get(0)).map_err(invalid)?;
        return if exists {
            Ok(id)
        } else {
            Err(Error::new(
                "invoice_selector_not_found",
                format!("invoice document {id} does not exist"),
            ))
        };
    }
    let number = value[number_key].as_str().unwrap_or("").trim();
    tx.query_row("SELECT id FROM documents WHERE document_type='issued_invoice' AND invoice_no=?1",[number],|r|r.get(0)).optional().map_err(invalid)?.ok_or_else(||Error::new("invoice_selector_not_found",format!("No issued invoice has invoice number '{number}' — check invoice_list or invoice_find")))
}
/// Validate source-only calculated fields before converting to the native essential input.
pub fn prepare(
    tx: &Transaction<'_>,
    kind: Kind,
    body: &Value,
    now: &str,
) -> Result<(Value, Value, Vec<String>), Error> {
    let mut source = body["payload"].clone();
    if let Some(customer) = body.get("customerId") {
        if source.get("customerId").is_some_and(|v| v != customer) {
            return Err(input_error("conflicting customerId"));
        }
        source["customerId"] = customer.clone();
    }
    let mut notes = vec![];
    if let Some(customer) = source["customerId"].as_i64() {
        let (currency,terms):(String,Option<i64>)=tx.query_row("SELECT default_currency,payment_terms_days FROM customers WHERE id=?1 AND archived=0",[customer],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(invalid)?.ok_or_else(||input_error("active customer required"))?;
        if source.get("currency").is_none() {
            source["currency"] = json!(currency);
        }
        if source.get("dueDate").is_none()
            && let Some(terms) = terms
        {
            let company: i64 = tx
                .query_row(
                    "SELECT payment_terms_days FROM companies WHERE id=1",
                    [],
                    |r| r.get(0),
                )
                .map_err(invalid)?;
            if company != terms {
                notes.push(format!("Betalingsfrist {terms} dage fra kundekortet — virksomhedens standard er {company} dage."));
            }
        }
    }
    let supplied = source.clone();
    if matches!(kind, Kind::Issue) {
        let full = source["invoiceType"] == "full";
        let standard = source["vatTreatment"].as_str().unwrap_or("standard") == "standard";
        if source["totals"].get("grossAmount").is_none()
            || (full && source["totals"].get("netAmount").is_none())
            || (standard
                && (source["totals"].get("vatRate").is_none()
                    || (source["totals"].get("vatAmount").is_none()
                        && source["totals"]["vatComputationBasis"] != "VAT_20_OF_GROSS")))
        {
            return Err(input_error(
                "original invoice requires reconciled gross/net/VAT totals",
            ));
        }
    }
    if matches!(kind, Kind::Issue) {
        let t = source.get("totals").cloned();
        if let Some(rate) = t.as_ref().and_then(|t| t.get("fxRateToDkk")) {
            source["fxRateToDkk"] = rate.clone();
        }
        let reduced = source["lines"].as_array().is_some_and(|a| {
            a.iter()
                .all(|l| l.get("quantity").is_none() && l.get("unitPriceExVat").is_none())
        });
        if !reduced {
            source.as_object_mut().unwrap().remove("totals");
        } else if let Some(t) = source["totals"].as_object_mut() {
            t.retain(|k, _| {
                [
                    "grossAmount",
                    "vatRate",
                    "vatComputationBasis",
                    "vatAmount",
                    "netAmount",
                ]
                .contains(&k.as_str())
            });
        }
        if let Some(lines) = source["lines"].as_array_mut() {
            for l in lines {
                l.as_object_mut().unwrap().remove("lineTotalExVat");
            }
        }
    }
    let input: Input = serde_json::from_value(source)
        .map_err(|_| input_error("invalid invoice essentials or computed field types"))?;
    let computed = sales_invoice::compute(&input).map_err(|e| input_error(e.message))?;
    if matches!(kind, Kind::Issue) {
        if let Some(lines) = supplied["lines"].as_array() {
            for (index, line) in lines.iter().enumerate() {
                if let Some(v) = line.get("lineTotalExVat") {
                    same_amount(v, &computed["lines"][index]["lineTotalExVat"])?;
                }
            }
        }
        if let Some(totals) = supplied["totals"].as_object() {
            for (key, value) in totals {
                match key.as_str() {
                    "vatRate" | "fxRateToDkk" => {
                        if value.as_f64().is_none()
                            || value.as_f64() != computed["totals"][key].as_f64()
                        {
                            return Err(input_error(format!(
                                "supplied {key} differs from native computation"
                            )));
                        }
                    }
                    "vatComputationBasis" => {
                        if value != &computed["totals"][key] {
                            return Err(input_error("VAT computation basis differs"));
                        }
                    }
                    _ => same_amount(value, &computed["totals"][key])?,
                }
            }
        }
    }
    Ok((
        crate::sales::prepare(tx, &input, now, true)?,
        computed,
        notes,
    ))
}
fn same_amount(a: &Value, b: &Value) -> Result<(), Error> {
    if crate::invoice_fx::amount(a).map_err(|e| input_error(e.message))?
        != crate::invoice_fx::amount(b).map_err(|e| input_error(e.message))?
    {
        return Err(input_error(
            "supplied invoice amount differs from native computation",
        ));
    }
    Ok(())
}
pub fn bank_id(tx: &Transaction<'_>, p: &Value) -> Result<i64, Error> {
    if let Some(id) = p["bankTransactionId"].as_i64() {
        let exists: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM bank_transactions WHERE id=?1)",
                [id],
                |r| r.get(0),
            )
            .map_err(invalid)?;
        return if exists {
            Ok(id)
        } else {
            Err(Error::new(
                "bank_selector_not_found",
                format!("bank transaction {id} does not exist"),
            ))
        };
    }
    let mut q = tx
        .prepare("SELECT id FROM bank_transactions WHERE reference=?1 ORDER BY id LIMIT 2")
        .map_err(invalid)?;
    let ids = q
        .query_map(
            [p["bankTransactionReference"].as_str().unwrap_or("").trim()],
            |r| r.get::<_, i64>(0),
        )
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    if ids.len() != 1 {
        return Err(Error::new(
            "bank_selector_not_found",
            "bank reference must identify exactly one imported transaction",
        ));
    }
    Ok(ids[0])
}
pub fn assert_accounts(
    tx: &Transaction<'_>,
    p: &Value,
    invoice: i64,
    bank: i64,
) -> Result<(), Error> {
    if let Some(expected) = p["receivableAccountNo"].as_str() {
        let actual:String=tx.query_row("SELECT a.account_no FROM issued_invoice_postings p JOIN accounts a ON a.id=p.receivable_account_id WHERE p.invoice_document_id=?1",[invoice],|r|r.get(0)).map_err(invalid)?;
        if expected.trim() != actual {
            return Err(Error::new(
                "invoice_account_mismatch",
                "receivableAccountNo differs from original booked receivable",
            ));
        }
    }
    if let Some(expected) = p["bankAccountNo"].as_str() {
        let mapped:Option<String>=tx.query_row("SELECT a.ledger_account_no FROM bank_transactions b LEFT JOIN bank_accounts a ON a.id=b.bank_account_id WHERE b.id=?1",[bank],|r|r.get(0)).map_err(invalid)?;
        let actual=match mapped{Some(v)=>v,None=>tx.query_row("SELECT account_no FROM account_role_mappings WHERE role='bank' AND status='confirmed'",[],|r|r.get(0)).map_err(invalid)?};
        if expected.trim() != actual {
            return Err(Error::new(
                "invoice_account_mismatch",
                "bankAccountNo differs from imported bank ledger mapping",
            ));
        }
    }
    Ok(())
}
pub fn envelope(mut value: Value) -> Value {
    let rules = value.as_object_mut().unwrap().remove("appliedRules");
    value.as_object_mut().unwrap().remove("ok");
    value.as_object_mut().unwrap().remove("errors");
    let mut result = json!({"ok":true,"data":value,"errors":[]});
    if let Some(rules) = rules {
        result["appliedRules"] = rules;
    }
    result
}
pub fn issue_receipt(tx: &Transaction<'_>, id: i64, slug: &str) -> Result<Value, Error> {
    let pdf = crate::sales::pdf_id(tx, id)?;
    let (number, hash): (String, String) = tx
        .query_row(
            "SELECT invoice_no,sha256_hash FROM documents WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(invalid)?;
    let pdf_hash: String = tx
        .query_row(
            "SELECT sha256_hash FROM documents WHERE id=?1",
            [pdf],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    Ok(
        json!({"documentId":id,"invoiceNumber":number,"sha256":hash,"storedPath":format!("/api/companies/{slug}/documents/{id}/file"),"pdfDocumentId":pdf,"pdfSha256":pdf_hash,"pdfStoredPath":format!("/api/companies/{slug}/invoices/{id}/pdf")}),
    )
}
pub fn journal_receipt(tx: &Transaction<'_>, mut v: Value) -> Result<Value, Error> {
    if let Some(id) = v["entryId"].as_i64() {
        let no: String = tx
            .query_row(
                "SELECT entry_no FROM journal_entries WHERE id=?1",
                [id],
                |r| r.get(0),
            )
            .map_err(invalid)?;
        v["entryNo"] = json!(no);
    }
    Ok(v)
}
#[cfg(test)]
mod tests;
