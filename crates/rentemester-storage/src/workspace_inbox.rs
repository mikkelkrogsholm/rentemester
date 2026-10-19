//! Canonical workspace ingress evidence. Caller owns current authorization and transactions.
use rentemester_engine::{Error, company_knowledge::iso};
use rusqlite::{OptionalExtension, Transaction, params};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
fn sql(_: rusqlite::Error) -> Error {
    refused("inbox storage unavailable; abort transaction")
}
pub fn refused(message: &str) -> Error {
    Error::new("workspace_inbox_refused", message)
}
fn text(s: &str, max: usize) -> Result<String, Error> {
    let value = rentemester_engine::company_knowledge::text(s, max)?;
    if value.chars().any(char::is_control) {
        return Err(refused("inbox text contains control characters"));
    }
    Ok(value)
}
fn missing() -> Error {
    Error::new("workspace_inbox_not_found", "workspace source unavailable")
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Ingest {
    pub source_id: Option<String>,
    pub idempotency_key: String,
    pub bytes_base64: String,
    pub filename: String,
    pub mime_type: String,
    pub transport: String,
    pub transport_identity: Option<String>,
    pub received_at: String,
    pub metadata: Value,
    #[serde(default)]
    pub candidates: Vec<Candidate>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Candidate {
    pub company_slug: String,
    pub evidence_kind: String,
    pub evidence: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Destination {
    pub company_slug: String,
}
fn canonical(v: &Value) -> Result<String, Error> {
    fn value(v: &Value, depth: usize) -> Result<String, Error> {
        if depth > 32 {
            return Err(refused("inbox JSON nesting exceeds bound"));
        }
        Ok(match v {
            Value::Object(o) => {
                let mut keys = o.keys().collect::<Vec<_>>();
                keys.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
                format!(
                    "{{{}}}",
                    keys.into_iter()
                        .map(|k| Ok(format!("{}:{}", json!(k), value(&o[k], depth + 1)?)))
                        .collect::<Result<Vec<_>, Error>>()?
                        .join(",")
                )
            }
            Value::Array(a) => format!(
                "[{}]",
                a.iter()
                    .map(|v| value(v, depth + 1))
                    .collect::<Result<Vec<_>, _>>()?
                    .join(",")
            ),
            Value::Number(n) => ryu_js::Buffer::new()
                .format_finite(
                    n.as_f64()
                        .filter(|n| n.is_finite())
                        .ok_or_else(|| refused("invalid JSON number"))?,
                )
                .into(),
            _ => v.to_string(),
        })
    }
    value(v, 0)
}
fn event(
    tx: &Transaction<'_>,
    id: &str,
    kind: &str,
    payload: Value,
    actor: &str,
    now: &str,
) -> Result<(), Error> {
    let body = canonical(&payload)?;
    tx.execute("INSERT INTO rm_workspace_inbox_events(source_id,event_type,payload_hash,canonical_payload,actor,created_at)VALUES(?,?,?,?,?,?)",params![id,kind,format!("{:x}",Sha256::digest(body.as_bytes())),body,actor,now]).map_err(sql)?;
    Ok(())
}
pub struct Provenance<'a> {
    pub actor: &'a str,
    pub now: &'a str,
}
pub fn ingest(
    tx: &Transaction<'_>,
    anchor: &str,
    i: &Ingest,
    bytes: &[u8],
    generated: &str,
    visible: &BTreeSet<String>,
    provenance: Provenance<'_>,
) -> Result<Value, Error> {
    let Provenance { actor, now } = provenance;
    if i.transport != "upload" {
        return Err(refused("native inbox supports explicit uploads only"));
    }
    if bytes.is_empty() || bytes.len() > 25 * 1024 * 1024 {
        return Err(refused("source bytes required; maximum 25 MiB"));
    }
    let key = text(&i.idempotency_key, 200)?;
    let filename = text(&i.filename, 255)?;
    let mime = text(&i.mime_type, 160)?;
    let transport_id = i
        .transport_identity
        .as_deref()
        .map(|v| text(v, 512))
        .transpose()?;
    let received = iso(&i.received_at)?;
    if !i.metadata.is_object() {
        return Err(refused("metadata must be an object"));
    }
    let metadata = canonical(&i.metadata)?;
    if metadata.len() > 64 * 1024 {
        return Err(refused("metadata exceeds 64 KiB"));
    }
    let hash = format!("{:x}", Sha256::digest(bytes));
    let prior: Option<String> = tx
        .query_row(
            "SELECT source_id FROM rm_workspace_inbox_sources WHERE idempotency_key=?",
            [&key],
            |r| r.get(0),
        )
        .optional()
        .map_err(sql)?;
    // An idempotency hit cannot disclose another anchor or silently accept changed intent.
    if let Some(id) = prior {
        let old = inspect(tx, anchor, &id, visible)?;
        let old_bytes = original(tx, &id)?;
        if i.source_id
            .as_deref()
            .is_some_and(|provided| provided != id)
            || old_bytes != bytes
            || old["filename"] != filename
            || old["mimeType"] != mime
            || old["transport"] != i.transport
            || old["transportIdentity"] != json!(transport_id)
            || old["receivedAt"] != received
            || old["metadata"] != i.metadata
            || old["createdBy"] != actor
        {
            return Err(refused(
                "inbox key belongs to different original intent/principal",
            ));
        }
        let planned = candidates(i, visible)?;
        let old_candidates = old["history"]
            .as_array()
            .and_then(|h| h.iter().find(|e| e["event_type"] == "candidate_resolved"))
            .map(|e| e["payload"]["candidates"].clone())
            .ok_or_else(|| refused("candidate proof unavailable"))?;
        if old_candidates != json!(planned) {
            return Err(refused("inbox key belongs to different candidate intent"));
        }
        return Ok(old);
    }
    let id = text(i.source_id.as_deref().unwrap_or(generated), 100)?;
    let candidates = candidates(i, visible)?;
    tx.execute("INSERT INTO rm_workspace_inbox_sources(source_id,visibility_anchor_slug,idempotency_key,original_bytes,sha256,filename,mime_type,transport,transport_identity,received_at,metadata_json,created_by,created_at)VALUES(?,?,?,?,?,?,?,'upload',?,?,?,?,?)",params![id,anchor,key,bytes,hash,filename,mime,transport_id,received,metadata,actor,now]).map_err(sql)?;
    event(
        tx,
        &id,
        "ingested",
        json!({"sourceHash":hash,"filename":filename,"mimeType":mime,"transport":"upload","transportIdentity":transport_id}),
        actor,
        now,
    )?;
    event(
        tx,
        &id,
        "candidate_resolved",
        json!({"candidates":candidates}),
        actor,
        now,
    )?;
    let companies = candidates
        .iter()
        .filter_map(|c| c["companySlug"].as_str())
        .collect::<BTreeSet<_>>();
    if companies.len() != 1 {
        let code = if companies.is_empty() {
            "INBOX_NO_CANDIDATE"
        } else {
            "INBOX_AMBIGUOUS_CANDIDATES"
        };
        tx.execute("INSERT INTO rm_workspace_inbox_exceptions(source_id,code,required_action,opened_at)VALUES(?,?,'review_and_assign',?)",params![id,code,now]).map_err(sql)?;
        event(
            tx,
            &id,
            "exception_opened",
            json!({"code":code,"candidates":candidates,"requiredAction":"review_and_assign"}),
            actor,
            now,
        )?;
    }
    inspect(tx, anchor, &id, visible)
}
fn candidates(i: &Ingest, visible: &BTreeSet<String>) -> Result<Vec<Value>, Error> {
    if i.candidates.len() > 100 {
        return Err(refused("too many candidates"));
    }
    let mut values = BTreeSet::new();
    for c in &i.candidates {
        let slug = text(&c.company_slug, 100)?;
        if !visible.contains(&slug) {
            continue;
        }
        if ![
            "recipient_alias",
            "buyer_vat",
            "buyer_name",
            "account",
            "portal",
        ]
        .contains(&c.evidence_kind.as_str())
        {
            return Err(refused("invalid candidate evidence kind"));
        }
        values.insert((slug, c.evidence_kind.clone(), text(&c.evidence, 1000)?));
    }
    Ok(values
        .into_iter()
        .map(|(s, k, e)| json!({"companySlug":s,"evidenceKind":k,"evidence":e}))
        .collect())
}
pub fn original(tx: &Transaction<'_>, id: &str) -> Result<Vec<u8>, Error> {
    let (bytes, hash): (Vec<u8>, String) = tx
        .query_row(
            "SELECT original_bytes,sha256 FROM rm_workspace_inbox_sources WHERE source_id=?",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(sql)?;
    if bytes.is_empty()
        || bytes.len() > 25 * 1024 * 1024
        || format!("{:x}", Sha256::digest(&bytes)) != hash
    {
        return Err(refused("original source hash/size mismatch"));
    }
    Ok(bytes)
}
pub fn inspect(
    tx: &Transaction<'_>,
    anchor: &str,
    id: &str,
    visible: &BTreeSet<String>,
) -> Result<Value, Error> {
    let id = text(id, 100)?;
    if !visible.contains(anchor) {
        return Err(missing());
    }
    let mut v=tx.query_row("SELECT source_id,sha256,filename,mime_type,transport,transport_identity,received_at,metadata_json,created_by,created_at FROM rm_workspace_inbox_sources WHERE source_id=? AND visibility_anchor_slug=?",[&id,anchor],|r|Ok(json!({"sourceId":r.get::<_,String>(0)?,"visibilityAnchorSlug":anchor,"sha256":r.get::<_,String>(1)?,"filename":r.get::<_,String>(2)?,"mimeType":r.get::<_,String>(3)?,"transport":r.get::<_,String>(4)?,"transportIdentity":r.get::<_,Option<String>>(5)?,"receivedAt":r.get::<_,String>(6)?,"metadata":serde_json::from_str::<Value>(&r.get::<_,String>(7)?).map_err(|_|rusqlite::Error::InvalidQuery)?,"createdBy":r.get::<_,String>(8)?,"createdAt":r.get::<_,String>(9)?}))).optional().map_err(sql)?.ok_or_else(missing)?;
    let mut q=tx.prepare("SELECT company_slug,state,document_id,document_no,assigned_by,assigned_at,completed_at FROM rm_workspace_inbox_assignments WHERE source_id=? ORDER BY company_slug").map_err(sql)?;
    let assignments=q.query_map([&id],|r|Ok(json!({"companySlug":r.get::<_,String>(0)?,"state":r.get::<_,String>(1)?,"documentId":r.get::<_,Option<i64>>(2)?,"documentNo":r.get::<_,Option<String>>(3)?,"assignedBy":r.get::<_,String>(4)?,"assignedAt":r.get::<_,String>(5)?,"completedAt":r.get::<_,Option<String>>(6)?}))).map_err(sql)?.collect::<Result<Vec<_>,_>>().map_err(sql)?;
    v["assignments"] = json!(
        assignments
            .into_iter()
            .filter(|a| a["companySlug"]
                .as_str()
                .is_some_and(|s| visible.contains(s)))
            .collect::<Vec<_>>()
    );
    v["exception"]=tx.query_row("SELECT code,required_action,opened_at,resolved_at FROM rm_workspace_inbox_exceptions WHERE source_id=?",[&id],|r|Ok(json!({"code":r.get::<_,String>(0)?,"requiredAction":r.get::<_,String>(1)?,"openedAt":r.get::<_,String>(2)?,"resolvedAt":r.get::<_,Option<String>>(3)?}))).optional().map_err(sql)?.unwrap_or(Value::Null);
    let mut q=tx.prepare("SELECT event_type,payload_hash,actor,created_at,canonical_payload FROM rm_workspace_inbox_events WHERE source_id=? ORDER BY id").map_err(sql)?;
    let mut history = vec![];
    let events = q
        .query_map([id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
            ))
        })
        .map_err(sql)?;
    for row in events {
        let (kind, hash, actor, at, body) = row.map_err(sql)?;
        if format!("{:x}", Sha256::digest(body.as_bytes())) != hash {
            return Err(refused("inbox event proof mismatch"));
        }
        let mut payload: Value =
            serde_json::from_str(&body).map_err(|_| refused("inbox event JSON unavailable"))?;
        if canonical(&payload)? != body {
            return Err(refused("inbox event canonical proof differs"));
        }
        if ["companySlug", "requestedCompanySlug", "existingCompanySlug"]
            .iter()
            .any(|k| payload[k].as_str().is_some_and(|s| !visible.contains(s)))
        {
            continue;
        }
        if let Some(c) = payload.get_mut("candidates").and_then(Value::as_array_mut) {
            c.retain(|c| {
                c["companySlug"]
                    .as_str()
                    .is_some_and(|s| visible.contains(s))
            })
        }
        history.push(json!({"event_type":kind,"payload_hash":hash,"actor":actor,"created_at":at,"payload":payload}));
    }
    let mut q=tx.prepare("SELECT company_slug,state,lease_expires_at,document_id,document_no FROM rm_workspace_inbox_handoff_claims WHERE source_id=? ORDER BY company_slug").map_err(sql)?;
    let handoffs=q.query_map([&v["sourceId"].as_str().ok_or_else(missing)?],|r|Ok(json!({"companySlug":r.get::<_,String>(0)?,"state":r.get::<_,String>(1)?,"leaseExpiresAt":r.get::<_,String>(2)?,"documentId":r.get::<_,Option<i64>>(3)?,"documentNo":r.get::<_,Option<String>>(4)?}))).map_err(sql)?.collect::<Result<Vec<_>,_>>().map_err(sql)?;
    v["handoffs"] = json!(
        handoffs
            .into_iter()
            .filter(|h| h["companySlug"]
                .as_str()
                .is_some_and(|s| visible.contains(s)))
            .collect::<Vec<_>>()
    );
    v["history"] = json!(history);
    Ok(v)
}
pub fn list(
    tx: &Transaction<'_>,
    anchor: &str,
    visible: &BTreeSet<String>,
    cursor: usize,
    limit: usize,
) -> Result<Value, Error> {
    if limit == 0 || limit > 100 {
        return Err(refused("limit must be 1..100"));
    }
    if !visible.contains(anchor) {
        return Ok(json!({"rows":[],"count":0,"nextCursor":null}));
    }
    let count: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM rm_workspace_inbox_sources WHERE visibility_anchor_slug=?",
            [anchor],
            |r| r.get(0),
        )
        .map_err(sql)?;
    let offset = i64::try_from(cursor).map_err(|_| refused("cursor exceeds storage range"))?;
    let mut q=tx.prepare("SELECT source_id FROM rm_workspace_inbox_sources WHERE visibility_anchor_slug=? ORDER BY source_id LIMIT ? OFFSET ?").map_err(sql)?;
    let ids = q
        .query_map(params![anchor, limit as i64, offset], |r| {
            r.get::<_, String>(0)
        })
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    let rows = ids
        .iter()
        .map(|id| inspect(tx, anchor, id, visible))
        .collect::<Result<Vec<_>, _>>()?;
    let end = cursor.saturating_add(limit);
    Ok(
        json!({"rows":rows,"count":count,"nextCursor":if i64::try_from(end).is_ok_and(|end|end<count){Some(end)}else{None}}),
    )
}
pub fn assign(
    tx: &Transaction<'_>,
    anchor: &str,
    id: &str,
    target: &str,
    visible: &BTreeSet<String>,
    actor: &str,
    now: &str,
) -> Result<Value, Error> {
    let source = inspect(tx, anchor, id, visible)?;
    if !visible.contains(target) {
        return Err(missing());
    }
    let existing:Option<String>=tx.query_row("SELECT company_slug FROM rm_workspace_inbox_assignments WHERE source_id=? AND state IN ('approved','completed')",[id],|r|r.get(0)).optional().map_err(sql)?;
    if let Some(existing) = existing {
        if existing == target {
            return Ok(source);
        }
        return Err(refused(
            "source already has a different approved legal destination",
        ));
    }
    tx.execute("INSERT INTO rm_workspace_inbox_assignments(source_id,company_slug,state,assigned_by,assigned_at)VALUES(?,?,'approved',?,?)",params![id,target,actor,now]).map_err(sql)?;
    tx.execute("UPDATE rm_workspace_inbox_exceptions SET resolved_at=? WHERE source_id=? AND resolved_at IS NULL",params![now,id]).map_err(sql)?;
    event(
        tx,
        id,
        "assigned",
        json!({"companySlug":target}),
        actor,
        now,
    )?;
    inspect(tx, anchor, id, visible)
}
/// Durable claim remains after a company/control split failure. Caller must hold
/// the workspace owner mutex across claim, company commit, and finalize.
pub fn claim(
    tx: &Transaction<'_>,
    id: &str,
    target: &str,
    claim: &str,
    now: &str,
    expires: &str,
) -> Result<(), Error> {
    let approved:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM rm_workspace_inbox_assignments WHERE source_id=? AND company_slug=? AND state='approved')",[id,target],|r|r.get(0)).map_err(sql)?;
    if !approved {
        return Err(refused("source assignment is not approved"));
    }
    let bytes = original(tx, id)?;
    let hash = format!("{:x}", Sha256::digest(&bytes));
    let prior:Option<(String,String,String)>=tx.query_row("SELECT source_hash,state,lease_expires_at FROM rm_workspace_inbox_handoff_claims WHERE source_id=? AND company_slug=?",[id,target],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(sql)?;
    if let Some((h, state, expiry)) = prior {
        if h != hash || state != "claimed" {
            return Err(refused("handoff claim source/state differs"));
        }
        if expiry.as_str() > now {
            return Err(refused(
                "handoff in progress; inspect status before retrying",
            ));
        }
        tx.execute("UPDATE rm_workspace_inbox_handoff_claims SET claim_id=?,lease_expires_at=?,updated_at=? WHERE source_id=? AND company_slug=? AND state='claimed'",params![claim,expires,now,id,target]).map_err(sql)?;
    } else {
        tx.execute("INSERT INTO rm_workspace_inbox_handoff_claims(source_id,company_slug,source_hash,state,claim_id,lease_expires_at,created_at,updated_at)VALUES(?,?,?,'claimed',?,?,?,?)",params![id,target,hash,claim,expires,now,now]).map_err(sql)?;
    }
    Ok(())
}
pub fn complete(
    tx: &Transaction<'_>,
    id: &str,
    target: &str,
    claim: &str,
    document: i64,
    number: &str,
    provenance: Provenance<'_>,
) -> Result<(), Error> {
    let Provenance { actor, now } = provenance;
    let bytes = original(tx, id)?;
    let hash = format!("{:x}", Sha256::digest(&bytes));
    let n=tx.execute("UPDATE rm_workspace_inbox_handoff_claims SET state='completed',document_id=?,document_no=?,updated_at=? WHERE source_id=? AND company_slug=? AND state='claimed' AND claim_id=? AND source_hash=?",params![document,number,now,id,target,claim,hash]).map_err(sql)?;
    if n != 1 {
        return Err(refused("handoff claim became stale"));
    }
    let n=tx.execute("UPDATE rm_workspace_inbox_assignments SET state='completed',document_id=?,document_no=?,completed_at=? WHERE source_id=? AND company_slug=? AND state='approved'",params![document,number,now,id,target]).map_err(sql)?;
    if n != 1 {
        return Err(refused("approved assignment became stale"));
    }
    event(
        tx,
        id,
        "handoff_completed",
        json!({"companySlug":target,"documentId":document,"documentNo":number,"sourceHash":hash}),
        actor,
        now,
    )
}
/// Read-only recovery admits the complete company schema and requires one exact
/// source marker/hash/metadata identity. Caller separately re-reads protected bytes.
pub fn recovered_document(
    tx: &Transaction<'_>,
    marker: &str,
    hash: &str,
    metadata: &rentemester_engine::document::Metadata,
) -> Result<Option<Value>, Error> {
    crate::validate(tx)?;
    let mut q=tx.prepare("SELECT id,document_no,sha256_hash,payload_json FROM documents WHERE source=? ORDER BY id").map_err(sql)?;
    let rows = q
        .query_map([marker], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    if rows.len() > 1 {
        return Err(refused("multiple company documents claim source marker"));
    }
    let Some((id, no, stored_hash, payload)) = rows.into_iter().next() else {
        return Ok(None);
    };
    if !(1..=9_007_199_254_740_991).contains(&id) || no.is_empty() || stored_hash != hash {
        return Err(refused("recovered marker document identity differs"));
    }
    if serde_json::from_str::<Value>(&payload)
        .map_err(|_| refused("recovered metadata unavailable"))?
        != serde_json::to_value(metadata).map_err(|_| refused("source metadata unavailable"))?
    {
        return Err(refused("recovered metadata differs from approved source"));
    }
    Ok(Some(json!({"id":id,"documentNo":no})))
}
