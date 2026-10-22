//! Original claim bindings; every financial operation remains owned by the native writers.
use crate::{idempotency::PrincipalKind, invalid, invoice_lifecycle};
use rentemester_engine::{Error, fiscal_year::Date};
use rusqlite::{OptionalExtension, Transaction};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Remind,
    PostReminder,
    Compensation,
    ClaimCompensation,
    PostCompensation,
    Interest,
    ClaimInterest,
    PostInterest,
    InterestCorrection,
    PostInterestCorrection,
    SettleClaimBank,
}
impl Kind {
    pub fn readonly(self) -> bool {
        matches!(
            self,
            Self::Compensation | Self::Interest | Self::InterestCorrection
        )
    }
    pub fn registration(self) -> bool {
        matches!(
            self,
            Self::Remind | Self::ClaimCompensation | Self::ClaimInterest
        )
    }
}
fn input_error(message: impl Into<String>) -> Error {
    Error::new("invoice_claims_input_invalid", message)
}
pub fn validate(kind: Kind, body: &Value) -> Result<(), Error> {
    let o = body
        .as_object()
        .ok_or_else(|| input_error("object required"))?;
    let extra: &[&str] = match kind {
        Kind::Remind => &["date", "fee", "note"],
        Kind::PostReminder => &[
            "reminderId",
            "date",
            "receivableAccountNo",
            "reminderIncomeAccountNo",
        ],
        Kind::Compensation => &["asOf", "amountDkk"],
        Kind::ClaimCompensation => &["asOf", "amountDkk", "note"],
        Kind::PostCompensation => &["date", "receivableAccountNo", "compensationIncomeAccountNo"],
        Kind::Interest => &["asOf", "referenceRate"],
        Kind::ClaimInterest => &["asOf", "referenceRate", "note"],
        Kind::PostInterest => &[
            "date",
            "claimId",
            "receivableAccountNo",
            "interestIncomeAccountNo",
        ],
        Kind::InterestCorrection => &[],
        Kind::PostInterestCorrection => &["date", "reason"],
        Kind::SettleClaimBank => &["payload"],
    };
    for (key, v) in o {
        let allowed = extra.contains(&key.as_str())
            || key == "confirm" && !kind.readonly()
            || matches!(key.as_str(), "documentId" | "invoiceNumber")
                && kind != Kind::SettleClaimBank;
        if !allowed {
            return Err(input_error("unknown claim field"));
        }
        check(key, v)?;
    }
    if !kind.readonly() && body["confirm"] != true {
        return Err(Error::new("confirmation_required", "confirm:true required"));
    }
    let p = if kind == Kind::SettleClaimBank {
        let p = body["payload"]
            .as_object()
            .ok_or_else(|| input_error("claim bank payload required"))?;
        for (key, v) in p {
            if ![
                "invoiceDocumentId",
                "invoiceNumber",
                "bankTransactionId",
                "bankTransactionReference",
                "paymentDate",
                "amount",
                "bankAccountNo",
                "receivableAccountNo",
            ]
            .contains(&key.as_str())
            {
                return Err(input_error("unknown claim bank field"));
            }
            check(key, v)?;
        }
        if !p.contains_key("bankTransactionId") && !p.contains_key("bankTransactionReference") {
            return Err(input_error("exact bank selector required"));
        }
        &body["payload"]
    } else {
        body
    };
    let id = if kind == Kind::SettleClaimBank {
        "invoiceDocumentId"
    } else {
        "documentId"
    };
    if p.get(id).is_none() && p.get("invoiceNumber").is_none() {
        return Err(input_error("invoice selector required"));
    }
    let required: &[&str] = match kind {
        Kind::Remind => &["date"],
        Kind::Compensation | Kind::ClaimCompensation | Kind::Interest | Kind::ClaimInterest => {
            &["asOf"]
        }
        Kind::PostReminder => &["receivableAccountNo", "reminderIncomeAccountNo"],
        Kind::PostCompensation => &["receivableAccountNo", "compensationIncomeAccountNo"],
        Kind::PostInterest => &["receivableAccountNo", "interestIncomeAccountNo"],
        Kind::PostInterestCorrection => &["reason"],
        _ => &[],
    };
    for key in required {
        if body.get(*key).is_none() {
            return Err(input_error(format!("{key} required")));
        }
    }
    Ok(())
}
fn check(key: &str, v: &Value) -> Result<(), Error> {
    if v.is_null() {
        return Err(input_error("null claim fields are not accepted"));
    }
    if key == "payload" {
        return if v.is_object() {
            Ok(())
        } else {
            Err(input_error("object payload required"))
        };
    }
    if key == "confirm" {
        return if v.is_boolean() {
            Ok(())
        } else {
            Err(input_error("boolean confirmation required"))
        };
    }
    if [
        "documentId",
        "invoiceDocumentId",
        "bankTransactionId",
        "reminderId",
        "claimId",
    ]
    .contains(&key)
    {
        return if v
            .as_i64()
            .is_some_and(|n| (1..=9_007_199_254_740_991).contains(&n))
        {
            Ok(())
        } else {
            Err(input_error("positive exact id required"))
        };
    }
    if ["fee", "amountDkk", "referenceRate", "amount"].contains(&key) {
        return if v.is_number() {
            Ok(())
        } else {
            Err(input_error("numeric claim amount/rate required"))
        };
    }
    let s = v
        .as_str()
        .ok_or_else(|| input_error("claim text must be string"))?;
    if s.len() > 4096 || s.chars().any(char::is_control) || (key != "note" && s.trim().is_empty()) {
        return Err(input_error("bounded nonblank claim text required"));
    }
    if ["date", "asOf", "paymentDate"].contains(&key) {
        Date::parse(s)?;
    }
    Ok(())
}
pub fn invoice_id(tx: &Transaction<'_>, kind: Kind, body: &Value) -> Result<i64, Error> {
    invoice_lifecycle::resolve_invoice(
        tx,
        if kind == Kind::SettleClaimBank {
            &body["payload"]
        } else {
            body
        },
        if kind == Kind::SettleClaimBank {
            "invoiceDocumentId"
        } else {
            "documentId"
        },
        "invoiceNumber",
    )
}
fn decode<T: DeserializeOwned>(v: Value) -> Result<T, Error> {
    serde_json::from_value(v).map_err(|_| input_error("invalid native claim mapping"))
}
fn checked(value: Value) -> Result<Value, Error> {
    if value["ok"] == false {
        return Err(Error::new(
            "invoice_claims_refused",
            value["errors"].to_string(),
        ));
    }
    Ok(value)
}
/// Called only inside the admitted sales transaction after immutable sources are checked.
pub fn apply(
    tx: &Transaction<'_>,
    kind: Kind,
    body: &Value,
    id: i64,
    subject: &str,
    principal: PrincipalKind,
    latest: Date,
) -> Result<Value, Error> {
    validate(kind, body)?;
    let mut n = json!({"invoiceDocumentId":id});
    if !kind.readonly() {
        n["confirm"] = json!(true);
    }
    for (from, to) in [
        ("date", "transactionDate"),
        ("asOf", "asOfDate"),
        ("amountDkk", "compensationAmountDkk"),
        ("referenceRate", "referenceRatePercent"),
        ("note", "note"),
        ("claimId", "claimId"),
        ("reason", "reason"),
        ("receivableAccountNo", "receivableAccountNo"),
        ("reminderIncomeAccountNo", "reminderIncomeAccountNo"),
        ("compensationIncomeAccountNo", "compensationIncomeAccountNo"),
        ("interestIncomeAccountNo", "interestIncomeAccountNo"),
    ] {
        if let Some(v) = body.get(from) {
            n[to] = v.clone();
        }
    }
    let mut value = match kind {
        Kind::Remind => {
            n.as_object_mut().unwrap().remove("transactionDate");
            n["reminderDate"] = body["date"].clone();
            n["feeAmount"] = body.get("fee").cloned().unwrap_or(json!(100));
            crate::reminder_claims::register_with_principal(tx, &decode(n)?, subject, principal)?
        }
        Kind::PostReminder => {
            let reminder = if let Some(id) = body["reminderId"].as_i64() {
                id
            } else {
                tx.query_row("SELECT id FROM invoice_reminders WHERE invoice_document_id=?1 ORDER BY reminder_date,id LIMIT 1",[id],|r|r.get(0)).optional().map_err(invalid)?.ok_or_else(||input_error("registered reminder not found"))?
            };
            n["reminderId"] = json!(reminder);
            crate::reminder_claims::post_with_principal(
                tx,
                &decode(n)?,
                subject,
                latest,
                principal,
            )?
        }
        Kind::Compensation => crate::invoice_compensation::calculate(tx, &decode(n)?)?,
        Kind::ClaimCompensation => crate::invoice_compensation::register_with_principal(
            tx,
            &decode(n)?,
            subject,
            principal,
        )?,
        Kind::PostCompensation => crate::invoice_compensation::post_with_principal(
            tx,
            &decode(n)?,
            subject,
            latest,
            principal,
        )?,
        Kind::Interest => crate::invoice_interest::calculate(tx, &decode(n)?)?,
        Kind::ClaimInterest => {
            let mut preview = n.clone();
            preview.as_object_mut().unwrap().remove("confirm");
            preview.as_object_mut().unwrap().remove("note");
            let before = checked(crate::invoice_interest::calculate(tx, &decode(preview)?)?)?;
            let mut after = checked(crate::invoice_interest::register_with_principal(
                tx,
                &decode(n)?,
                subject,
                principal,
            )?)?;
            // Original result reports the incremental amount just registered, not a second zero calculation.
            for key in [
                "accruedInterestAmount",
                "priorClaimedInterest",
                "totalInterestToDate",
                "overClaimedInterest",
                "claimableDays",
                "interestFromDate",
            ] {
                if let Some(v) = before.get(key) {
                    after[key] = v.clone();
                }
            }
            after["claimDate"] = body["asOf"].clone();
            after
        }
        Kind::PostInterest => crate::invoice_interest::post_with_principal(
            tx,
            &decode(n)?,
            subject,
            latest,
            principal,
        )?,
        Kind::InterestCorrection => crate::invoice_interest::correction(tx, &decode(n)?)?,
        Kind::PostInterestCorrection => {
            let before = checked(crate::invoice_interest::correction(
                tx,
                &crate::invoice_interest::Correction {
                    invoice_document_id: id,
                },
            )?)?;
            let mut after = checked(crate::invoice_interest::post_correction_with_principal(
                tx,
                &decode(n)?,
                subject,
                latest,
                principal,
            )?)?;
            after["correctedAmount"] = before["overClaimedAmount"].clone();
            after
        }
        Kind::SettleClaimBank => {
            let p = &body["payload"];
            let bank = invoice_lifecycle::bank_id(tx, p)?;
            // Principal account assertion is inappropriate for independently allocated claim accounts.
            if let Some(expected) = p["bankAccountNo"].as_str() {
                let actual:String=tx.query_row("SELECT a.ledger_account_no FROM bank_transactions b JOIN bank_accounts a ON a.id=b.bank_account_id WHERE b.id=?1",[bank],|r|r.get(0)).map_err(invalid)?;
                if expected.trim() != actual {
                    return Err(Error::new(
                        "invoice_account_mismatch",
                        "bankAccountNo differs from imported bank ledger mapping",
                    ));
                }
            }
            let input = crate::reminder_claims::Settle {
                invoice_document_id: id,
                bank_transaction_id: bank,
                payment_date: p["paymentDate"].as_str().map(str::to_owned),
                amount: p["amount"].as_number().cloned(),
                receivable_account_no: p["receivableAccountNo"].as_str().map(str::to_owned),
                confirm: true,
            };
            crate::reminder_claims::settle_with_principal(tx, &input, subject, latest, principal)?
        }
    };
    value = checked(value)?;
    if matches!(
        kind,
        Kind::Remind
            | Kind::ClaimCompensation
            | Kind::ClaimInterest
            | Kind::PostReminder
            | Kind::PostCompensation
            | Kind::PostInterest
            | Kind::PostInterestCorrection
            | Kind::SettleClaimBank
    ) {
        let status = crate::reminder_claims::read(tx, id, &latest.to_string())?;
        for key in [
            "invoiceNumber",
            "principalCurrency",
            "claimCurrency",
            "claimOpenBalance",
            "registeredClaimBalanceDkk",
            "totalReminderFees",
        ] {
            if let Some(v) = status.get(key) {
                value[key] = v.clone();
            }
        }
    }
    if kind == Kind::ClaimCompensation {
        value["reason"] = json!("registered");
        value["eligible"] = json!(true);
    }
    if kind == Kind::PostReminder
        && let Some(reminder) = value["reminderId"].as_i64()
    {
        let (date, fee): (String, Value) = tx
            .query_row(
                "SELECT reminder_date,fee_amount FROM invoice_reminders WHERE id=?1",
                [reminder],
                |r| {
                    Ok((
                        r.get(0)?,
                        rentemester_engine::sales_invoice::wire(
                            &crate::legacy_money(r.get_ref(1)?)
                                .map_err(|_| rusqlite::Error::InvalidQuery)?,
                        )
                        .map_err(|_| rusqlite::Error::InvalidQuery)?,
                    ))
                },
            )
            .map_err(invalid)?;
        value["reminderDate"] = json!(date);
        value["feeAmount"] = fee;
    }
    if kind == Kind::PostInterest
        && let Some(claim) = value["claimId"].as_i64()
    {
        let (date, amount): (String, Value) = tx
            .query_row(
                "SELECT claim_date,amount_dkk FROM invoice_interest_claims WHERE id=?1",
                [claim],
                |r| {
                    Ok((
                        r.get(0)?,
                        rentemester_engine::sales_invoice::wire(
                            &crate::legacy_money(r.get_ref(1)?)
                                .map_err(|_| rusqlite::Error::InvalidQuery)?,
                        )
                        .map_err(|_| rusqlite::Error::InvalidQuery)?,
                    ))
                },
            )
            .map_err(invalid)?;
        value["claimDate"] = json!(date);
        value["accruedInterestAmount"] = amount;
    }
    for (from, to) in [("journalEntryId", "entryId"), ("journalEntryNo", "entryNo")] {
        if let Some(v) = value.get(from).cloned() {
            value[to] = v;
        }
    }
    Ok(invoice_lifecycle::envelope(
        invoice_lifecycle::journal_receipt(tx, value)?,
    ))
}
#[cfg(test)]
mod tests;
