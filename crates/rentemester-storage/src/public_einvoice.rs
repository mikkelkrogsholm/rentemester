//! Durable public-invoice transport evidence. Caller holds current authority and transactions.
use rentemester_engine::Error;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
mod handoff;
mod registration;
pub use handoff::prepare_original;
pub use registration::{
    Identity, Registration, identity, load_bound, outbound_registered, registration, save_key,
    save_outbound,
};
const PREFIX: &str = "<NativePeppolSubmission xmlns=\"urn:rentemester-native-peppol:1\"><Evidence>";
const SUFFIX: &str = "</Evidence></NativePeppolSubmission>";
const MAX_ENVELOPE: usize = 24 * 1024 * 1024;
fn refused() -> Error {
    Error::new(
        "public_einvoice_refused",
        "Public invoice evidence or current company binding refused",
    )
}
fn sql(_: rusqlite::Error) -> Error {
    refused()
}
fn ensure(ok: bool) -> Result<(), Error> {
    if ok { Ok(()) } else { Err(refused()) }
}
fn text(s: &str, n: usize) -> bool {
    !s.trim().is_empty() && s == s.trim() && s.len() <= n && !s.chars().any(char::is_control)
}
fn hash(s: &[u8]) -> String {
    format!("{:x}", Sha256::digest(s))
}
fn canonical<T: Serialize>(v: &T) -> Result<String, Error> {
    crate::workspace_inbox::canonical(&serde_json::to_value(v).map_err(|_| refused())?)
}
fn provider_status(s: &str) -> bool {
    [
        "delivered",
        "queued-for-delivery",
        "document-not-valid",
        "unable-to-deliver",
        "unknown-server-error",
        "temporary-upstream-error",
    ]
    .contains(&s)
}
fn environment(s: &str) -> Result<(), Error> {
    ensure(matches!(s, "test" | "production"))
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Source {
    pub invoice_document_id: i64,
    pub invoice_number: String,
    pub ledger_uuid: String,
    pub json_sha256: String,
    pub company_key: String,
    pub environment: String,
    pub oioubl_xml: String,
    pub oioubl_sha256: String,
    pub receiver_endpoint_id: String,
}
/// Reads the immutable issued snapshot. Physical JSON/PDF verification remains the held host's duty.
pub fn load_source(
    db: &Connection,
    id: i64,
    company_key: &str,
    env: &str,
) -> Result<Source, Error> {
    ensure(
        id > 0
            && load_bound(db, env)?.as_deref() == Some(company_key)
            && outbound_registered(db, env, company_key)?,
    )?;
    let (no,sha,payload):(String,String,String)=db.query_row("SELECT invoice_no,sha256_hash,payload_json FROM documents WHERE id=? AND document_type='issued_invoice'",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(sql)?;
    let payload: Value = serde_json::from_str(&payload).map_err(|_| refused())?;
    let export = rentemester_engine::public_einvoice::export(&no, &payload)?;
    let uuid = db
        .query_row(
            "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
            [],
            |r| r.get(0),
        )
        .map_err(sql)?;
    let s = Source {
        invoice_document_id: id,
        invoice_number: no,
        ledger_uuid: uuid,
        json_sha256: sha,
        company_key: company_key.into(),
        environment: env.into(),
        oioubl_xml: export.xml,
        oioubl_sha256: export.sha256,
        receiver_endpoint_id: export.receiver_endpoint_id,
    };
    validate_source(db, &s)?;
    Ok(s)
}
fn validate_source(db: &Connection, s: &Source) -> Result<(), Error> {
    environment(&s.environment)?;
    ensure(
        text(&s.company_key, 512)
            && text(&s.invoice_number, 256)
            && s.oioubl_xml.len() <= 10 * 1024 * 1024
            && hash(s.oioubl_xml.as_bytes()) == s.oioubl_sha256,
    )?;
    let (no,sha,raw,uuid):(String,String,String,String)=db.query_row("SELECT d.invoice_no,d.sha256_hash,d.payload_json,l.ledger_uuid FROM documents d CROSS JOIN ledger_identity l WHERE d.id=? AND d.document_type='issued_invoice' AND l.id=1",[s.invoice_document_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).map_err(sql)?;
    let payload: Value = serde_json::from_str(&raw).map_err(|_| refused())?;
    let export = rentemester_engine::public_einvoice::export(&no, &payload)?;
    ensure(
        s.invoice_number == no
            && payload["invoiceNumber"] == no
            && s.json_sha256 == sha
            && hash(raw.as_bytes()) == sha
            && s.ledger_uuid == uuid
            && export.xml == s.oioubl_xml
            && export.sha256 == s.oioubl_sha256
            && export.receiver_endpoint_id == s.receiver_endpoint_id,
    )
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Envelope {
    version: String,
    source: Source,
    actor: String,
    at: String,
}
fn encode(e: &Envelope) -> Result<String, Error> {
    let raw = canonical(e)?;
    let mut out = String::with_capacity(PREFIX.len() + raw.len() * 2 + SUFFIX.len());
    out.push_str(PREFIX);
    for b in raw.bytes() {
        use std::fmt::Write;
        write!(&mut out, "{b:02x}").map_err(|_| refused())?;
    }
    out.push_str(SUFFIX);
    ensure(out.len() <= MAX_ENVELOPE)?;
    Ok(out)
}
fn decode(raw: &str) -> Result<Envelope, Error> {
    ensure(raw.len() <= MAX_ENVELOPE)?;
    let hex = raw
        .strip_prefix(PREFIX)
        .and_then(|s| s.strip_suffix(SUFFIX))
        .ok_or_else(refused)?;
    ensure(
        hex.len() % 2 == 0
            && hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
    )?;
    let bytes = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| refused()))
        .collect::<Result<Vec<_>, _>>()?;
    let e: Envelope = serde_json::from_slice(&bytes).map_err(|_| refused())?;
    ensure(
        e.version == "rentemester-native-peppol/1"
            && text(&e.actor, 512)
            && text(&e.at, 64)
            && encode(&e)? == raw,
    )?;
    Ok(e)
}
pub struct Reservation {
    pub id: i64,
    pub should_deliver: bool,
    pub result: Value,
}
#[derive(Clone, Debug)]
pub struct Observed {
    pub document_id: Option<String>,
    pub status: String,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Event {
    version: u8,
    previous_hash: String,
    event_type: String,
    document_id: Option<String>,
    status: String,
    actor: String,
    at: String,
}
#[derive(Default)]
struct State {
    document_id: Option<String>,
    status: String,
    head: String,
    history: Vec<Value>,
}
fn event(
    tx: &Transaction<'_>,
    id: i64,
    kind: &str,
    o: &Observed,
    actor: &str,
    at: &str,
    previous: &str,
) -> Result<(), Error> {
    ensure(
        text(actor, 512)
            && text(at, 64)
            && text(&o.status, 128)
            && o.document_id.as_ref().is_none_or(|d| text(d, 512)),
    )?;
    let e = Event {
        version: 1,
        previous_hash: previous.into(),
        event_type: kind.into(),
        document_id: o.document_id.clone(),
        status: o.status.clone(),
        actor: actor.into(),
        at: at.into(),
    };
    let raw = canonical(&e)?;
    tx.execute("INSERT INTO peppol_submission_events(submission_id,event_type,document_id,status,observed_at,message)VALUES(?,?,?,?,?,?)",params![id,kind,o.document_id,o.status,at,raw]).map_err(sql)?;
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor,created_at)VALUES('native_public_einvoice_event','peppol_submission',?,?,?,?)",params![id.to_string(),raw,actor,at]).map_err(sql)?;
    Ok(())
}
fn state(db: &Connection, id: i64, initial: &str) -> Result<State, Error> {
    let mut s = State {
        head: initial.into(),
        status: "uncertain".into(),
        ..State::default()
    };
    let mut q=db.prepare("SELECT event_type,document_id,status,observed_at,message,public_url FROM peppol_submission_events WHERE submission_id=? ORDER BY id").map_err(sql)?;
    let rows = q
        .query_map([id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, Option<String>>(5)?,
            ))
        })
        .map_err(sql)?;
    for row in rows {
        let (kind, doc, status, at, raw, url) = row.map_err(sql)?;
        let e: Event = serde_json::from_str(&raw).map_err(|_| refused())?;
        ensure(
            e.version == 1
                && e.previous_hash == s.head
                && e.event_type == kind
                && e.document_id == doc
                && Some(&e.status) == status.as_ref()
                && e.at == at
                && url.is_none()
                && text(&e.actor, 512)
                && text(&e.at, 64)
                && text(&e.status, 128)
                && canonical(&e)? == raw,
        )?;
        if kind != "delivery_reserved" && kind != "delivery_failed" {
            ensure(provider_status(&e.status))?;
        }
        if s.history.is_empty() {
            ensure(
                kind == "delivery_reserved" && doc.is_none() && e.status == "delivery-uncertain",
            )?;
        } else {
            ensure(kind != "delivery_reserved")?;
            match kind.as_str() {
                "delivery_failed" => ensure(
                    s.document_id.is_none() && doc.is_none() && e.status == "delivery-uncertain",
                )?,
                "queued" => {
                    ensure(s.document_id.is_none() && doc.as_ref().is_some_and(|d| text(d, 512)))?
                }
                "status_observed" => ensure(s.document_id.is_some() && s.document_id == doc)?,
                "delivered" => ensure(
                    e.status == "delivered"
                        && doc.as_ref().is_some_and(|d| text(d, 512))
                        && (s.document_id.is_none() || s.document_id == doc),
                )?,
                _ => return Err(refused()),
            }
            ensure(s.status != "delivered" || e.status == "delivered")?;
        }
        let audit:i64=db.query_row("SELECT count(*) FROM audit_log WHERE event_type='native_public_einvoice_event' AND entity_type='peppol_submission' AND entity_id=? AND message=? AND actor=? AND created_at=?",params![id.to_string(),raw,e.actor,e.at],|r|r.get(0)).map_err(sql)?;
        ensure(audit == 1)?;
        s.head = hash(raw.as_bytes());
        if doc.is_some() {
            s.document_id = doc;
        }
        s.status = e.status.clone();
        s.history
            .push(serde_json::to_value(e).map_err(|_| refused())?);
    }
    ensure(!s.history.is_empty())?;
    Ok(s)
}
/// ANY earlier row for this invoice blocks a second delivery, including legacy and changed credentials.
pub fn reserve(
    tx: &Transaction<'_>,
    source: &Source,
    actor: &str,
    now: &str,
) -> Result<Reservation, Error> {
    validate_backup(tx)?;
    if let Some(id) = blocking_id(tx, source.invoice_document_id)? {
        return Ok(Reservation {
            id,
            should_deliver: false,
            result: project(tx, id)?,
        });
    }
    ensure(
        text(actor, 512)
            && text(now, 64)
            && load_bound(tx, &source.environment)?.as_deref() == Some(source.company_key.as_str())
            && outbound_registered(tx, &source.environment, &source.company_key)?,
    )?;
    validate_source(tx, source)?;
    let envelope = encode(&Envelope {
        version: "rentemester-native-peppol/1".into(),
        source: source.clone(),
        actor: actor.into(),
        at: now.into(),
    })?;
    let sha = hash(envelope.as_bytes());
    let key = hash(canonical(source)?.as_bytes());
    let reference = format!("PEPPOL-{}-{}", source.invoice_number, &key[..12]);
    tx.execute("INSERT INTO peppol_submissions(invoice_document_id,invoice_no,idempotency_key,submission_reference,access_point_id,receiver_endpoint_id,oioubl_sha256,envelope_sha256,envelope_xml,status,created_at)VALUES(?,?,?,?,?,?,?,?,?,'prepared',?)",params![source.invoice_document_id,source.invoice_number,key,reference,"digisense",source.receiver_endpoint_id,source.oioubl_sha256,sha,envelope,now]).map_err(sql)?;
    let id = tx.last_insert_rowid();
    event(
        tx,
        id,
        "delivery_reserved",
        &Observed {
            document_id: None,
            status: "delivery-uncertain".into(),
        },
        actor,
        now,
        &sha,
    )?;
    Ok(Reservation {
        id,
        should_deliver: true,
        result: {
            let mut result = project(tx, id)?;
            result["duplicate"] = json!(false);
            result
        },
    })
}
pub fn record_outcome(
    tx: &Transaction<'_>,
    id: i64,
    o: Observed,
    actor: &str,
    now: &str,
) -> Result<Value, Error> {
    validate_backup(tx)?;
    let row = raw(tx, id)?;
    ensure(row.envelope.starts_with(PREFIX))?;
    let s = state(tx, id, &row.envelope_hash)?;
    ensure(text(&o.status, 128) && o.document_id.as_ref().is_none_or(|d| text(d, 512)))?;
    if s.document_id == o.document_id && s.status == o.status {
        return project(tx, id);
    }
    if s.document_id.is_some() {
        ensure(s.document_id == o.document_id)?;
    }
    if o.document_id.is_some() {
        ensure(provider_status(&o.status))?;
    }
    let kind = if o.document_id.is_none() {
        ensure(o.status == "delivery-uncertain")?;
        "delivery_failed"
    } else if o.status == "delivered" {
        "delivered"
    } else if s.document_id.is_none() {
        "queued"
    } else {
        "status_observed"
    };
    ensure(s.status != "delivered" || o.status == "delivered")?;
    event(tx, id, kind, &o, actor, now, &s.head)?;
    project(tx, id)
}
#[derive(Serialize)]
struct Row {
    invoice: i64,
    no: Option<String>,
    key: String,
    reference: String,
    access: String,
    receiver: String,
    xml_hash: String,
    envelope_hash: String,
    envelope: String,
    status: String,
    transmission: Option<String>,
    ack: Option<String>,
    created: String,
}
fn raw(db: &Connection, id: i64) -> Result<Row, Error> {
    db.query_row("SELECT invoice_document_id,invoice_no,idempotency_key,submission_reference,access_point_id,receiver_endpoint_id,oioubl_sha256,envelope_sha256,envelope_xml,status,transmission_id,acknowledged_at,created_at FROM peppol_submissions WHERE id=?",[id],|r|Ok(Row{invoice:r.get(0)?,no:r.get(1)?,key:r.get(2)?,reference:r.get(3)?,access:r.get(4)?,receiver:r.get(5)?,xml_hash:r.get(6)?,envelope_hash:r.get(7)?,envelope:r.get(8)?,status:r.get(9)?,transmission:r.get(10)?,ack:r.get(11)?,created:r.get(12)?})).map_err(sql)
}
fn project(db: &Connection, id: i64) -> Result<Value, Error> {
    let r = raw(db, id)?;
    if !r.envelope.starts_with(PREFIX) {
        return Ok(
            json!({"id":id,"invoiceDocumentId":r.invoice,"invoiceNumber":r.no,"submissionReference":r.reference,"idempotencyKey":r.key,"oioublSha256":r.xml_hash,"envelopeSha256":r.envelope_hash,"envelope":r.envelope,"status":r.status,"transmissionId":r.transmission,"legacy":!handoff::verified(db,id,&r)?,"duplicate":true,"redeliveryBlocked":!handoff::safe(db,id,&r)?}),
        );
    }
    let e = decode(&r.envelope)?;
    validate_source(db, &e.source)?;
    let s = state(db, id, &r.envelope_hash)?;
    let status = if s.status == "delivered" {
        "acknowledged"
    } else if [
        "document-not-valid",
        "unable-to-deliver",
        "unknown-server-error",
    ]
    .contains(&s.status.as_str())
    {
        "failed"
    } else if s.document_id.is_some() {
        "prepared"
    } else {
        "uncertain"
    };
    Ok(
        json!({"id":id,"invoiceDocumentId":r.invoice,"invoiceNumber":r.no,"submissionReference":r.reference,"idempotencyKey":r.key,"oioublSha256":r.xml_hash,"envelopeSha256":r.envelope_hash,"envelope":r.envelope,"status":status,"providerStatus":s.status,"transmissionId":s.document_id,"companyKey":e.source.company_key,"environment":e.source.environment,"stateHash":s.head,"history":s.history,"legacy":false,"duplicate":true,"redeliveryBlocked":true}),
    )
}
// Only positively verified new preparations without any attestation or event may be bypassed.
fn blocking_id(db: &Connection, invoice_id: i64) -> Result<Option<i64>, Error> {
    let mut q = db
        .prepare("SELECT id FROM peppol_submissions WHERE invoice_document_id=? ORDER BY id")
        .map_err(sql)?;
    let ids = q
        .query_map([invoice_id], |r| r.get(0))
        .map_err(sql)?
        .collect::<Result<Vec<i64>, _>>()
        .map_err(sql)?;
    for id in ids {
        let r = raw(db, id)?;
        if !handoff::safe(db, id, &r)? {
            return Ok(Some(id));
        }
    }
    Ok(None)
}
/// Read prior invoice state without needing current credentials or registration.
pub fn get(db: &Connection, invoice_id: i64) -> Result<Option<Value>, Error> {
    validate_backup(db)?;
    let id = blocking_id(db, invoice_id)?;
    id.map(|id| project(db, id)).transpose()
}
pub fn accepted(db: &Connection, invoice_id: i64) -> Result<Option<(i64, Source, String)>, Error> {
    validate_backup(db)?;
    let id = blocking_id(db, invoice_id)?;
    let Some(id) = id else { return Ok(None) };
    let r = raw(db, id)?;
    // Legacy rows retain their original projection but never acquire native transport authority.
    if !r.envelope.starts_with(PREFIX) {
        return Ok(None);
    }
    let e = decode(&r.envelope)?;
    let s = state(db, id, &r.envelope_hash)?;
    Ok(s.document_id.map(|d| (id, e.source, d)))
}
pub fn rows(db: &Connection) -> Result<Value, Error> {
    validate_backup(db)?;
    let mut q = db
        .prepare("SELECT id FROM peppol_submissions ORDER BY id")
        .map_err(sql)?;
    let ids = q
        .query_map([], |r| r.get(0))
        .map_err(sql)?
        .collect::<Result<Vec<i64>, _>>()
        .map_err(sql)?;
    let mut submissions = ids
        .into_iter()
        .map(|id| project(db, id))
        .collect::<Result<Vec<_>, _>>()?;
    // First-per-invoice consumers must see durable delivery/attestation before
    // an earlier harmless artifact preparation. Preserve every immutable row.
    submissions.sort_by_key(|r| {
        (
            r["invoiceDocumentId"].as_i64().unwrap_or(0),
            r["redeliveryBlocked"] == false,
            r["id"].as_i64().unwrap_or(0),
        )
    });
    Ok(json!({"submissions":submissions}))
}
pub fn validate_backup(db: &Connection) -> Result<(), Error> {
    registration::validate(db)?;
    let mut q = db
        .prepare("SELECT id FROM peppol_submissions ORDER BY id")
        .map_err(sql)?;
    let ids = q
        .query_map([], |r| r.get(0))
        .map_err(sql)?
        .collect::<Result<Vec<i64>, _>>()
        .map_err(sql)?;
    let mut native_events = 0usize;
    for id in ids {
        let r = raw(db, id)?;
        ensure(r.envelope.len() <= MAX_ENVELOPE && hash(r.envelope.as_bytes()) == r.envelope_hash)?;
        if r.envelope.starts_with(PREFIX) {
            let e = decode(&r.envelope)?;
            validate_source(db, &e.source)?;
            ensure(
                r.invoice == e.source.invoice_document_id
                    && r.no.as_deref() == Some(e.source.invoice_number.as_str())
                    && r.key == hash(canonical(&e.source)?.as_bytes())
                    && r.reference
                        == format!("PEPPOL-{}-{}", e.source.invoice_number, &r.key[..12])
                    && r.access == "digisense"
                    && r.receiver == e.source.receiver_endpoint_id
                    && r.xml_hash == e.source.oioubl_sha256
                    && r.status == "prepared"
                    && r.transmission.is_none()
                    && r.ack.is_none()
                    && r.created == e.at,
            )?;
            registration::historical_binding(db, &e.source)?;
            let state = state(db, id, &r.envelope_hash)?;
            ensure(state.history[0]["actor"] == e.actor && state.history[0]["at"] == e.at)?;
            native_events += state.history.len();
        } else {
            let exists:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM documents WHERE id=? AND document_type='issued_invoice')",[r.invoice],|r|r.get(0)).map_err(sql)?;
            ensure(exists)?;
            handoff::verified(db, id, &r)?;
        }
    }
    let audits: i64 = db
        .query_row(
            "SELECT count(*) FROM audit_log WHERE event_type='native_public_einvoice_event'",
            [],
            |r| r.get(0),
        )
        .map_err(sql)?;
    ensure(audits == native_events as i64)?;
    handoff::validate_audits(db)?;
    Ok(())
}
#[cfg(test)]
mod tests;
