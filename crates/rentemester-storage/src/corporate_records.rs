//! Shared immutable governance evidence in canonical control tables16/20/23. No ledger writes.
use rentemester_engine::{
    Error,
    company_knowledge::{iso, text},
};
use rusqlite::{OptionalExtension, Transaction, params};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
pub const MAX_BYTES: usize = 16 * 1024 * 1024;
const TYPES: &[&str] = &[
    "incorporation",
    "articles",
    "registration",
    "shareholder_register",
    "shareholder_agreement",
    "board_resolution",
    "general_meeting_minutes",
    "capital_or_dividend_resolution",
    "annual_report",
    "auditor_deliverable",
    "intercompany_agreement",
    "ownership_register",
    "share_transfer",
    "authority_letter",
    "bank_mandate",
    "tax_registration",
    "other",
];
fn fail(_: impl std::fmt::Display) -> Error {
    Error::new(
        "corporate_record_refused",
        "corporate record operation refused",
    )
}
fn missing() -> Error {
    Error::new("corporate_record_not_found", "corporate record not found")
}
pub fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
#[derive(Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct Link {
    pub r#type: String,
    pub id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Ingest {
    pub record_id: Option<String>,
    pub r#type: String,
    pub bytes_base64: String,
    pub filename: String,
    pub source: String,
    pub received_at: String,
    pub record_date: Option<String>,
    pub retention_until: Option<String>,
    pub uploader: String,
    pub sensitivity: Option<String>,
    pub links: Option<Vec<Link>>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Enrich {
    pub record_id: Option<String>,
    pub assertion: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Supersede {
    pub record_id: Option<String>,
    pub replacement_record_id: String,
    pub reason: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LinkInput {
    pub record_id: Option<String>,
    pub r#type: String,
    pub id: String,
}
pub fn scopes(tx: &Transaction<'_>, id: &str) -> Result<BTreeSet<String>, Error> {
    let mut q=tx.prepare("SELECT scope_id FROM main.rm_corporate_record_scope_assertions WHERE record_id=? AND scope_kind='company' ORDER BY scope_id").map_err(fail)?;
    q.query_map([id], |r| r.get(0))
        .map_err(fail)?
        .collect::<Result<_, _>>()
        .map_err(fail)
}
pub fn exists(tx: &Transaction<'_>, id: &str) -> Result<bool, Error> {
    tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM main.rm_corporate_record_bytes WHERE record_id=?)",
        [id],
        |r| r.get(0),
    )
    .map_err(fail)
}
pub fn visible(tx: &Transaction<'_>, id: &str, read: &BTreeSet<String>) -> Result<bool, Error> {
    let scopes = scopes(tx, id)?;
    Ok(exists(tx, id)? && !scopes.is_empty() && scopes.iter().all(|s| read.contains(s)))
}
pub fn inspect(tx: &Transaction<'_>, id: &str) -> Result<Value, Error> {
    let mut record=tx.query_row("SELECT sha256,filename,source,received_at,record_date,retention_until,uploader,sensitivity FROM main.rm_corporate_record_bytes WHERE record_id=?",[id],|r|Ok(json!({"recordId":id,"sha256":r.get::<_,String>(0)?,"filename":r.get::<_,String>(1)?,"source":r.get::<_,String>(2)?,"receivedAt":r.get::<_,String>(3)?,"recordDate":r.get::<_,Option<String>>(4)?,"retentionUntil":r.get::<_,Option<String>>(5)?,"uploader":r.get::<_,String>(6)?,"sensitivity":r.get::<_,String>(7)?}))).optional().map_err(fail)?.ok_or_else(missing)?;
    let mut q=tx.prepare("SELECT event_type,payload_hash,actor,created_at,canonical_payload FROM main.rm_corporate_record_events WHERE record_id=? ORDER BY id").map_err(fail)?;
    let rows=q.query_map([id],|r|Ok((json!({"event_type":r.get::<_,String>(0)?,"payload_hash":r.get::<_,String>(1)?,"actor":r.get::<_,String>(2)?,"created_at":r.get::<_,String>(3)?}),r.get::<_,String>(4)?))).map_err(fail)?.collect::<Result<Vec<_>,_>>().map_err(fail)?;
    record["type"] = match rows.first() {
        Some((_, payload)) => serde_json::from_str::<Value>(payload).map_err(fail)?["type"].clone(),
        None => json!("other"),
    };
    record["history"] = json!(rows.into_iter().map(|(r, _)| r).collect::<Vec<_>>());
    let mut q=tx.prepare("SELECT scope_kind,scope_id FROM main.rm_corporate_record_scope_assertions WHERE record_id=? ORDER BY scope_kind,scope_id").map_err(fail)?;
    record["links"] = json!(
        q.query_map([id], |r| Ok(
            json!({"type":r.get::<_,String>(0)?,"id":r.get::<_,String>(1)?})
        ))
        .map_err(fail)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(fail)?
    );
    Ok(record)
}
pub fn detail_visible(
    tx: &Transaction<'_>,
    id: &str,
    read: &BTreeSet<String>,
    write: &BTreeSet<String>,
) -> Result<bool, Error> {
    if !visible(tx, id, read)? {
        return Ok(false);
    }
    Ok(inspect(tx, id)?["sensitivity"] == "normal"
        || scopes(tx, id)?.iter().all(|s| write.contains(s)))
}
pub fn list(
    tx: &Transaction<'_>,
    read: &BTreeSet<String>,
    cursor: usize,
    limit: usize,
) -> Result<Value, Error> {
    if !(1..=100).contains(&limit) || cursor > 9_007_199_254_740_991 {
        return Err(fail("pagination"));
    }
    let mut q = tx
        .prepare("SELECT record_id FROM main.rm_corporate_record_bytes ORDER BY record_id")
        .map_err(fail)?;
    let ids = q
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(fail)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(fail)?;
    let mut records = vec![];
    for id in ids {
        if visible(tx, &id, read)? {
            records.push(inspect(tx, &id)?);
        }
    }
    let count = records.len();
    Ok(
        json!({"rows":records.into_iter().skip(cursor).take(limit).collect::<Vec<_>>(),"count":count,"nextCursor":cursor.checked_add(limit).filter(|n|*n<count)}),
    )
}
fn append(
    tx: &Transaction<'_>,
    id: &str,
    kind: &str,
    event: &str,
    payload: Value,
    actor: &str,
    stamp: &str,
) -> Result<String, Error> {
    let payload = serde_json::to_string(&payload).map_err(fail)?;
    let digest = hash(payload.as_bytes());
    let stored = if [
        "articles",
        "registration",
        "board_resolution",
        "ownership_register",
        "share_transfer",
        "authority_letter",
        "bank_mandate",
        "tax_registration",
        "other",
    ]
    .contains(&kind)
    {
        kind
    } else {
        "other"
    };
    tx.execute("INSERT OR IGNORE INTO main.rm_corporate_record_events(record_id,event_type,record_type,payload_hash,canonical_payload,actor,created_at) VALUES(?,?,?,?,?,?,?)",params![id,event,stored,digest,payload,text(actor,160)?,stamp]).map_err(fail)?;
    Ok(digest)
}
pub fn validate_link(tx: &Transaction<'_>, link: &Link) -> Result<(), Error> {
    if ![
        "company",
        "party",
        "ownership",
        "disposition",
        "legacy_evidence_ref",
    ]
    .contains(&link.r#type.as_str())
    {
        return Err(fail("link type"));
    }
    let id = text(&link.id, 256)?;
    if link.r#type == "party" && !crate::parties::exists(tx, &id)? {
        return Err(fail("party target"));
    }
    Ok(())
}
pub fn link(
    tx: &Transaction<'_>,
    record: &str,
    link: &Link,
    actor: &str,
    stamp: &str,
) -> Result<Value, Error> {
    let old = inspect(tx, record)?;
    validate_link(tx, link)?;
    let id = text(&link.id, 256)?;
    tx.execute("INSERT OR IGNORE INTO main.rm_corporate_record_links(record_id,link_type,link_id) VALUES(?,?,?)",params![record,link.r#type,id]).map_err(fail)?;
    tx.execute("INSERT OR IGNORE INTO main.rm_corporate_record_scope_assertions(record_id,scope_kind,scope_id,actor,created_at) VALUES(?,?,?,?,?)",params![record,link.r#type,id,text(actor,160)?,stamp]).map_err(fail)?;
    append(
        tx,
        record,
        old["type"].as_str().ok_or_else(|| fail("type"))?,
        "linked",
        json!({"type":link.r#type,"id":id}),
        actor,
        stamp,
    )?;
    inspect(tx, record)
}
pub fn ingest(
    tx: &Transaction<'_>,
    input: &Ingest,
    bytes: &[u8],
    generated: &str,
    links: &[Link],
    actor: &str,
) -> Result<Value, Error> {
    if !TYPES.contains(&input.r#type.as_str()) || bytes.is_empty() || bytes.len() > MAX_BYTES {
        return Err(fail("type/size"));
    }
    let id = text(input.record_id.as_deref().unwrap_or(generated), 64)?;
    let digest = hash(bytes);
    let received = iso(&text(&input.received_at, 40)?)?;
    let sensitivity = input.sensitivity.as_deref().unwrap_or("normal");
    if !["normal", "confidential", "restricted"].contains(&sensitivity) {
        return Err(fail("sensitivity"));
    }
    let optional_date =
        |v: &Option<String>| v.as_deref().filter(|s| !s.is_empty()).map(iso).transpose();
    let payload = json!({"type":input.r#type,"sha256":digest,"filename":text(&input.filename,320)?,"source":text(&input.source,320)?,"receivedAt":received,"recordDate":optional_date(&input.record_date)?,"retentionUntil":optional_date(&input.retention_until)?,"uploader":text(&input.uploader,160)?,"sensitivity":sensitivity});
    if exists(tx, &id)? {
        if inspect(tx, &id)?["sha256"] != digest {
            return Err(fail("immutable recordId"));
        }
        return inspect(tx, &id);
    }
    let duplicate: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM main.rm_corporate_record_bytes WHERE sha256=?)",
            [&digest],
            |r| r.get(0),
        )
        .map_err(fail)?;
    if duplicate {
        return Err(fail("duplicate bytes require explicit linking"));
    }
    for l in links {
        validate_link(tx, l)?;
    }
    tx.execute("INSERT INTO main.rm_corporate_record_bytes(record_id,original_bytes,sha256,filename,source,received_at,record_date,retention_until,uploader,sensitivity) VALUES(?,?,?,?,?,?,?,?,?,?)",params![id,bytes,digest,payload["filename"].as_str(),payload["source"].as_str(),received,payload["recordDate"].as_str(),payload["retentionUntil"].as_str(),payload["uploader"].as_str(),sensitivity]).map_err(fail)?;
    append(
        tx,
        &id,
        &input.r#type,
        "ingested",
        payload,
        actor,
        &received,
    )?;
    for l in links {
        link(tx, &id, l, actor, &received)?;
    }
    inspect(tx, &id)
}
pub fn enrich(
    tx: &Transaction<'_>,
    id: &str,
    input: &Enrich,
    actor: &str,
    stamp: &str,
) -> Result<Value, Error> {
    let old = inspect(tx, id)?;
    let digest = append(
        tx,
        id,
        old["type"].as_str().ok_or_else(|| fail("type"))?,
        "enriched",
        json!({"assertion":text(&input.assertion,2000)?}),
        actor,
        stamp,
    )?;
    Ok(json!({"payloadHash":digest}))
}
pub fn supersede(
    tx: &Transaction<'_>,
    id: &str,
    input: &Supersede,
    actor: &str,
    stamp: &str,
) -> Result<Value, Error> {
    let old = inspect(tx, id)?;
    inspect(tx, &input.replacement_record_id)?;
    let digest = append(
        tx,
        id,
        old["type"].as_str().ok_or_else(|| fail("type"))?,
        "superseded",
        json!({"replacementRecordId":input.replacement_record_id,"reason":text(&input.reason,1000)?}),
        actor,
        stamp,
    )?;
    Ok(json!({"payloadHash":digest}))
}
pub fn download(tx: &Transaction<'_>, id: &str) -> Result<Vec<u8>, Error> {
    let (bytes, digest): (Vec<u8>, String) = tx
        .query_row(
            "SELECT original_bytes,sha256 FROM main.rm_corporate_record_bytes WHERE record_id=?",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(fail)?
        .ok_or_else(missing)?;
    if bytes.len() > MAX_BYTES || bytes.is_empty() || hash(&bytes) != digest {
        return Err(fail("original bytes corrupt"));
    }
    Ok(bytes)
}
#[cfg(test)]
mod tests;
