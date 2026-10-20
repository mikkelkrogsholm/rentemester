//! Opted-in browser post recovery. Facts accompany the original atomic receipt;
//! old CLI/MCP/historical receipts are never retroactively treated as interrupted.
use rentemester_engine::Error;
use rusqlite::{Connection, Transaction, params};
use serde_json::{Value, json};
const PENDING: &str = "manual_post_recovery_pending";
const ACK: &str = "manual_post_recovery_acknowledged";
fn failure(_: impl std::fmt::Display) -> Error {
    Error::new(
        "manual_recovery_unavailable",
        "Posting receipt recovery unavailable; inspect the journal before continuing",
    )
}
fn own(db: &Connection, subject: &str, id: i64) -> Result<(), Error> {
    if id <= 0 || id > 9_007_199_254_740_991 || subject.is_empty() {
        return Err(failure("identity"));
    }
    let n: i64 = db.query_row("SELECT count(*) FROM mutation_idempotency_tombstones t JOIN ledger_identity l ON l.id=1 AND l.ledger_uuid=t.ledger_uuid WHERE t.id=?1 AND t.operation='journal_post' AND t.principal_kind='user' AND t.principal_subject_id=?2",params![id,subject],|r|r.get(0)).map_err(failure)?;
    if n != 1 {
        return Err(failure("receipt"));
    }
    Ok(())
}
fn fact(tx: &Transaction<'_>, event: &str, subject: &str, id: i64) -> Result<(), Error> {
    crate::idempotency::writer(tx)?;
    own(tx, subject, id)?;
    let message =
        json!({"format":"rentemester-manual-post-recovery/1","receiptId":id,"subject":subject});
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES(?1,'idempotency_receipt',?2,?3,?4)",params![event,id.to_string(),message.to_string(),format!("user:{subject}")]).map_err(failure)?;
    Ok(())
}
/// Requires an existing current-principal receipt. Called in its posting transaction only.
pub fn mark(tx: &Transaction<'_>, subject: &str, id: i64) -> Result<(), Error> {
    fact(tx, PENDING, subject, id)
}
pub fn inventory(db: &Connection, subject: &str) -> Result<Value, Error> {
    // Validate relevant history, including acknowledged facts; damaged facts never
    // silently disappear through a SQL anti-join. Bound returned recovery to 100.
    let mut query = db.prepare("SELECT a.id,a.event_type,a.entity_type,a.entity_id,a.message,a.actor FROM audit_log a LEFT JOIN mutation_idempotency_tombstones t ON a.entity_id=CAST(t.id AS TEXT) WHERE a.event_type IN (?1,?2) AND (a.actor=?4 OR (t.principal_kind='user' AND t.principal_subject_id=?3)) ORDER BY a.id").map_err(failure)?;
    let mut rows = query
        .query(params![PENDING, ACK, subject, format!("user:{subject}")])
        .map_err(failure)?;
    let mut pending = std::collections::BTreeSet::new();
    while let Some(row) = rows.next().map_err(failure)? {
        let event: String = row.get(1).map_err(failure)?;
        let entity: String = row.get(2).map_err(failure)?;
        let id: String = row.get(3).map_err(failure)?;
        let message: String = row.get(4).map_err(failure)?;
        let actor: String = row.get(5).map_err(failure)?;
        let canonical_id = id.clone();
        let id = id.parse::<i64>().map_err(failure)?;
        if id.to_string() != canonical_id {
            return Err(failure("noncanonical id"));
        }
        own(db, subject, id)?;
        let expected =
            json!({"format":"rentemester-manual-post-recovery/1","receiptId":id,"subject":subject});
        if entity != "idempotency_receipt"
            || actor != format!("user:{subject}")
            || serde_json::from_str::<Value>(&message).map_err(failure)? != expected
        {
            return Err(failure("history"));
        }
        if event == PENDING {
            if !pending.insert(id) {
                return Err(failure("duplicate pending"));
            }
        } else if !pending.remove(&id) {
            return Err(failure("ack without pending"));
        }
    }
    if pending.len() > 100 {
        return Err(failure("bounded recovery"));
    }
    let mut receipts = Vec::new();
    for id in pending {
        own(db, subject, id)?;
        let (created,result):(String,Option<String>)=db.query_row("SELECT t.created_at,o.outcome_json FROM mutation_idempotency_tombstones t LEFT JOIN mutation_idempotency_outcomes o ON o.tombstone_id=t.id WHERE t.id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?))).map_err(failure)?;
        let result = result
            .map(|s| serde_json::from_str::<Value>(&s).map_err(failure))
            .transpose()?;
        // Expired replay material is explicitly unresolved, never permission to repost.
        receipts.push(json!({"receiptId":id,"createdAt":created,"resultAvailable":result.is_some(),"result":result}));
    }
    Ok(json!({"receipts":receipts}))
}
pub fn require_clear(db: &Connection, subject: &str) -> Result<(), Error> {
    if inventory(db, subject)?["receipts"]
        .as_array()
        .is_some_and(|r| r.is_empty())
    {
        Ok(())
    } else {
        Err(Error::new(
            "manual_recovery_required",
            "Kontrollér den tidligere bogføringskvittering, før du opretter en ny postering.",
        ))
    }
}
/// Existing current-principal keys can only replay/refuse in idempotency::execute;
/// they cannot create a new financial effect. Preserve exact-envelope recovery.
pub fn require_clear_for_key(db: &Connection, subject: &str, key: &str) -> Result<(), Error> {
    match require_clear(db, subject) {
        Ok(()) => Ok(()),
        Err(e) if e.code == "manual_recovery_required" => {
            use sha2::{Digest, Sha256};
            crate::idempotency::validate_key(key)?;
            let hash = format!("{:x}", Sha256::digest(key.as_bytes()));
            let exists:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM mutation_idempotency_tombstones t JOIN ledger_identity l ON l.id=1 AND l.ledger_uuid=t.ledger_uuid WHERE t.client_key_hash=?1 AND t.operation='journal_post' AND t.principal_kind='user' AND t.principal_subject_id=?2)",params![hash,subject],|r|r.get(0)).map_err(failure)?;
            if exists { Ok(()) } else { Err(e) }
        }
        Err(e) => Err(e),
    }
}

/// Acknowledgement is idempotent and changes no receipt, journal or reserved key.
pub fn acknowledge(tx: &Transaction<'_>, subject: &str, id: i64) -> Result<Value, Error> {
    own(tx, subject, id)?;
    let pending = inventory(tx, subject)?;
    if pending["receipts"]
        .as_array()
        .is_some_and(|r| r.iter().any(|r| r["receiptId"] == id))
    {
        fact(tx, ACK, subject, id)?;
    }
    Ok(json!({"acknowledgedReceiptId":id}))
}
#[cfg(test)]
mod tests;
