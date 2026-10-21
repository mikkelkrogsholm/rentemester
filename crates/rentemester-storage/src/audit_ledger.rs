//! Read-only full ledger assurance, including retained native and historical evidence.
use crate::{invalid, legacy_money};
use rentemester_engine::{Error, fx::FxRate, money::Money};
use rusqlite::{Transaction, params, types::ValueRef};
use serde_json::{Value, json};
use std::collections::BTreeSet;

// Source verifyAuditChain's IMPORTED_HISTORICAL_PROGRAMS contains exactly this
// persisted internal marker. Its exception concerns document/audit completeness
// only: hashes, balances, references and reversals are still checked.
const HISTORICAL: &str = "rentemester-import-postings";
fn native(program: &str) -> bool {
    matches!(
        program,
        "rentemester-rust"
            | "service-principal"
            | "rentemester-opening-balance"
            | "rentemester-accounting-draft"
            | "direct-bank-purchase-payable-correction"
            | "rentemester-native-bank-correction"
    )
}
fn count(tx: &Transaction<'_>, sql: &str) -> Result<i64, Error> {
    tx.query_row(sql, [], |row| row.get(0)).map_err(invalid)
}
fn optional_money(value: ValueRef<'_>) -> Result<Option<Money>, Error> {
    if value == ValueRef::Null {
        Ok(None)
    } else {
        legacy_money(value).map(Some)
    }
}
fn rate(value: ValueRef<'_>) -> Result<Option<String>, Error> {
    let text = match value {
        ValueRef::Null => return Ok(None),
        ValueRef::Integer(value) => value.to_string(),
        ValueRef::Real(value) if value.is_finite() => {
            ryu_js::Buffer::new().format_finite(value).to_owned()
        }
        _ => return Err(invalid("unsupported reversal rate storage")),
    };
    let unsigned = text.strip_prefix('-').unwrap_or(&text);
    match FxRate::parse(unsigned) {
        Ok(value) => Ok(Some(format!(
            "{}{}",
            if text.starts_with('-') { "-" } else { "" },
            value.decimal()
        ))),
        Err(_) if Money::parse(unsigned)? == Money::default() => Ok(Some("0.000000".into())),
        Err(error) => Err(error),
    }
}
fn reversal_metadata(tx: &Transaction<'_>, errors: &mut Vec<Value>) -> Result<(), Error> {
    let mut q=tx.prepare("SELECT r.id,r.document_id,o.document_id,r.source_bank_transaction_id,o.source_bank_transaction_id,r.currency,o.currency,r.amount_foreign,o.amount_foreign,r.amount_dkk,o.amount_dkk,r.fx_rate_to_dkk,o.fx_rate_to_dkk FROM journal_entries r JOIN journal_entries o ON o.id=r.reversal_of_entry_id ORDER BY r.id").map_err(invalid)?;
    let mut rows = q.query([]).map_err(invalid)?;
    while let Some(row) = rows.next().map_err(invalid)? {
        let id: i64 = row.get(0).map_err(invalid)?;
        let mut fields = Vec::new();
        for (name, a, b) in [("document_id", 1, 2), ("source_bank_transaction_id", 3, 4)] {
            if row.get::<_, Option<i64>>(a).map_err(invalid)?
                != row.get::<_, Option<i64>>(b).map_err(invalid)?
            {
                fields.push(name);
            }
        }
        if row
            .get::<_, String>(5)
            .map_err(invalid)?
            .trim()
            .to_uppercase()
            != row
                .get::<_, String>(6)
                .map_err(invalid)?
                .trim()
                .to_uppercase()
        {
            fields.push("currency");
        }
        for (name, a, b) in [("amount_foreign", 7, 8), ("amount_dkk", 9, 10)] {
            if optional_money(row.get_ref(a).map_err(invalid)?)?
                != optional_money(row.get_ref(b).map_err(invalid)?)?
            {
                fields.push(name);
            }
        }
        if rate(row.get_ref(11).map_err(invalid)?)? != rate(row.get_ref(12).map_err(invalid)?)? {
            fields.push("fx_rate_to_dkk");
        }
        if !fields.is_empty() {
            errors.push(json!({"code":"REVERSAL_METADATA_MISMATCH","entryId":id,"fields":fields}));
        }
    }
    Ok(())
}
fn receipts(tx: &Transaction<'_>, errors: &mut Vec<Value>) -> Result<BTreeSet<i64>, Error> {
    // Imported mappings/boundaries retain their original identities; this read
    // validates their existing contract without adopting or rewriting anything.
    if let Err(error) = crate::receipt_reconciliation::inventory(tx) {
        errors.push(json!({"code":"INVALID_IMPORTED_RECEIPT_HISTORY","error":error.code,"message":error.message}));
    }
    let ledger: String = tx
        .query_row(
            "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
            [],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    let mut q=tx.prepare("SELECT t.id,t.ledger_uuid,t.operation,t.client_key_hash,t.payload_hash,t.principal_kind,t.principal_subject_id,t.created_at,t.expires_at,o.outcome_json,o.created_at FROM mutation_idempotency_tombstones t LEFT JOIN mutation_idempotency_outcomes o ON o.tombstone_id=t.id WHERE t.operation IN ('journal_post','journal_reverse','opening_balance_post') ORDER BY t.id").map_err(invalid)?;
    let mut rows = q.query([]).map_err(invalid)?;
    let mut referenced = BTreeSet::new();
    while let Some(row) = rows.next().map_err(invalid)? {
        let id: i64 = row.get(0).map_err(invalid)?;
        let receipt_ledger: String = row.get(1).map_err(invalid)?;
        let operation: String = row.get(2).map_err(invalid)?;
        let key: String = row.get(3).map_err(invalid)?;
        let payload: String = row.get(4).map_err(invalid)?;
        let kind: String = row.get(5).map_err(invalid)?;
        let subject: String = row.get(6).map_err(invalid)?;
        let created: String = row.get(7).map_err(invalid)?;
        let expires: String = row.get(8).map_err(invalid)?;
        let outcome: Option<String> = row.get(9).map_err(invalid)?;
        let outcome_at: Option<String> = row.get(10).map_err(invalid)?;
        let hex = |v: &str| {
            v.len() == 64
                && v.bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        };
        let period = crate::cookie_session::timestamp(&created)
            .ok()
            .zip(crate::cookie_session::timestamp(&expires).ok())
            .and_then(|(a, b)| b.checked_sub(a));
        if id <= 0
            || receipt_ledger != ledger
            || !hex(&key)
            || !hex(&payload)
            || !matches!(kind.as_str(), "user" | "service-account")
            || subject.is_empty()
            || subject.len() > 256
            || subject.chars().any(char::is_control)
            || period != Some(30 * 86_400_000)
        {
            errors.push(json!({"code":"INVALID_NATIVE_RECEIPT","receiptId":id}));
        }
        let audits:i64=tx.query_row("SELECT count(*) FROM audit_log WHERE event_type='idempotency_original' AND entity_type='idempotency_receipt' AND entity_id=?1 AND message=?2 AND length(actor)>0",params![id.to_string(),format!("Recorded idempotent {operation} outcome")],|r|r.get(0)).map_err(invalid)?;
        if audits != 1 {
            errors.push(json!({"code":"MISSING_OR_DUPLICATE_RECEIPT_AUDIT","receiptId":id}));
        }
        if let Some(raw) = outcome {
            let result = serde_json::from_str::<Value>(&raw).ok();
            let valid = result.as_ref().is_some_and(|result| {
                raw.len() <= 65536
                    && outcome_at.as_deref() == Some(created.as_str())
                    && crate::idempotency::outcome_json(result).is_ok_and(|v| v == raw)
                    && crate::receipt_reconciliation::verify_result(tx, &operation, result).is_ok()
            });
            if !valid {
                errors.push(json!({"code":"INVALID_NATIVE_RECEIPT_OUTCOME","receiptId":id}));
            }
            if let Some(result) = result {
                let entry = if operation == "opening_balance_post" {
                    result["openingBalance"]["journalEntryId"].as_i64()
                } else {
                    result["entryId"].as_i64()
                };
                if let Some(entry) = entry {
                    referenced.insert(entry);
                }
            }
        } else if outcome_at.is_some() {
            errors.push(json!({"code":"INVALID_NATIVE_RECEIPT_OUTCOME","receiptId":id}));
        }
        // Pruned outcome bytes are permitted by the durable receipt contract;
        // their protected tombstone and original audit are still checked above.
    }
    // An original receipt audit cannot survive without its immutable tombstone.
    let mut q=tx.prepare("SELECT a.id FROM audit_log a LEFT JOIN mutation_idempotency_tombstones t ON a.entity_id=CAST(t.id AS TEXT) WHERE a.event_type='idempotency_original' AND (a.entity_type!='idempotency_receipt' OR t.id IS NULL) ORDER BY a.id").map_err(invalid)?;
    for id in q.query_map([], |r| r.get::<_, i64>(0)).map_err(invalid)? {
        errors.push(json!({"code":"ORPHAN_RECEIPT_AUDIT","auditId":id.map_err(invalid)?}));
    }
    Ok(referenced)
}
/// A deterministic observation on the admitted caller-owned transaction; no writes or paths.
pub fn verify(tx: &Transaction<'_>) -> Result<Value, Error> {
    let mut errors = crate::periods::audit::structural(tx, false)?;
    errors.extend(crate::periods::audit::log_integrity(tx)?);
    reversal_metadata(tx, &mut errors)?;
    let mut audit=tx.prepare("SELECT a.id FROM audit_log a LEFT JOIN journal_entries j ON a.entity_id=CAST(j.id AS TEXT) WHERE a.event_type IN ('journal_post','journal_reverse') AND (a.entity_type!='journal_entry' OR j.id IS NULL OR (a.event_type='journal_post' AND j.reversal_of_entry_id IS NOT NULL) OR (a.event_type='journal_reverse' AND j.reversal_of_entry_id IS NULL)) ORDER BY a.id").map_err(invalid)?;
    for id in audit
        .query_map([], |r| r.get::<_, i64>(0))
        .map_err(invalid)?
    {
        errors.push(
            json!({"code":"ORPHAN_OR_MISMATCHED_JOURNAL_AUDIT","auditId":id.map_err(invalid)?}),
        );
    }
    drop(audit);
    let native_references = receipts(tx, &mut errors)?;
    let mut q=tx.prepare("SELECT j.id,j.created_by_program,j.created_by,j.reversal_of_entry_id,j.document_id,EXISTS(SELECT 1 FROM journal_lines l JOIN accounts a ON a.id=l.account_id WHERE l.journal_entry_id=j.id AND a.type IN ('expense','income')) FROM journal_entries j ORDER BY j.id").map_err(invalid)?;
    let mut rows = q.query([]).map_err(invalid)?;
    while let Some(row) = rows.next().map_err(invalid)? {
        let id: i64 = row.get(0).map_err(invalid)?;
        let program: String = row.get(1).map_err(invalid)?;
        let actor: String = row.get(2).map_err(invalid)?;
        let reversal: Option<i64> = row.get(3).map_err(invalid)?;
        let document: Option<i64> = row.get(4).map_err(invalid)?;
        let requires_document: bool = row.get(5).map_err(invalid)?;
        if requires_document && program != HISTORICAL && document.is_none() {
            errors.push(json!({"code":"MISSING_JOURNAL_DOCUMENT","entryId":id}));
        }
        // Legacy verifyAuditChain deliberately disables universal audit cross-
        // checks for migration/raw fixtures. Known native programs and durable
        // native receipt references retain the stronger atomic-audit invariant.
        if native(&program) || native_references.contains(&id) {
            let expected = if reversal.is_some() {
                "journal_reverse"
            } else {
                "journal_post"
            };
            let audit_actor = if actor == "system" && program == "system" {
                "system".into()
            } else {
                format!("{actor} via {program}")
            };
            let matches:i64=tx.query_row("SELECT count(*) FROM audit_log WHERE event_type=?1 AND entity_type='journal_entry' AND entity_id=?2 AND actor=?3",params![expected,id.to_string(),audit_actor],|r|r.get(0)).map_err(invalid)?;
            if matches != 1 {
                errors
                    .push(json!({"code":"MISSING_OR_DUPLICATE_NATIVE_JOURNAL_AUDIT","entryId":id}));
            }
        }
    }
    for (code, result) in [
        (
            "INVALID_NATIVE_DINERO_HISTORY",
            crate::dinero_import::validate_history(tx),
        ),
        (
            "INVALID_CLOSED_HISTORY",
            crate::archive_import::validate_history(tx),
        ),
        (
            "INVALID_OPENING_HISTORY",
            crate::opening_balance::validate_history(tx).map(|_| ()),
        ),
    ] {
        if let Err(error) = result {
            errors.push(json!({"code":code,"error":error.code,"message":error.message}));
        }
    }
    errors.sort_by_cached_key(Value::to_string);
    let checked = json!({
        "entries":count(tx,"SELECT count(*) FROM journal_entries")?,
        "lines":count(tx,"SELECT count(*) FROM journal_lines")?,
        "reversals":count(tx,"SELECT count(*) FROM journal_entries WHERE reversal_of_entry_id IS NOT NULL")?,
        "auditRows":count(tx,"SELECT count(*) FROM audit_log")?,
        "journalSequences":count(tx,"SELECT count(*) FROM sequences WHERE kind='journal_entry'")?,
        "historicalEntries":count(tx,"SELECT count(*) FROM journal_entries WHERE created_by_program='rentemester-import-postings'")?,
        "archiveYears":count(tx,"SELECT count(*) FROM import_archive_years")?,
        "archivePostings":count(tx,"SELECT count(*) FROM import_archive_postings")?,
        "openingBalances":count(tx,"SELECT count(*) FROM opening_balances")?,
        "journalReceipts":count(tx,"SELECT count(*) FROM mutation_idempotency_tombstones WHERE operation IN ('journal_post','journal_reverse','opening_balance_post')")?
    });
    Ok(json!({"available":true,"ok":errors.is_empty(),"checked":checked,"errors":errors}))
}
#[cfg(test)]
mod tests;
