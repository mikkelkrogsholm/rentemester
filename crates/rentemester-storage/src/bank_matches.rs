//! Deterministic, read-only bank advice. Scores are ranking signals, not probabilities.
use crate::{invalid, legacy_money};
use rentemester_engine::{Error, money::Money, sales_invoice::wire};
use rusqlite::Transaction;
use serde_json::{Value, json};
#[derive(Default)]
pub struct Input {
    pub bank_transaction_id: Option<i64>,
    pub max: Option<usize>,
}
struct Bank {
    id: i64,
    date: String,
    text: String,
    amount: Money,
    currency: String,
    reference: Option<String>,
    combined: String,
}
struct Doc {
    id: i64,
    kind: String,
    no: Option<String>,
    number: Option<String>,
    date: Option<String>,
    gross: Money,
    currency: String,
    name: Option<String>,
    payment: Option<String>,
    payload: Option<String>,
    hash: String,
    original: Option<i64>,
    unposted: bool,
}
const STOP: &[&str] = &[
    "APS", "A/S", "AS", "IVS", "P/S", "PS", "K/S", "KS", "I/S", "IS", "AMBA", "FMBA", "SMBA",
    "GMBH", "LTD", "INC", "PLC", "LLC", "AB", "OY", "DKK", "EUR", "USD", "SEK", "NOK", "GBP",
    "FOR", "OG", "THE", "AND", "VED", "MED", "TIL", "FRA", "DEN", "DET",
];
fn tokens(value: &str) -> Vec<String> {
    let mut result = vec![];
    for token in value
        .to_uppercase()
        .split(|c| !rentemester_engine::bank::match_token_character(c))
    {
        if token.encode_utf16().count() >= 3
            && !STOP.contains(&token)
            && !result.iter().any(|v| v == token)
        {
            result.push(token.to_owned());
        }
    }
    result
}
fn overlap(bank: &str, name: Option<&str>, max: usize) -> Vec<String> {
    let left = tokens(bank);
    tokens(name.unwrap_or(""))
        .into_iter()
        .filter(|s| left.contains(s))
        .take(max)
        .collect()
}
fn number(value: &Money) -> String {
    let text = value.decimal();
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}
fn positive(value: &Money) -> bool {
    !value.is_negative() && value != &Money::default()
}
fn absolute(value: &Money) -> Money {
    if value.is_negative() {
        Money::default().subtract(value)
    } else {
        value.clone()
    }
}
fn dated(
    bank: &Bank,
    doc: &Doc,
    limit: i64,
    label: &str,
    score: &mut i32,
    reasons: &mut Vec<String>,
    points: i32,
) {
    if let Some(date) = &doc.date
        && let Ok(days) = rentemester_engine::vat_period::date(date).and_then(|d| {
            Ok((d - rentemester_engine::vat_period::date(&bank.date)?)
                .whole_days()
                .abs())
        })
        && days <= limit
    {
        *score += points;
        reasons.push(format!("date within {days} days of {label}"));
    }
}
fn active_invoice(tx: &Transaction<'_>, id: i64) -> Result<bool, Error> {
    tx.query_row("SELECT EXISTS(SELECT 1 FROM documents d JOIN issued_invoice_postings p ON p.invoice_document_id=d.id JOIN journal_entries j ON j.id=p.journal_entry_id WHERE d.id=?1 AND d.document_type='issued_invoice' AND j.document_id=d.id AND j.transaction_date=d.invoice_date AND j.status='posted' AND j.reversal_of_entry_id IS NULL AND j.source_bank_transaction_id IS NULL AND NOT EXISTS(SELECT 1 FROM journal_entries r WHERE r.reversal_of_entry_id=j.id))",[id],|r|r.get(0)).map_err(invalid)
}
fn status(tx: &Transaction<'_>, doc: &Doc, as_of: &str) -> Result<(Money, Money), Error> {
    let payload: Value =
        serde_json::from_str(doc.payload.as_deref().unwrap_or("")).map_err(invalid)?;
    if payload["invoiceNumber"] != json!(doc.no)
        || payload["currency"].as_str().unwrap_or("DKK") != doc.currency
        || payload["issueDate"] != json!(doc.date)
        || crate::invoice_email::sha(doc.payload.as_deref().unwrap_or("").as_bytes()) != doc.hash
    {
        return Err(Error::new(
            "bank_match_evidence_required",
            "invoice snapshot differs",
        ));
    }
    let projection = rentemester_engine::sales_invoice::project(&payload)?;
    if projection.gross != doc.gross {
        return Err(Error::new(
            "bank_match_evidence_required",
            "invoice gross differs",
        ));
    }
    crate::credit_note::verify_existing(tx, doc.id)?;
    if doc.currency == "DKK" {
        let value = crate::reminder_claims::read(tx, doc.id, as_of)?;
        Ok((
            crate::invoice_fx::amount(&value["principalOpenBalance"])?,
            crate::invoice_fx::amount(&value["claimOpenBalance"])?,
        ))
    } else {
        let value =
            crate::sales::list(tx, "", as_of, doc.date.as_deref().and_then(|s| s.get(..4)))?;
        let row = value["invoices"]
            .as_array()
            .and_then(|rows| rows.iter().find(|r| r["documentId"] == doc.id))
            .ok_or_else(|| Error::new("bank_match_evidence_required", "invoice status missing"))?;
        let open = crate::invoice_fx::amount(&row["openBalance"])?;
        Ok((open.clone(), open))
    }
}
fn suggestion(kind: &str, doc: &Doc, score: i32, reasons: Vec<String>) -> Option<Value> {
    if score < 50 {
        return None;
    }
    let mut result = json!({"kind":kind,"documentId":doc.id,"invoiceNo":doc.no,"confidence":f64::from(score)/100.0,"reasons":reasons});
    result[if matches!(kind, "issued_invoice" | "credit_note_refund") {
        "customerName"
    } else {
        "supplierName"
    }] = json!(doc.name);
    Some(result)
}
fn invoice(bank: &Bank, doc: &Doc, open: &Money, claim: &Money) -> Option<Value> {
    if !positive(&bank.amount) || bank.currency != doc.currency || !positive(claim) {
        return None;
    }
    let mut score = 0;
    let mut reasons = vec![];
    let mut corroborated = false;
    if bank.amount == *claim {
        score += 60;
        reasons.push(format!(
            "amount match: {} vs claim open balance {}",
            number(&bank.amount),
            number(claim)
        ));
    } else if bank.amount == *open {
        score += 55;
        reasons.push(format!(
            "amount match: {} vs principal open balance {}",
            number(&bank.amount),
            number(open)
        ));
    }
    if let Some(no) = doc
        .no
        .as_ref()
        .filter(|no| bank.combined.contains(&no.to_uppercase()))
    {
        score += 25;
        corroborated = true;
        reasons.push(format!(
            "invoice number '{no}' found in bank text/reference"
        ));
    }
    let names = overlap(&bank.combined, doc.name.as_deref(), 3);
    if !names.is_empty() {
        score += names.len() as i32 * 5;
        corroborated |= names.len() >= 2;
        reasons.push(format!("customer token match: {}", names.join(", ")));
    }
    dated(bank, doc, 7, "invoice date", &mut score, &mut reasons, 10);
    if !corroborated && score >= 50 {
        score = 45;
        reasons.push(
            "low confidence: amount-only match, no invoice number or name corroboration".into(),
        );
    }
    suggestion("issued_invoice", doc, score, reasons)
}
fn purchase(bank: &Bank, doc: &Doc, supplier_refund: bool) -> Option<Value> {
    if bank.currency != "DKK"
        || doc.currency != "DKK"
        || !positive(&doc.gross)
        || positive(&bank.amount) != supplier_refund
        || bank.amount == Money::default()
    {
        return None;
    }
    let amount = absolute(&bank.amount);
    let mut score = 0;
    let mut reasons = vec![];
    let mut corroborated = false;
    if amount == doc.gross {
        score += if supplier_refund { 50 } else { 55 };
        reasons.push(format!(
            "amount match: {} vs purchase gross amount {}",
            number(&amount),
            number(&doc.gross)
        ));
    } else if supplier_refund {
        if amount.subtract(&doc.gross).is_negative() {
            score += 30;
            reasons.push(format!(
                "partial-refund amount: {} of purchase gross amount {}",
                number(&amount),
                number(&doc.gross)
            ));
        } else {
            return None;
        }
    }
    if let Some(no) = doc
        .no
        .as_ref()
        .filter(|no| bank.combined.contains(&no.to_uppercase()))
    {
        score += if supplier_refund { 25 } else { 20 };
        corroborated = true;
        reasons.push(format!(
            "{}invoice number '{no}' found in bank text/reference",
            if supplier_refund { "purchase " } else { "" }
        ));
    }
    let payment = overlap(&bank.combined, doc.payment.as_deref(), 2);
    if !payment.is_empty() {
        score += 10;
        corroborated = true;
        reasons.push(format!(
            "payment-details token match: {}",
            payment.join(", ")
        ));
    }
    let names = overlap(&bank.combined, doc.name.as_deref(), 3);
    if !names.is_empty() {
        score += (names.len() as i32 * 10).min(20);
        corroborated |= names.len() >= 2;
        reasons.push(format!("supplier token match: {}", names.join(", ")));
    }
    if supplier_refund {
        // Source JS \\b uses ASCII word boundaries, independently of name tokens.
        let cue = bank
            .combined
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .any(|s| {
                [
                    "KREDIT",
                    "KREDITNOTA",
                    "REFUSION",
                    "REFUND",
                    "TILBAGEBETALING",
                    "CREDIT",
                ]
                .contains(&s)
            });
        if cue {
            score += 10;
            reasons.push("bank text identifies the row as a credit-note / refund".into());
        }
        if !(corroborated && cue) && score >= 50 {
            score = 45;
            reasons
                .push("low confidence: not corroborated as a supplier credit-note refund".into());
        }
    } else {
        dated(
            bank,
            doc,
            7,
            "purchase invoice date",
            &mut score,
            &mut reasons,
            5,
        );
        if !corroborated && score >= 50 {
            score = 45;
            reasons.push(
                "low confidence: amount-only match, no invoice number or supplier corroboration"
                    .into(),
            );
        }
    }
    suggestion(
        if supplier_refund {
            "supplier_credit_refund"
        } else {
            "purchase_sale"
        },
        doc,
        score,
        reasons,
    )
}
fn refund(bank: &Bank, doc: &Doc, refundable: &Money) -> Option<Value> {
    let amount = absolute(&bank.amount);
    if !bank.amount.is_negative()
        || bank.currency != doc.currency
        || !positive(&doc.gross)
        || !positive(refundable)
        || refundable.subtract(&amount).is_negative()
    {
        return None;
    }
    let mut score = 0;
    let mut reasons = vec![];
    let mut corroborated = false;
    if amount == doc.gross {
        score += 55;
        reasons.push(format!(
            "amount match: {} vs credit-note gross amount {}",
            number(&amount),
            number(&doc.gross)
        ));
    }
    if let Some(no) = doc
        .no
        .as_ref()
        .filter(|no| bank.combined.contains(&no.to_uppercase()))
    {
        score += 25;
        corroborated = true;
        reasons.push(format!(
            "credit-note number '{no}' found in bank text/reference"
        ));
    }
    if let Some(no) = doc
        .payment
        .as_ref()
        .filter(|no| bank.combined.contains(&no.to_uppercase()))
    {
        score += 15;
        corroborated = true;
        reasons.push(format!(
            "credited invoice number '{no}' found in bank text/reference"
        ));
    }
    let names = overlap(&bank.combined, doc.name.as_deref(), 3);
    if !names.is_empty() {
        score += names.len() as i32 * 5;
        corroborated |= names.len() >= 2;
        reasons.push(format!("customer token match: {}", names.join(", ")));
    }
    dated(
        bank,
        doc,
        14,
        "credit-note date",
        &mut score,
        &mut reasons,
        10,
    );
    if !corroborated && score >= 50 {
        score = 45;
        reasons.push(
            "low confidence: amount-only match, no credit-note number or customer corroboration"
                .into(),
        );
    }
    suggestion("credit_note_refund", doc, score, reasons)
}
fn near_miss(bank: &Bank, docs: &[Doc]) -> Option<String> {
    if !bank.amount.is_negative() {
        return None;
    }
    let amount = absolute(&bank.amount);
    let matching = docs
        .iter()
        .filter(|doc| {
            matches!(doc.kind.as_str(), "purchase_sale" | "cash_register_receipt")
                && doc.unposted
                && doc.currency == bank.currency
                && doc.gross == amount
        })
        .take(2)
        .collect::<Vec<_>>();
    match matching.as_slice(){
        []=>None,[_a,_b]=>Some("Flere bilag har præcis dette beløb — beløbet alene kan ikke afgøre hvilket, så intet sikkert forslag gives. Vælg det rette bilag manuelt.".into()),
        [doc]=>Some(format!("Beløbet passer præcist til {}{}, men der mangler et fakturanummer eller navnematch for at det er et sikkert forslag. Kontrollér, at bilaget hører til, og afstem manuelt.",doc.number.as_ref().map(|s|format!("bilag {s}")).unwrap_or_else(||"et bilag".into()),doc.name.as_ref().map(|s|format!(" fra {s}")).unwrap_or_default())),_=>unreachable!()
    }
}
fn documents(tx: &Transaction<'_>) -> Result<Vec<Doc>, Error> {
    let mut query=tx.prepare("SELECT d.id,d.document_type,d.invoice_no,d.invoice_date,d.amount_inc_vat,d.currency,CASE WHEN d.document_type IN ('issued_invoice','credit_note') THEN d.recipient_name ELSE d.sender_name END,d.payment_details,d.payload_json,d.sha256_hash,(SELECT original_invoice_document_id FROM credit_note_postings WHERE credit_note_document_id=d.id),NOT EXISTS(SELECT 1 FROM journal_entries j WHERE j.document_id=d.id AND j.status IN ('posted','reversed')),d.document_no FROM documents d WHERE d.document_type IN ('issued_invoice','purchase_sale','credit_note','cash_register_receipt') ORDER BY d.id").map_err(invalid)?;
    let mut rows = query.query([]).map_err(invalid)?;
    let mut result = vec![];
    while let Some(row) = rows.next().map_err(invalid)? {
        let gross = if row.get_ref(4).map_err(invalid)? == rusqlite::types::ValueRef::Null {
            Money::default()
        } else {
            legacy_money(row.get_ref(4).map_err(invalid)?)?
        };
        result.push(Doc {
            id: row.get(0).map_err(invalid)?,
            kind: row.get(1).map_err(invalid)?,
            no: row.get(2).map_err(invalid)?,
            number: row.get(12).map_err(invalid)?,
            date: row.get(3).map_err(invalid)?,
            gross,
            currency: row.get(5).map_err(invalid)?,
            name: row.get(6).map_err(invalid)?,
            payment: row.get(7).map_err(invalid)?,
            payload: row.get(8).map_err(invalid)?,
            hash: row.get(9).map_err(invalid)?,
            original: row.get(10).map_err(invalid)?,
            unposted: row.get(11).map_err(invalid)?,
        });
    }
    for doc in &mut result {
        if doc.kind == "issued_invoice" {
            doc.name = doc
                .payload
                .as_deref()
                .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
                .and_then(|value| value["buyer"]["name"].as_str().map(str::to_owned));
        }
    }
    Ok(result)
}
pub fn suggest(tx: &Transaction<'_>, input: &Input) -> Result<Value, Error> {
    crate::validate(tx)?;
    let max = input.max.unwrap_or(5);
    let mut errors = vec![];
    if input
        .bank_transaction_id
        .is_some_and(|id| !(1..=9007199254740991).contains(&id))
    {
        errors.push("bankTransactionId must be a positive integer when present".to_owned());
    }
    if max == 0 {
        errors.push("max must be a positive integer when present".to_owned());
    }
    if !errors.is_empty() {
        return Ok(json!({"ok":false,"count":0,"rows":[],"errors":errors}));
    }
    let mut query=tx.prepare("SELECT b.id,b.transaction_date,b.text,b.amount,b.currency,b.reference,b.counterparty_name,b.message FROM bank_transactions b WHERE (?1 IS NULL OR b.id=?1) AND NOT EXISTS(SELECT 1 FROM bank_journal_reconciliations r WHERE r.bank_transaction_id=b.id) ORDER BY b.transaction_date DESC,b.id DESC").map_err(invalid)?;
    let banks = query
        .query_map([input.bank_transaction_id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(4)?,
                r.get::<_, Option<String>>(5)?,
                r.get::<_, Option<String>>(6)?,
                r.get::<_, Option<String>>(7)?,
            ))
        })
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    if banks.is_empty()
        && let Some(id) = input.bank_transaction_id
    {
        return Ok(
            json!({"ok":false,"count":0,"rows":[],"errors":[format!("unmatched bank transaction {id} does not exist")]}),
        );
    }
    let docs = documents(tx)?;
    let mut result = vec![];
    for (id, date, text, currency, reference, counterparty, message) in banks {
        let amount = tx
            .query_row(
                "SELECT amount FROM bank_transactions WHERE id=?1",
                [id],
                |r| legacy_money(r.get_ref(0)?).map_err(|_| rusqlite::Error::InvalidQuery),
            )
            .map_err(invalid)?;
        let combined = [
            Some(text.as_str()),
            reference.as_deref(),
            counterparty.as_deref(),
            message.as_deref(),
        ]
        .into_iter()
        .flatten()
        .filter(|s| !s.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_uppercase();
        let bank = Bank {
            id,
            date,
            text,
            amount,
            currency,
            reference,
            combined,
        };
        let mut candidates = vec![];
        for doc in docs.iter().filter(|d| d.kind == "issued_invoice") {
            if active_invoice(tx, doc.id)?
                && let Ok((open, claim)) = status(tx, doc, &bank.date)
                && let Some(value) = invoice(&bank, doc, &open, &claim)
            {
                candidates.push(value);
            }
        }
        for doc in docs
            .iter()
            .filter(|d| d.kind == "purchase_sale" && d.unposted)
        {
            if let Some(value) = purchase(&bank, doc, false) {
                candidates.push(value);
            }
        }
        for doc in docs.iter().filter(|d| d.kind == "credit_note") {
            let Some(original) = doc.original.and_then(|id| docs.iter().find(|d| d.id == id))
            else {
                continue;
            };
            let active:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM credit_note_postings p JOIN journal_entries j ON j.id=p.journal_entry_id WHERE p.credit_note_document_id=?1 AND j.document_id=?1 AND j.status='posted' AND j.reversal_of_entry_id IS NULL AND NOT EXISTS(SELECT 1 FROM journal_entries r WHERE r.reversal_of_entry_id=j.id))",[doc.id],|r|r.get(0)).map_err(invalid)?;
            if !active
                || !active_invoice(tx, original.id)?
                || crate::invoice_email::sha(doc.payload.as_deref().unwrap_or("").as_bytes())
                    != doc.hash
            {
                continue;
            }
            if let Ok((open, _)) = status(tx, original, &bank.date)
                && let Some(value) = refund(&bank, doc, &Money::default().subtract(&open))
            {
                candidates.push(value);
            }
        }
        for doc in docs.iter().filter(|d| d.kind == "purchase_sale") {
            if let Some(value) = purchase(&bank, doc, true) {
                candidates.push(value);
            }
        }
        candidates.sort_by(|a, b| {
            b["confidence"]
                .as_f64()
                .unwrap()
                .total_cmp(&a["confidence"].as_f64().unwrap())
                .then_with(|| a["documentId"].as_i64().cmp(&b["documentId"].as_i64()))
        });
        candidates.truncate(max);
        let unsafe_reason = if candidates.is_empty() {
            near_miss(&bank, &docs)
        } else {
            None
        };
        result.push(json!({"bankTransactionId":bank.id,"date":bank.date,"text":bank.text,"amount":wire(&bank.amount)?,"currency":bank.currency,"reference":bank.reference,"suggestions":candidates,"unsafeMatchReason":unsafe_reason}));
    }
    Ok(json!({"ok":true,"count":result.len(),"rows":result,"errors":[]}))
}
#[cfg(test)]
mod tests;
