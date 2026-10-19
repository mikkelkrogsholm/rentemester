//! Append-only invoice email reservations and evidence. No credentials, MIME bytes or network I/O.
use crate::invalid;
use rentemester_engine::Error;
use rusqlite::{Connection, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
pub fn refused() -> Error {
    Error::new(
        "invoice_email_refused",
        "invoice email intent, immutable source or current evidence refused",
    )
}
pub fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn hash(v: &str) -> bool {
    v.len() == 64
        && v.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub fn message_id(v: &str) -> bool {
    v.strip_prefix('<')
        .and_then(|s| s.strip_suffix("@rentemester.local>"))
        .is_some_and(|s| {
            s.len() == 32
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
}
pub fn recipient(v: &str) -> Result<String, Error> {
    let v = v.trim();
    let parts = v.split('@').collect::<Vec<_>>();
    if v.len() > 254
        || !v.is_ascii()
        || parts.len() != 2
        || parts.iter().any(|s| s.is_empty())
        || !parts[1].contains('.')
        || v.chars()
            .any(|c| c.is_whitespace() || c.is_control() || "<>,;\\\"".contains(c))
    {
        return Err(refused());
    }
    Ok(v.into())
}
fn text(v: &str, max: usize) -> bool {
    !v.is_empty() && v.len() <= max && !v.chars().any(char::is_control)
}
fn review(v: &str, max: usize) -> Result<String, Error> {
    let v = rentemester_engine::company_knowledge::trim(v);
    if v.len() < 10 || !text(v, max) {
        return Err(refused());
    }
    Ok(v.into())
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Send {
    pub invoice_document_id: i64,
    pub to: String,
    pub kind: Option<String>,
    pub confirm: bool,
}
impl Send {
    pub fn validate(&self) -> Result<(String, String), Error> {
        if !self.confirm || !(1..=9_007_199_254_740_991).contains(&self.invoice_document_id) {
            return Err(refused());
        }
        let kind = self.kind.as_deref().unwrap_or("invoice");
        if !matches!(kind, "invoice" | "reminder") {
            return Err(refused());
        }
        Ok((recipient(&self.to)?, kind.into()))
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Retry {
    pub message_id: String,
    pub expected_state_hash: String,
    pub reason: String,
    pub confirm: bool,
}
impl Retry {
    pub fn validate(&self) -> Result<String, Error> {
        if !self.confirm || !message_id(&self.message_id) || !hash(&self.expected_state_hash) {
            return Err(refused());
        }
        review(&self.reason, 2000)
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Reconcile {
    pub message_id: String,
    pub expected_state_hash: String,
    pub resolution: String,
    pub reason: String,
    pub evidence: String,
    pub confirm: bool,
}
impl Reconcile {
    pub fn validate(&self) -> Result<(String, String), Error> {
        if !self.confirm
            || !message_id(&self.message_id)
            || !hash(&self.expected_state_hash)
            || !matches!(self.resolution.as_str(), "accepted" | "not_accepted")
        {
            return Err(refused());
        }
        Ok((review(&self.reason, 2000)?, review(&self.evidence, 4000)?))
    }
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Sender {
    pub host: String,
    pub port: u16,
    pub from_address: String,
    pub from_name: Option<String>,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Source {
    pub ledger_uuid: String,
    pub invoice_document_id: i64,
    pub invoice_number: String,
    pub kind: String,
    pub recipient: String,
    pub subject: String,
    pub message_id: String,
    pub body_sha256: String,
    pub json_sha256: String,
    pub pdf_sha256: String,
    pub json_path: String,
    pub pdf_path: String,
    pub pdf_filename: String,
    pub sender: Sender,
}
/// Metadata admission is shared with signed snapshots. Physical bytes/MIME are verified by the held namespace adapter.
pub fn validate_source(db: &Connection, s: &Source) -> Result<(), Error> {
    let uuid: String = db
        .query_row(
            "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
            [],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    let pairs:i64=db.query_row("SELECT count(*) FROM documents d JOIN documents p ON p.document_type='issued_invoice_pdf' AND p.invoice_no=d.invoice_no AND p.payload_json=d.payload_json WHERE d.id=?1 AND d.document_type='issued_invoice'",[s.invoice_document_id],|r|r.get(0)).map_err(invalid)?;
    if pairs != 1 {
        return Err(refused());
    }
    let row:(String,String,String,String,String,String,String)=db.query_row("SELECT d.invoice_no,d.sha256_hash,d.stored_path,p.sha256_hash,p.stored_path,p.original_filename,d.payload_json FROM documents d JOIN documents p ON p.document_type='issued_invoice_pdf' AND p.invoice_no=d.invoice_no AND p.payload_json=d.payload_json WHERE d.id=?1 AND d.document_type='issued_invoice'",[s.invoice_document_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?))).map_err(invalid)?;
    let payload: Value = serde_json::from_str(&row.6).map_err(invalid)?;
    if s.ledger_uuid != uuid
        || s.invoice_number != row.0
        || payload["invoiceNumber"] != s.invoice_number
        || s.json_sha256 != row.1
        || s.json_path != row.2
        || s.pdf_sha256 != row.3
        || s.pdf_path != row.4
        || s.pdf_filename != row.5
        || !matches!(s.kind.as_str(), "invoice" | "reminder")
        || recipient(&s.recipient)? != s.recipient
        || !message_id(&s.message_id)
        || ![&s.body_sha256, &s.pdf_sha256, &s.json_sha256]
            .iter()
            .all(|h| hash(h))
        || !text(&s.subject, 1000)
        || !text(&s.sender.host, 253)
        || s.sender.port == 0
        || recipient(&s.sender.from_address)? != s.sender.from_address
        || s.sender.from_name.as_ref().is_some_and(|n| !text(n, 200))
    {
        return Err(refused());
    }
    Ok(())
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Event {
    version: u8,
    source: Source,
    event_type: String,
    attempt_id: String,
    attempt_index: u32,
    previous_state_hash: String,
    actor: String,
    at: String,
    reason: Option<String>,
    evidence: Option<String>,
    resolution: Option<String>,
}
#[derive(Clone)]
pub struct Delivery {
    pub source: Source,
    pub state: String,
    pub state_hash: String,
    pub attempt_id: String,
    pub attempt_index: u32,
    pending: bool,
    history: Vec<Value>,
}
impl Delivery {
    pub fn row(&self, duplicate: bool) -> Value {
        json!({"invoiceDocumentId":self.source.invoice_document_id,"invoiceNumber":self.source.invoice_number,"kind":self.source.kind,"recipient":self.source.recipient,"subject":self.source.subject,"messageId":self.source.message_id,"bodySha256":self.source.body_sha256,"pdfSha256":self.source.pdf_sha256,"state":self.state,"stateHash":self.state_hash,"attemptId":self.attempt_id,"humanResolution":self.history.last().is_some_and(|e|e["eventType"]=="reconciled"),"history":self.history,"duplicate":duplicate})
    }
}
#[derive(Clone)]
struct Success {
    id: i64,
    doc: i64,
    no: String,
    kind: String,
    to: String,
    from: String,
    subject: String,
    message: String,
    body: String,
    host: String,
    at: String,
}
fn successes(db: &Connection) -> Result<BTreeMap<String, Success>, Error> {
    let mut q=db.prepare("SELECT id,invoice_document_id,invoice_no,kind,recipient,sender,subject,message_id,body_sha256,smtp_host,created_at FROM email_send_log ORDER BY id").map_err(invalid)?;
    let rows = q
        .query_map([], |r| {
            Ok(Success {
                id: r.get(0)?,
                doc: r.get(1)?,
                no: r.get(2)?,
                kind: r.get(3)?,
                to: r.get(4)?,
                from: r.get(5)?,
                subject: r.get(6)?,
                message: r.get(7)?,
                body: r.get(8)?,
                host: r.get(9)?,
                at: r.get(10)?,
            })
        })
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    let mut out = BTreeMap::new();
    for r in rows {
        if r.id <= 0
            || !message_id(&r.message)
            || !hash(&r.body)
            || recipient(&r.to)? != r.to
            || recipient(&r.from)? != r.from
            || !matches!(r.kind.as_str(), "invoice" | "reminder")
            || !text(&r.subject, 1000)
            || !text(&r.host, 253)
        {
            return Err(refused());
        }
        let no:String=db.query_row("SELECT d.invoice_no FROM documents d JOIN documents p ON p.document_type='issued_invoice_pdf' AND p.invoice_no=d.invoice_no AND p.payload_json=d.payload_json WHERE d.id=?1 AND d.document_type='issued_invoice'",[r.doc],|x|x.get(0)).map_err(invalid)?;
        if no != r.no || out.insert(r.message.clone(), r).is_some() {
            return Err(refused());
        }
    }
    Ok(out)
}
fn success_matches(r: &Success, s: &Source) -> bool {
    r.doc == s.invoice_document_id
        && r.no == s.invoice_number
        && r.kind == s.kind
        && r.to == s.recipient
        && r.from == s.sender.from_address
        && r.subject == s.subject
        && r.message == s.message_id
        && r.body == s.body_sha256
        && r.host == s.sender.host
}
fn load(db: &Connection) -> Result<BTreeMap<String, Delivery>, Error> {
    let mut q=db.prepare("SELECT event_type,entity_type,entity_id,message,actor,created_at FROM audit_log WHERE event_type LIKE 'native_invoice_email_%' ORDER BY id").map_err(invalid)?;
    let rows = q
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
            ))
        })
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    let mut out: BTreeMap<String, Delivery> = BTreeMap::new();
    let mut attempts = BTreeSet::new();
    for (kind, entity, id, canonical, actor, at) in rows {
        let e: Event = serde_json::from_str(&canonical).map_err(invalid)?;
        if e.version != 1
            || kind != format!("native_invoice_email_{}", e.event_type)
            || entity != "native_invoice_email_v1"
            || id != e.source.message_id
            || actor != format!("user:{}", e.actor)
            || at != e.at
            || !text(&e.actor, 4096)
            || !text(&e.at, 100)
            || !text(&e.attempt_id, 128)
            || e.attempt_index == 0
            || !hash(&e.previous_state_hash)
            || serde_json::to_string(&e).map_err(invalid)? != canonical
        {
            return Err(refused());
        }
        crate::cookie_session::timestamp(&e.at)?;
        validate_source(db, &e.source)?;
        let previous = out.get(&id);
        let zero = "0".repeat(64);
        if e.previous_state_hash != previous.map(|p| p.state_hash.as_str()).unwrap_or(&zero) {
            return Err(refused());
        }
        let state = match e.event_type.as_str() {
            "reserved" => {
                if e.evidence.is_some()
                    || e.resolution.is_some()
                    || previous.is_some_and(|p| {
                        p.state != "refused"
                            || p.attempt_index == u32::MAX
                            || e.source != p.source
                            || e.attempt_index != p.attempt_index + 1
                            || e.reason
                                .as_ref()
                                .is_none_or(|r| review(r, 2000).ok().as_ref() != Some(r))
                    })
                    || previous.is_none() && (e.attempt_index != 1 || e.reason.is_some())
                    || !attempts.insert(e.attempt_id.clone())
                {
                    return Err(refused());
                }
                "unknown"
            }
            "accepted" | "refused" | "unknown" => {
                let p = previous.ok_or_else(refused)?;
                if !p.pending
                    || e.source != p.source
                    || e.attempt_id != p.attempt_id
                    || e.attempt_index != p.attempt_index
                    || e.reason.is_some()
                    || e.evidence.is_some()
                    || e.resolution.is_some()
                {
                    return Err(refused());
                }
                e.event_type.as_str()
            }
            "reconciled" => {
                let p = previous.ok_or_else(refused)?;
                if p.state != "unknown"
                    || e.source != p.source
                    || e.attempt_id != p.attempt_id
                    || e.attempt_index != p.attempt_index
                    || e.reason
                        .as_ref()
                        .is_none_or(|r| review(r, 2000).ok().as_ref() != Some(r))
                    || e.evidence
                        .as_ref()
                        .is_none_or(|r| review(r, 4000).ok().as_ref() != Some(r))
                {
                    return Err(refused());
                }
                match e.resolution.as_deref() {
                    Some("accepted") => "accepted",
                    Some("not_accepted") => "refused",
                    _ => return Err(refused()),
                }
            }
            _ => return Err(refused()),
        }
        .to_owned();
        let state_hash = sha(canonical.as_bytes());
        let mut history = previous.map(|p| p.history.clone()).unwrap_or_default();
        history.push(json!({"eventType":e.event_type,"attemptId":e.attempt_id,"attemptIndex":e.attempt_index,"at":e.at,"actor":actor,"stateHash":state_hash,"reason":e.reason,"evidence":e.evidence,"resolution":e.resolution,"jsonSha256":e.source.json_sha256,"pdfSha256":e.source.pdf_sha256,"bodySha256":e.source.body_sha256,"senderIdentity":e.source.sender}));
        out.insert(
            id,
            Delivery {
                source: e.source,
                state,
                state_hash,
                attempt_id: e.attempt_id,
                attempt_index: e.attempt_index,
                pending: e.event_type == "reserved",
                history,
            },
        );
    }
    let successes = successes(db)?;
    for (id, d) in &out {
        match successes.get(id) {
            Some(r) if d.state == "accepted" && success_matches(r, &d.source) => {}
            None if d.state != "accepted" => {}
            _ => return Err(refused()),
        }
    }
    Ok(out)
}
pub fn sources(db: &Connection) -> Result<Vec<Source>, Error> {
    Ok(load(db)?.into_values().map(|r| r.source).collect())
}
pub fn validate_backup(db: &Connection) -> Result<(), Error> {
    crate::journal_preview::require_current(db)?;
    load(db)?;
    successes(db)?;
    Ok(())
}
pub fn get(db: &Connection, id: &str) -> Result<Option<Delivery>, Error> {
    Ok(load(db)?.remove(id))
}
fn legacy_row(_db: &Connection, r: &Success) -> Result<Value, Error> {
    Ok(
        json!({"invoiceDocumentId":r.doc,"invoiceNumber":r.no,"kind":r.kind,"recipient":r.to,"subject":r.subject,"messageId":r.message,"bodySha256":r.body,"pdfSha256":null,"state":"accepted","stateHash":sha(serde_json::to_vec(&json!({"legacySendLogId":r.id,"messageId":r.message,"bodySha256":r.body})).map_err(invalid)?.as_slice()),"attemptId":null,"humanResolution":false,"legacy":true,"history":[{"eventType":"legacy_accepted","sendLogId":r.id,"at":r.at,"transportReceiptAvailable":false,"historicalPdfHashAvailable":false}]}),
    )
}
pub fn rows(db: &Connection) -> Result<Vec<Value>, Error> {
    let native = load(db)?;
    let mut rows = native.values().map(|r| r.row(false)).collect::<Vec<_>>();
    for (id, r) in successes(db)? {
        if !native.contains_key(&id) {
            rows.push(legacy_row(db, &r)?);
        }
    }
    Ok(rows)
}
fn append(tx: &Transaction<'_>, e: Event) -> Result<Delivery, Error> {
    let raw = serde_json::to_string(&e).map_err(invalid)?;
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor,created_at)VALUES(?1,'native_invoice_email_v1',?2,?3,?4,?5)",params![format!("native_invoice_email_{}",e.event_type),e.source.message_id,raw,format!("user:{}",e.actor),e.at]).map_err(invalid)?;
    get(tx, &e.source.message_id)?.ok_or_else(refused)
}
fn log_success(tx: &Transaction<'_>, s: &Source, at: &str) -> Result<(), Error> {
    tx.execute("INSERT INTO email_send_log(invoice_document_id,invoice_no,kind,recipient,sender,subject,message_id,body_sha256,smtp_host,created_at)VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",params![s.invoice_document_id,s.invoice_number,s.kind,s.recipient,s.sender.from_address,s.subject,s.message_id,s.body_sha256,s.sender.host,at]).map_err(invalid)?;
    Ok(())
}
pub enum Reservation {
    Existing(Value),
    Reserved(Box<Delivery>),
}
pub fn reserve(
    tx: &Transaction<'_>,
    source: Source,
    attempt_id: &str,
    actor: &str,
    at: &str,
    retry: Option<&Retry>,
) -> Result<Reservation, Error> {
    crate::journal_post::require_current(tx)?;
    validate_source(tx, &source)?;
    let old = get(tx, &source.message_id)?;
    let (index, previous, reason) = if let Some(i) = retry {
        let reason = i.validate()?;
        let old = old.as_ref().ok_or_else(refused)?;
        if old.state != "refused"
            || i.message_id != source.message_id
            || i.expected_state_hash != old.state_hash
            || source != old.source
        {
            return Err(refused());
        }
        (
            old.attempt_index.checked_add(1).ok_or_else(refused)?,
            old.state_hash.clone(),
            Some(reason),
        )
    } else {
        if let Some(old) = old {
            if source != old.source {
                return Err(refused());
            }
            return Ok(Reservation::Existing(old.row(true)));
        }
        let native = load(tx)?;
        let legacy = successes(tx)?
            .into_values()
            .filter(|r| {
                !native.contains_key(&r.message)
                    && r.doc == source.invoice_document_id
                    && r.kind == source.kind
                    && r.to == source.recipient
                    && r.from == source.sender.from_address
            })
            .collect::<Vec<_>>();
        if legacy.len() > 1 {
            return Err(refused());
        }
        if let Some(r) = legacy.first() {
            if r.message == source.message_id && r.body != source.body_sha256 {
                return Err(refused());
            }
            let mut row = legacy_row(tx, r)?;
            row["duplicate"] = json!(true);
            return Ok(Reservation::Existing(row));
        }
        (1, "0".repeat(64), None)
    };
    Ok(Reservation::Reserved(Box::new(append(
        tx,
        Event {
            version: 1,
            source,
            event_type: "reserved".into(),
            attempt_id: attempt_id.into(),
            attempt_index: index,
            previous_state_hash: previous,
            actor: actor.into(),
            at: at.into(),
            reason,
            evidence: None,
            resolution: None,
        },
    )?)))
}
pub fn terminal(
    tx: &Transaction<'_>,
    id: &str,
    expected: &str,
    outcome: &str,
    actor: &str,
    at: &str,
) -> Result<Delivery, Error> {
    let d = get(tx, id)?.ok_or_else(refused)?;
    if !d.pending
        || d.state_hash != expected
        || !matches!(outcome, "accepted" | "refused" | "unknown")
    {
        return Err(refused());
    }
    if outcome == "accepted" {
        log_success(tx, &d.source, at)?;
    }
    append(
        tx,
        Event {
            version: 1,
            source: d.source,
            event_type: outcome.into(),
            attempt_id: d.attempt_id,
            attempt_index: d.attempt_index,
            previous_state_hash: d.state_hash,
            actor: actor.into(),
            at: at.into(),
            reason: None,
            evidence: None,
            resolution: None,
        },
    )
}
pub fn reconcile(
    tx: &Transaction<'_>,
    i: &Reconcile,
    actor: &str,
    at: &str,
) -> Result<Delivery, Error> {
    let (reason, evidence) = i.validate()?;
    let d = get(tx, &i.message_id)?.ok_or_else(refused)?;
    if d.state != "unknown" || i.expected_state_hash != d.state_hash {
        return Err(refused());
    }
    if i.resolution == "accepted" {
        log_success(tx, &d.source, at)?;
    }
    append(
        tx,
        Event {
            version: 1,
            source: d.source,
            event_type: "reconciled".into(),
            attempt_id: d.attempt_id,
            attempt_index: d.attempt_index,
            previous_state_hash: d.state_hash,
            actor: actor.into(),
            at: at.into(),
            reason: Some(reason),
            evidence: Some(evidence),
            resolution: Some(i.resolution.clone()),
        },
    )
}
pub fn timestamp(now: i64) -> Result<String, Error> {
    crate::mfa_login::date(now)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_review_envelopes_reject_unconfirmed_unknown_fields_and_invalid_evidence() {
        assert!(serde_json::from_value::<Send>(json!({"invoiceDocumentId":1,"to":"a@example.invalid","confirm":true,"actor":"forged"})).is_err());
        for to in [
            "a@example.invalid\r\nBcc: other@example.invalid",
            "a@example.invalid,b@example.invalid",
            "a@@example.invalid",
            "",
        ] {
            assert!(recipient(to).is_err());
        }
        assert_eq!(
            recipient(" a@example.invalid ").unwrap(),
            "a@example.invalid"
        );
        let i = Send {
            invoice_document_id: 1,
            to: "a@example.invalid".into(),
            kind: None,
            confirm: false,
        };
        assert!(i.validate().is_err());
        let mut r = Retry {
            message_id: "<11111111111111111111111111111111@rentemester.local>".into(),
            expected_state_hash: "1".repeat(64),
            reason: "Reviewed SMTP refusal".into(),
            confirm: true,
        };
        assert!(r.validate().is_ok());
        r.reason = "x".repeat(2001);
        assert!(r.validate().is_err());
        r.reason = "Reviewed\nSMTP refusal".into();
        assert!(r.validate().is_err());
        let mut r = Reconcile {
            message_id: r.message_id,
            expected_state_hash: r.expected_state_hash,
            resolution: "accepted".into(),
            reason: "Reviewed SMTP evidence".into(),
            evidence: "Synthetic operator log ref".into(),
            confirm: true,
        };
        assert!(r.validate().is_ok());
        r.evidence = "x".repeat(4001);
        assert!(r.validate().is_err());
        r.evidence = "no proof".into();
        assert!(r.validate().is_err());
    }
}
