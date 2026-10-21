//! Explicit per-receipt adoption of retained results; no alias, mutation callback or live authority.
use rentemester_engine::Error;
use rusqlite::{Connection, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
const EVENT: &str = "native_restore_receipt_reconciled";
const BOUNDARY: &str = "native_restore_receipt_boundary";
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Reconcile {
    pub receipt_id: i64,
    pub expected_state_hash: String,
    pub reason: String,
    pub confirm: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Mapping {
    version: u32,
    receipt_id: i64,
    ledger_uuid: String,
    operation: String,
    key_hash: String,
    payload_hash: String,
    source_kind: String,
    source_subject: String,
    outcome_hash: String,
    target_user_id: String,
    actor: String,
    at: String,
    reason: String,
    archive_sha256: String,
    restore_boundary_id: i64,
}
#[derive(Clone)]
struct Boundary {
    id: i64,
    high: i64,
    archive: String,
}
struct Source {
    id: i64,
    ledger: String,
    operation: String,
    key: String,
    payload: String,
    kind: String,
    subject: String,
    created: String,
    expires: String,
    result: Option<Value>,
    outcome_hash: Option<String>,
    boundary: Boundary,
}
fn refused() -> Error {
    Error::new(
        "RESTORED_RECEIPT_REFUSED",
        "Imported receipt proof, review or retained outcome unavailable; reconcile source evidence manually",
    )
}
fn sql(_: impl std::fmt::Display) -> Error {
    refused()
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn hex(s: &str, n: usize) -> bool {
    s.len() == n
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn text(s: &str, min: usize, max: usize) -> bool {
    s.len() >= min && s.len() <= max && s.trim() == s && !s.chars().any(char::is_control)
}
fn boundaries(db: &Connection) -> Result<Vec<Boundary>, Error> {
    let mut q=db.prepare("SELECT id,entity_type,entity_id,message,actor FROM audit_log WHERE event_type=?1 ORDER BY id").map_err(sql)?;
    let values = q
        .query_map([BOUNDARY], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
            ))
        })
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    let max: i64 = db
        .query_row(
            "SELECT COALESCE(MAX(id),0) FROM mutation_idempotency_tombstones",
            [],
            |r| r.get(0),
        )
        .map_err(sql)?;
    let mut out = Vec::new();
    let mut prior = 0;
    for (id, entity, high, message, actor) in values {
        let n = high.parse::<i64>().map_err(sql)?;
        let archive = message
            .strip_prefix("Imported receipt high-water mark; archive ")
            .and_then(|s| {
                s.strip_suffix(
                    "; original keys require manual reconciliation under fresh identities",
                )
            })
            .ok_or_else(refused)?;
        if id <= 0
            || entity != "mutation_idempotency_tombstones"
            || n < prior
            || n > max
            || high != n.to_string()
            || actor != "system:portable-restore"
            || !hex(archive, 64)
        {
            return Err(refused());
        }
        out.push(Boundary {
            id,
            high: n,
            archive: archive.into(),
        });
        prior = n;
    }
    Ok(out)
}
fn source(db: &Connection, id: i64, bs: &[Boundary]) -> Result<Source, Error> {
    let boundary = bs
        .iter()
        .rev()
        .find(|b| id > 0 && id <= b.high)
        .cloned()
        .ok_or_else(refused)?;
    let mut q=db.prepare("SELECT t.ledger_uuid,t.operation,t.client_key_hash,t.payload_hash,t.principal_kind,t.principal_subject_id,t.created_at,t.expires_at,o.outcome_json,o.created_at FROM mutation_idempotency_tombstones t LEFT JOIN mutation_idempotency_outcomes o ON o.tombstone_id=t.id WHERE t.id=?1").map_err(sql)?;
    type ReceiptColumns = (
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        Option<String>,
        Option<String>,
    );
    let (ledger, operation, key, payload, kind, subject, created, expires, outcome, outcome_at): ReceiptColumns = q
        .query_row([id], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
                r.get(8)?,
                r.get(9)?,
            ))
        })
        .map_err(sql)?;
    let actual: String = db
        .query_row(
            "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
            [],
            |r| r.get(0),
        )
        .map_err(sql)?;
    if ledger != actual
        || !hex(&ledger, 32)
        || !hex(&key, 64)
        || !hex(&payload, 64)
        || !matches!(
            operation.as_str(),
            "journal_post" | "journal_reverse" | "opening_balance_post"
        )
        || !matches!(kind.as_str(), "user" | "service-account")
        || !text(&subject, 1, 256)
    {
        return Err(refused());
    }
    let created_ms = crate::cookie_session::timestamp(&created)?;
    let expires_ms = crate::cookie_session::timestamp(&expires)?;
    if expires_ms.checked_sub(created_ms) != Some(30 * 86_400_000) {
        return Err(refused());
    }
    let (result, outcome_hash) = if let Some(raw) = outcome {
        if raw.len() > 65_536 || outcome_at.as_deref() != Some(created.as_str()) {
            return Err(refused());
        }
        let value: Value = serde_json::from_str(&raw).map_err(sql)?;
        if crate::idempotency::outcome_json(&value)? != raw {
            return Err(refused());
        }
        verify_result(db, &operation, &value)?;
        (Some(value), Some(hash(raw.as_bytes())))
    } else {
        if outcome_at.is_some() {
            return Err(refused());
        }
        (None, None)
    };
    Ok(Source {
        id,
        ledger,
        operation,
        key,
        payload,
        kind,
        subject,
        created,
        expires,
        result,
        outcome_hash,
        boundary,
    })
}
pub(crate) fn verify_result(db: &Connection, operation: &str, result: &Value) -> Result<(), Error> {
    if operation == "opening_balance_post" {
        let opening = crate::opening_balance::validate_history(db)
            .map_err(|_| refused())?
            .ok_or_else(refused)?;
        // Reconciliation adopts the exact historical result, never a new opening
        // or a projection rebuilt from current editable company settings.
        if *result != json!({"ok":true,"openingBalance":opening.record(),"errors":[]}) {
            return Err(refused());
        }
        return Ok(());
    }
    if result["ok"] != true {
        return Err(refused());
    }
    let id = result["entryId"]
        .as_i64()
        .filter(|id| *id > 0)
        .ok_or_else(refused)?;
    let (number, entry_hash, reversed, previous, status): (String, String, Option<i64>, String, String) = db
        .query_row(
            "SELECT entry_no,entry_hash,reversal_of_entry_id,previous_hash,status FROM journal_entries WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .map_err(sql)?;
    if status
        != if operation == "journal_reverse" {
            "reversed"
        } else {
            "posted"
        }
        || crate::journal_hash::stored_hash(db, id, &previous)? != entry_hash
        || result["entryNo"] != number
        || result["entryHash"] != entry_hash
        || !hex(&entry_hash, 64)
        || (operation == "journal_post" && reversed.is_some())
        || (operation == "journal_reverse"
            && (reversed.is_none() || result["originalEntryId"].as_i64() != reversed))
    {
        return Err(refused());
    }
    if let Some(original) = reversed
        && (original <= 0
            || original == id
            || !db
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM journal_entries WHERE id=?1)",
                    [original],
                    |r| r.get::<_, bool>(0),
                )
                .map_err(sql)?)
    {
        return Err(refused());
    }
    Ok(())
}
fn mapping_matches(m: &Mapping, s: &Source) -> bool {
    m.version == 1
        && m.receipt_id == s.id
        && m.ledger_uuid == s.ledger
        && m.operation == s.operation
        && m.key_hash == s.key
        && m.payload_hash == s.payload
        && m.source_kind == s.kind
        && m.source_subject == s.subject
        && s.outcome_hash
            .as_ref()
            .map_or_else(|| hex(&m.outcome_hash, 64), |h| h == &m.outcome_hash)
}
fn mappings(db: &Connection, s: &Source, bs: &[Boundary]) -> Result<Option<Mapping>, Error> {
    let mut q=db.prepare("SELECT entity_type,entity_id,message,actor,id FROM audit_log WHERE event_type=?1 AND entity_id=?2 ORDER BY id").map_err(sql)?;
    let mut rows = q.query(params![EVENT, s.id.to_string()]).map_err(sql)?;
    let mut current = None;
    let mut seen = std::collections::BTreeSet::new();
    while let Some(r) = rows.next().map_err(sql)? {
        let entity: String = r.get(0).map_err(sql)?;
        let id: String = r.get(1).map_err(sql)?;
        let raw: String = r.get(2).map_err(sql)?;
        let actor: String = r.get(3).map_err(sql)?;
        let audit_id: i64 = r.get(4).map_err(sql)?;
        if raw.len() > 16 * 1024 {
            return Err(refused());
        }
        let m: Mapping = serde_json::from_str(&raw).map_err(sql)?;
        let boundary = bs
            .iter()
            .find(|b| {
                b.id == m.restore_boundary_id && s.id <= b.high && b.archive == m.archive_sha256
            })
            .ok_or_else(refused)?;
        if bs.iter().rev().find(|b| b.id < audit_id).map(|b| b.id) != Some(boundary.id)
            || entity != "idempotency_receipt"
            || id != s.id.to_string()
            || !mapping_matches(&m, s)
            || m.actor != format!("user:{}", m.target_user_id)
            || actor != m.actor
            || !text(&m.target_user_id, 1, 256)
            || !text(&m.reason, 10, 2000)
            || crate::cookie_session::timestamp(&m.at).is_err()
            || !seen.insert(boundary.id)
        {
            return Err(refused());
        }
        if boundary.id == s.boundary.id {
            current = Some(m)
        }
    }
    Ok(current)
}
fn projection(s: &Source, m: Option<&Mapping>) -> Result<Value, Error> {
    let mut value = json!({"receiptId":s.id,"operation":s.operation,"keyHash":s.key,"payloadHash":s.payload,"ledgerUuid":s.ledger,"sourcePrincipal":{"kind":s.kind,"subject":s.subject},"createdAt":s.created,"expiresAt":s.expires,"archiveSha256":s.boundary.archive,"restoreBoundaryId":s.boundary.id,"resultAvailable":s.result.is_some(),"result":s.result,"mapping":m.map(|m|json!({"targetUserId":m.target_user_id,"actor":m.actor,"at":m.at,"reason":m.reason,"archiveSha256":m.archive_sha256,"restoreBoundaryId":m.restore_boundary_id}))});
    let state = hash(&serde_json::to_vec(&value).map_err(sql)?);
    value["stateHash"] = json!(state);
    Ok(value)
}
pub fn inventory(db: &Connection) -> Result<Value, Error> {
    let bs = boundaries(db)?;
    let high = bs.last().map_or(0, |b| b.high);
    let mut q=db.prepare("SELECT id FROM mutation_idempotency_tombstones WHERE id<=?1 AND operation IN ('journal_post','journal_reverse','opening_balance_post') ORDER BY id").map_err(sql)?;
    let ids = q
        .query_map([high], |r| r.get::<_, i64>(0))
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    let mut rows = Vec::new();
    for id in ids {
        let s = source(db, id, &bs)?;
        let m = mappings(db, &s, &bs)?;
        rows.push(projection(&s, m.as_ref())?)
    }
    Ok(json!({"rows":rows}))
}
/// Caller has resolved current workspace management, company administration and fresh MFA.
pub fn reconcile(
    tx: &Transaction<'_>,
    input: &Reconcile,
    subject: &str,
    now: &str,
) -> Result<Value, Error> {
    crate::idempotency::writer(tx)?;
    if !input.confirm
        || input.receipt_id <= 0
        || !hex(&input.expected_state_hash, 64)
        || !text(input.reason.trim(), 10, 2000)
        || !text(subject, 1, 256)
        || crate::cookie_session::timestamp(now).is_err()
    {
        return Err(refused());
    }
    let bs = boundaries(tx)?;
    let s = source(tx, input.receipt_id, &bs)?;
    let m = mappings(tx, &s, &bs)?;
    if projection(&s, m.as_ref())?["stateHash"] != input.expected_state_hash || s.result.is_none() {
        return Err(refused());
    }
    if let Some(existing) = m {
        if existing.target_user_id == subject {
            return Ok(json!({"rows":[projection(&s,Some(&existing))?]}));
        }
        return Err(refused());
    }
    if imported_ids(tx, &s.key, &s.ledger, &s.operation)?.len() != 1 {
        return Err(refused());
    }
    let m = Mapping {
        version: 1,
        receipt_id: s.id,
        ledger_uuid: s.ledger.clone(),
        operation: s.operation.clone(),
        key_hash: s.key.clone(),
        payload_hash: s.payload.clone(),
        source_kind: s.kind.clone(),
        source_subject: s.subject.clone(),
        outcome_hash: s.outcome_hash.clone().ok_or_else(refused)?,
        target_user_id: subject.into(),
        actor: format!("user:{subject}"),
        at: now.into(),
        reason: input.reason.trim().into(),
        archive_sha256: s.boundary.archive.clone(),
        restore_boundary_id: s.boundary.id,
    };
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES(?1,'idempotency_receipt',?2,?3,?4)",params![EVENT,s.id.to_string(),serde_json::to_string(&m).map_err(sql)?,m.actor]).map_err(sql)?;
    Ok(json!({"rows":[projection(&s,Some(&m))?]}))
}
/// No callback: an authorized exact imported replay can only return the retained source outcome.
pub(crate) fn mapped_replay(
    tx: &Transaction<'_>,
    id: i64,
    subject: &str,
    payload: &str,
) -> Result<Option<(Value, Value)>, Error> {
    if !tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM audit_log WHERE event_type=?1 AND entity_id=?2)",
            params![EVENT, id.to_string()],
            |r| r.get::<_, bool>(0),
        )
        .map_err(sql)?
    {
        return Ok(None);
    }
    let bs = boundaries(tx)?;
    let s = source(tx, id, &bs)?;
    let m = mappings(tx, &s, &bs)?;
    if s.payload != payload {
        return Err(Error::new(
            "IDEMPOTENCY_CONFLICT",
            "Imported key payload differs from the original accepted request",
        ));
    }
    match m {
        Some(m) if m.target_user_id == subject => Ok(Some((
            s.result.ok_or_else(refused)?,
            json!({"archiveSha256":m.archive_sha256,"restoreBoundaryId":m.restore_boundary_id,"sourcePrincipal":{"kind":m.source_kind,"subject":m.source_subject},"mappedUserId":m.target_user_id}),
        ))),
        _ => Ok(None),
    }
}
/// Exact imported match count is checked before disclosure, regardless of the live principal.
pub(crate) fn imported_ids(
    db: &Connection,
    key: &str,
    ledger: &str,
    operation: &str,
) -> Result<Vec<i64>, Error> {
    let bs = boundaries(db)?;
    let high = bs.last().map_or(0, |b| b.high);
    let mut q=db.prepare("SELECT id FROM mutation_idempotency_tombstones WHERE id<=?1 AND client_key_hash=?2 AND ledger_uuid=?3 AND operation=?4 ORDER BY id").map_err(sql)?;
    q.query_map(params![high, key, ledger, operation], |r| r.get(0))
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)
}
#[cfg(test)]
mod tests;
