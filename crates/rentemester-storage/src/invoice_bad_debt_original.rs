//! Original payload adapter; the native loss writer remains the only financial owner.
use crate::{idempotency::PrincipalKind, invalid, invoice_bad_debt as native};
use rentemester_engine::{Error, fiscal_year::Date};
use rusqlite::Transaction;
use serde_json::{Value, json};
fn refused(message: &str) -> Error {
    Error::new("invoice_bad_debt_input_invalid", message)
}
pub fn validate(body: &Value) -> Result<(), Error> {
    let o = body.as_object().ok_or_else(|| refused("object required"))?;
    if o.keys()
        .any(|k| !["payload", "confirm"].contains(&k.as_str()))
    {
        return Err(refused("unknown body field"));
    }
    if body["confirm"] != true {
        return Err(Error::new("confirmation_required", "confirm:true required"));
    }
    let p = body["payload"]
        .as_object()
        .ok_or_else(|| refused("object payload required"))?;
    for (k, v) in p {
        if ![
            "invoiceDocumentId",
            "invoiceNumber",
            "writeOffDate",
            "grossAmount",
            "expenseAccountNo",
            "receivableAccountNo",
            "vatAccountNo",
            "note",
        ]
        .contains(&k.as_str())
            || v.is_null()
        {
            return Err(refused("unknown or null payload field"));
        }
        match k.as_str() {
            "invoiceDocumentId" => {
                if !v
                    .as_i64()
                    .is_some_and(|n| (1..=9_007_199_254_740_991).contains(&n))
                {
                    return Err(refused("positive exact invoice id required"));
                }
            }
            "grossAmount" => {
                if !v.is_number() {
                    return Err(refused("numeric amount required"));
                }
            }
            _ => {
                let s = v.as_str().ok_or_else(|| refused("string required"))?;
                if s.len() > 4096
                    || s.chars().any(char::is_control)
                    || (k != "note" && s.trim().is_empty())
                {
                    return Err(refused("bounded nonblank text required"));
                }
            }
        }
    }
    if !p.contains_key("invoiceDocumentId") && !p.contains_key("invoiceNumber") {
        return Err(refused("invoice selector required"));
    }
    for k in ["writeOffDate", "expenseAccountNo"] {
        if !p.contains_key(k) {
            return Err(refused("writeOffDate and expenseAccountNo required"));
        }
    }
    Date::parse(body["payload"]["writeOffDate"].as_str().unwrap())?;
    Ok(())
}
pub fn invoice_id(tx: &Transaction<'_>, body: &Value) -> Result<i64, Error> {
    crate::invoice_lifecycle::resolve_invoice(
        tx,
        &body["payload"],
        "invoiceDocumentId",
        "invoiceNumber",
    )
}
pub fn apply(
    tx: &Transaction<'_>,
    body: &Value,
    id: i64,
    subject: &str,
    principal: PrincipalKind,
    latest: Date,
) -> Result<Value, Error> {
    validate(body)?;
    let p = &body["payload"];
    let debtor:String=tx.query_row("SELECT a.account_no FROM issued_invoice_postings p JOIN accounts a ON a.id=p.receivable_account_id WHERE p.invoice_document_id=?1",[id],|r|r.get(0)).map_err(invalid)?;
    let vat = native::historical_vat_account(tx, id)?;
    // The retained native insert control requires the current confirmed role.
    // A remap therefore needs explicit reconciliation rather than relief on an
    // account different from the original assertion.
    let target:String=tx.query_row("SELECT account_no FROM account_role_mappings WHERE role='output_vat' AND status='confirmed'",[],|r|r.get(0)).map_err(invalid)?;
    if target != vat {
        return Err(Error::new(
            "invoice_account_mismatch",
            "current output VAT role differs from original invoice; reconcile before writeoff",
        ));
    }

    for (key, actual) in [("receivableAccountNo", debtor), ("vatAccountNo", vat)] {
        if p[key].as_str().is_some_and(|s| s != actual) {
            return Err(Error::new(
                "invoice_account_mismatch",
                format!("{key} differs from original invoice posting"),
            ));
        }
    }
    let input = native::WriteOff {
        invoice_document_id: id,
        write_off_date: p["writeOffDate"].as_str().unwrap().into(),
        gross_amount: p["grossAmount"].as_number().cloned(),
        expense_account_no: p["expenseAccountNo"].as_str().unwrap().into(),
        note: p["note"].as_str().map(str::to_owned),
        confirm: true,
    };
    let mut value = native::write_off_with_principal(tx, &input, subject, latest, principal)?;
    value["entryId"] = value["journalEntryId"].clone();
    value["entryNo"] = value["journalEntryNo"].clone();
    value["openBalance"] = value["remainingPrincipal"].clone();
    let status = crate::reminder_claims::read(tx, id, &input.write_off_date)?;
    value["claimOpenBalance"] = status["claimOpenBalance"].clone();
    value["appliedRules"] = json!(["DK-INVOICE-BAD-DEBT-WRITEOFF-001", "DK-VAT-BAD-DEBT-001"]);
    value["errors"] = json!([]);
    Ok(value)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_original_loss_payload() {
        let b = json!({"payload":{"invoiceNumber":"SYN-1","writeOffDate":"2026-01-21","expenseAccountNo":"E"},"confirm":true});
        assert!(validate(&b).is_ok());
        let mut both = b.clone();
        both["payload"]["invoiceDocumentId"] = json!(1);
        assert!(validate(&both).is_ok());
        for (key, v) in [
            ("grossAmount", Value::Null),
            ("actor", json!("x")),
            ("vatAccountNo", json!("")),
        ] {
            let mut bad = b.clone();
            bad["payload"][key] = v;
            assert!(validate(&bad).is_err());
        }
        let mut b = b;
        b["confirm"] = json!(false);
        assert_eq!(validate(&b).unwrap_err().code, "confirmation_required");
    }
}
