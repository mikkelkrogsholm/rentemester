//! Exact original handoffs share the append-only submission table. Canonical
//! audit provenance, never an XML prefix, authorizes bypassing a preparation.
use super::*;
use rentemester_engine::peppol_handoff::{AccessPoint, Acknowledgement};
const AUDIT: &str = "original_public_einvoice_handoff";
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Evidence {
    version: u8,
    invoice_document_id: i64,
    invoice_number: String,
    ledger_uuid: String,
    json_sha256: String,
    access_point: AccessPoint,
    acknowledgement: Option<Acknowledgement>,
    oioubl_sha256: String,
    row_sha256: String,
    actor: String,
    at: String,
}
fn receiver(value: &str) -> String {
    if value.starts_with("GLN:") {
        value.into()
    } else {
        format!("GLN:{value}")
    }
}
fn source(db: &Connection, id: i64) -> Result<(String, String, String, Value), Error> {
    let (no,sha,raw,uuid):(String,String,String,String)=db.query_row("SELECT d.invoice_no,d.sha256_hash,d.payload_json,l.ledger_uuid FROM documents d CROSS JOIN ledger_identity l WHERE d.id=? AND d.document_type='issued_invoice' AND l.id=1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).map_err(sql)?;
    ensure(hash(raw.as_bytes()) == sha)?;
    let payload: Value = serde_json::from_str(&raw).map_err(|_| refused())?;
    ensure(payload["invoiceNumber"] == no)?;
    Ok((no, sha, uuid, payload))
}
pub(super) fn verified(db: &Connection, id: i64, r: &Row) -> Result<bool, Error> {
    let mut q=db.prepare("SELECT message,actor,created_at FROM audit_log WHERE event_type=? AND entity_type='peppol_submission' AND entity_id=?").map_err(sql)?;
    let audits = q
        .query_map(params![AUDIT, id.to_string()], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    if audits.is_empty() {
        return Ok(false);
    }
    ensure(audits.len() == 1)?;
    let (raw, actor, at) = &audits[0];
    let e: Evidence = serde_json::from_str(raw).map_err(|_| refused())?;
    ensure(
        e.version == 1
            && canonical(&e)? == *raw
            && text(actor, 512)
            && text(at, 64)
            && e.actor == *actor
            && e.at == *at
            && e.row_sha256 == hash(canonical(r)?.as_bytes()),
    )?;
    let (no, sha, uuid, payload) = source(db, r.invoice)?;
    let export = rentemester_engine::public_einvoice::export(&no, &payload)?;
    let h = rentemester_engine::peppol_handoff::prepare(
        &no,
        &export,
        &e.access_point,
        e.acknowledgement.as_ref(),
    )?;
    ensure(
        e.invoice_document_id == r.invoice
            && e.invoice_number == no
            && e.ledger_uuid == uuid
            && e.json_sha256 == sha
            && e.oioubl_sha256 == export.sha256
            && r.no.as_deref() == Some(no.as_str())
            && r.key == h.idempotency_key
            && r.reference == h.submission_reference
            && r.envelope == h.envelope
            && r.envelope_hash == h.envelope_sha256
            && r.xml_hash == h.oioubl_sha256
            && r.receiver == receiver(&export.receiver_endpoint_id)
            && r.access == e.access_point.access_point_id.trim()
            && r.status == h.status
            && r.transmission == h.transmission_id
            && r.ack
                == e.acknowledgement
                    .as_ref()
                    .map(|a| a.acknowledged_at.clone())
            && r.created == e.at,
    )?;
    let events: i64 = db
        .query_row(
            "SELECT count(*) FROM peppol_submission_events WHERE submission_id=?",
            [id],
            |r| r.get(0),
        )
        .map_err(sql)?;
    ensure(events == 0)?;
    Ok(true)
}
pub(super) fn safe(db: &Connection, id: i64, r: &Row) -> Result<bool, Error> {
    Ok(verified(db, id, r)?
        && r.status == "prepared"
        && r.transmission.is_none()
        && r.ack.is_none())
}
pub(super) fn validate_audits(db: &Connection) -> Result<(), Error> {
    let malformed:i64=db.query_row("SELECT count(*) FROM audit_log a WHERE a.event_type=? AND (a.entity_type!='peppol_submission' OR NOT EXISTS(SELECT 1 FROM peppol_submissions p WHERE CAST(p.id AS TEXT)=a.entity_id))",[AUDIT],|r|r.get(0)).map_err(sql)?;
    ensure(malformed == 0)?;
    let mut q = db
        .prepare("SELECT entity_id FROM audit_log WHERE event_type=?")
        .map_err(sql)?;
    let ids = q
        .query_map([AUDIT], |r| r.get::<_, String>(0))
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    for id in ids {
        let id: i64 = id.parse().map_err(|_| refused())?;
        ensure(verified(db, id, &raw(db, id)?)?)?;
    }
    Ok(())
}
pub fn prepare_original(
    tx: &Transaction<'_>,
    id: i64,
    access: &AccessPoint,
    ack: Option<&Acknowledgement>,
    actor: &str,
    now: &str,
) -> Result<Value, Error> {
    validate_backup(tx)?;
    ensure(text(actor, 512) && text(now, 64))?;
    let (no, sha, uuid, payload) = source(tx, id)?;
    let export = rentemester_engine::public_einvoice::export(&no, &payload)?;
    let h = rentemester_engine::peppol_handoff::prepare(&no, &export, access, ack)?;
    let existing: Option<i64> = tx
        .query_row(
            "SELECT id FROM peppol_submissions WHERE idempotency_key=?",
            [&h.idempotency_key],
            |r| r.get(0),
        )
        .optional()
        .map_err(sql)?;
    if let Some(existing) = existing {
        let r = raw(tx, existing)?;
        ensure(r.invoice == id && verified(tx, existing, &r)?)?;
        return result(tx, existing, true);
    }
    // A manual attestation cannot race behind an admitted provider reservation.
    // The same immediate transaction protects this test and the append.
    if ack.is_some() {
        ensure(blocking_id(tx, id)?.is_none())?;
    }
    tx.execute("INSERT INTO peppol_submissions(invoice_document_id,invoice_no,idempotency_key,submission_reference,access_point_id,receiver_endpoint_id,oioubl_sha256,envelope_sha256,envelope_xml,status,transmission_id,acknowledged_at,created_at)VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)",params![id,no,h.idempotency_key,h.submission_reference,access.access_point_id.trim(),receiver(&export.receiver_endpoint_id),h.oioubl_sha256,h.envelope_sha256,h.envelope,h.status,h.transmission_id,ack.map(|a|a.acknowledged_at.as_str()),now]).map_err(sql)?;
    let row_id = tx.last_insert_rowid();
    let e = Evidence {
        version: 1,
        invoice_document_id: id,
        invoice_number: no,
        ledger_uuid: uuid,
        json_sha256: sha,
        access_point: serde_json::from_value(serde_json::to_value(access).map_err(|_| refused())?)
            .map_err(|_| refused())?,
        acknowledgement: ack
            .map(|a| {
                serde_json::from_value(serde_json::to_value(a).map_err(|_| refused())?)
                    .map_err(|_| refused())
            })
            .transpose()?,
        oioubl_sha256: export.sha256,
        row_sha256: hash(canonical(&raw(tx, row_id)?)?.as_bytes()),
        actor: actor.into(),
        at: now.into(),
    };
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor,created_at)VALUES(?,'peppol_submission',?,?,?,?)",params![AUDIT,row_id.to_string(),canonical(&e)?,actor,now]).map_err(sql)?;
    ensure(verified(tx, row_id, &raw(tx, row_id)?)?)?;
    result(tx, row_id, false)
}
fn result(db: &Connection, id: i64, duplicate: bool) -> Result<Value, Error> {
    let mut v = project(db, id)?;
    v["ok"] = json!(true);
    v["duplicate"] = json!(duplicate);
    v["appliedRules"] = json!(["DK-PEPPOL-SUBMIT-001"]);
    v["errors"] = json!([]);
    Ok(v)
}
