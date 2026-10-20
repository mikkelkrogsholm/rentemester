//! Trusted mail observations and reviewed handoff. Caller owns admission and transactions.
pub use crate::workspace_inbox::Provenance;
use crate::workspace_inbox::{canonical, event, refused};
use rentemester_engine::{Error, mail_intake::ParsedMail};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
mod ingress;
mod receipt;
mod review;
pub use ingress::{cursor, retain, retain_invalid, retain_refusal};
pub(crate) use receipt::cross_validate;
pub use receipt::{MailReceipt, record_document, validate_mail_intake};
pub(crate) use review::validate_control;
pub use review::{ApprovedAttachment, approved_attachment, review};
const VERSION: &str = "rentemester-native-mail-source/1";
fn sql(_: rusqlite::Error) -> Error {
    refused("mail evidence storage unavailable; abort transaction")
}
fn hash(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}
fn ensure(ok: bool, why: &str) -> Result<(), Error> {
    if ok { Ok(()) } else { Err(refused(why)) }
}
fn required<'a>(v: &'a Value, k: &str) -> Result<&'a str, Error> {
    v[k].as_str()
        .ok_or_else(|| refused("mail evidence field missing"))
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MailObservation {
    pub company_slug: String,
    pub config_epoch: String,
    pub mailbox: String,
    pub uid_validity: u32,
    pub uid: u32,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RetainedMail {
    pub source_ids: Vec<String>,
    pub duplicate: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewInput {
    pub company_slug: String,
    pub source_hash: String,
    pub metadata: Value,
}
fn source(db: &Connection, id: &str) -> Result<(Value, Vec<u8>, String, String), Error> {
    let (transport,raw,bytes,sha,anchor):(String,String,Vec<u8>,String,String)=db.query_row("SELECT transport,metadata_json,original_bytes,sha256,visibility_anchor_slug FROM rm_workspace_inbox_sources WHERE source_id=?",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).map_err(sql)?;
    let value: Value =
        serde_json::from_str(&raw).map_err(|_| refused("mail source JSON differs"))?;
    ensure(
        bytes.len() > 0
            && bytes.len() <= 25 * 1024 * 1024
            && raw.len() <= 64 * 1024
            && canonical(&value)? == raw
            && hash(&bytes) == sha,
        "mail source canonical bytes/hash differ",
    )?;
    if transport != "mail" {
        return Ok((Value::Null, bytes, sha, anchor));
    }
    if value.get("nativeMail").is_none() {
        return Ok((Value::Null, bytes, sha, anchor));
    }
    ensure(
        value["nativeMail"]["version"] == VERSION,
        "unsupported mail source provenance",
    )?;
    Ok((value["nativeMail"].clone(), bytes, sha, anchor))
}
fn events(db: &Connection, id: &str) -> Result<Vec<Value>, Error> {
    let mut q=db.prepare("SELECT id,event_type,canonical_payload,payload_hash,actor,created_at FROM rm_workspace_inbox_events WHERE source_id=? ORDER BY id").map_err(sql)?;
    q.query_map([id], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, String>(5)?,
        ))
    })
    .map_err(sql)?
    .map(|r| {
        let (id, kind, body, h, actor, at) = r.map_err(sql)?;
        let p: Value =
            serde_json::from_str(&body).map_err(|_| refused("mail event JSON differs"))?;
        ensure(
            canonical(&p)? == body && hash(body.as_bytes()) == h,
            "mail event hash differs",
        )?;
        Ok(json!({"id":id,"kind":kind,"hash":h,"payload":p,"actor":actor,"at":at}))
    })
    .collect()
}
/// Projection never grants authority. Legacy uploads have no native mail fields.
pub fn projection(db: &Connection, id: &str) -> Result<(Value, Value), Error> {
    let (mail, _, _, _) = source(db, id)?;
    if mail.is_null() {
        return Ok((Value::Null, Value::Null));
    }
    let h = events(db, id)?;
    let review = h
        .iter()
        .rev()
        .find(|e| e["kind"] == "metadata_reviewed")
        .map(|e| {
            let mut p = e["payload"].clone();
            p["reviewedBy"] = e["actor"].clone();
            p["reviewedAt"] = e["at"].clone();
            p
        })
        .unwrap_or(Value::Null);
    Ok((mail, review))
}
#[cfg(test)]
mod tests;
