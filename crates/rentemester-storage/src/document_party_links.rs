//! Company-local immutable party snapshots. Registry authority stays on the held control writer.
use rentemester_engine::{
    Error,
    parties::{self as policy, Identifier},
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanInput {
    pub document_id: i64,
    pub role: String,
    pub party_id: Option<String>,
    pub jurisdiction: Option<String>,
    pub identifier_kind: Option<String>,
    pub identifier: Option<String>,
    pub legacy_kind: Option<String>,
    pub legacy_id: Option<String>,
    pub reviewed_legacy_reference: Option<String>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Apply {
    pub document_id: i64,
    pub role: String,
    pub party_id: Option<String>,
    pub jurisdiction: Option<String>,
    pub identifier_kind: Option<String>,
    pub identifier: Option<String>,
    pub legacy_kind: Option<String>,
    pub legacy_id: Option<String>,
    pub reviewed_legacy_reference: Option<String>,
    pub plan_hash: String,
    pub confirm: bool,
    pub idempotency_key: Option<String>,
}
impl Apply {
    pub fn input(&self) -> PlanInput {
        PlanInput {
            document_id: self.document_id,
            role: self.role.clone(),
            party_id: self.party_id.clone(),
            jurisdiction: self.jurisdiction.clone(),
            identifier_kind: self.identifier_kind.clone(),
            identifier: self.identifier.clone(),
            legacy_kind: self.legacy_kind.clone(),
            legacy_id: self.legacy_id.clone(),
            reviewed_legacy_reference: self.reviewed_legacy_reference.clone(),
        }
    }
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Supersede {
    pub document_id: i64,
    pub role: String,
    pub plan_hash: String,
    pub reason: String,
    pub confirm: bool,
}
pub fn refused(code: &'static str) -> Error {
    Error::new(
        code,
        "Document party evidence or current reviewed state refused",
    )
}
fn sql(_: impl std::fmt::Display) -> Error {
    refused("DOCUMENT_PARTY_STORAGE_REFUSED")
}
fn hash(v: &Value) -> Result<String, Error> {
    crate::idempotency::canonical_payload_hash(v).map_err(sql)
}
fn hex(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn valid_role(role: &str) -> Result<(), Error> {
    if policy::ROLES.contains(&role) {
        Ok(())
    } else {
        Err(refused("UNSUPPORTED_PARTY_ROLE"))
    }
}
fn identity(input: &PlanInput) -> Result<policy::ResolvedIdentifier, Error> {
    valid_role(&input.role)?;
    if input.document_id <= 0 || input.document_id > 9_007_199_254_740_991 {
        return Err(refused("DOCUMENT_NOT_FOUND"));
    }
    if input.legacy_kind.is_some()
        || input.legacy_id.is_some()
        || input.reviewed_legacy_reference.is_some()
    {
        return Err(refused("UNSUPPORTED_EVIDENCE_KIND"));
    }
    policy::identity(&Identifier {
        country: input
            .jurisdiction
            .clone()
            .ok_or_else(|| refused("NO_IDENTIFIER_EVIDENCE"))?,
        identifier_kind: input
            .identifier_kind
            .clone()
            .ok_or_else(|| refused("NO_IDENTIFIER_EVIDENCE"))?,
        identifier: input.identifier.clone(),
    })
    .map_err(|_| refused("NO_IDENTIFIER_EVIDENCE"))
}
fn evidence(input: &PlanInput) -> Result<Value, Error> {
    valid_role(&input.role)?;
    if input.document_id <= 0 || input.document_id > 9_007_199_254_740_991 {
        return Err(refused("DOCUMENT_NOT_FOUND"));
    }
    if input.legacy_kind.is_some()
        || input.legacy_id.is_some()
        || input.reviewed_legacy_reference.is_some()
    {
        if input.identifier.is_some()
            || input.identifier_kind.is_some()
            || input.jurisdiction.is_some()
        {
            return Err(refused("UNSUPPORTED_EVIDENCE_KIND"));
        }
        let kind = input
            .legacy_kind
            .as_deref()
            .filter(|k| ["customer", "vendor"].contains(k))
            .ok_or_else(|| refused("NO_IDENTIFIER_EVIDENCE"))?;
        let id = input
            .legacy_id
            .as_deref()
            .filter(|v| !v.trim().is_empty() && v.len() <= 160)
            .ok_or_else(|| refused("NO_IDENTIFIER_EVIDENCE"))?;
        let reference = input
            .reviewed_legacy_reference
            .as_deref()
            .filter(|v| !v.trim().is_empty() && v.len() <= 500)
            .ok_or_else(|| refused("LEGACY_REFERENCE_UNREVIEWED"))?;
        return Ok(
            json!({"kind":"reviewed_legacy_reference","legacyKind":kind,"legacyId":id,"reviewedLegacyReference":reference}),
        );
    }
    let id = identity(input)?;
    Ok(
        json!({"kind":"exact_identifier","jurisdiction":id.country,"identifierKind":id.identifier_kind,"identifier":id.identifier}),
    )
}
/// Called only after current selected-company read/master-data authorization. No registry writes.
pub fn capture(control: &Transaction<'_>, input: &PlanInput, slug: &str) -> Result<Value, Error> {
    let evidence = evidence(input)?;
    let scope = BTreeSet::from([slug.to_owned()]);
    let candidates = if evidence["kind"] == "exact_identifier" {
        let mut q = control.prepare("SELECT party_id FROM rm_party_identifiers WHERE jurisdiction=?1 AND identifier_kind=?2 AND identifier=?3 ORDER BY party_id").map_err(sql)?;
        q.query_map(
            params![
                evidence["jurisdiction"].as_str(),
                evidence["identifierKind"].as_str(),
                evidence["identifier"].as_str()
            ],
            |r| r.get::<_, String>(0),
        )
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?
    } else {
        let mut q = control.prepare("SELECT party_id FROM rm_party_legacy_links WHERE company_slug=?1 AND legacy_kind=?2 AND legacy_id=?3 ORDER BY party_id").map_err(sql)?;
        q.query_map(
            params![
                slug,
                evidence["legacyKind"].as_str(),
                evidence["legacyId"].as_str()
            ],
            |r| r.get::<_, String>(0),
        )
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?
    };
    let mut visible = vec![];
    for p in candidates {
        if crate::parties::visible(control, &p, &scope)? {
            visible.push(p)
        }
    }
    let party_id = match visible.as_slice() {
        [p] => p,
        [] => return Err(refused("NO_IDENTIFIER_EVIDENCE")),
        _ => return Err(refused("MULTIPLE_CANDIDATES")),
    };
    if input
        .party_id
        .as_ref()
        .is_some_and(|p| p.trim() != party_id)
    {
        return Err(refused("IDENTIFIER_CONFLICT"));
    }
    let party = crate::parties::inspect(control, party_id, &scope)?;
    let roles: Vec<_> = party["roles"]
        .as_array()
        .ok_or_else(|| refused("PARTY_NOT_FOUND"))?
        .iter()
        .filter(|r| r["companySlug"] == slug && r["role"] == input.role)
        .cloned()
        .collect();
    if roles.is_empty() {
        return Err(refused("SCOPE_MISMATCH"));
    }
    Ok(
        json!({"partyId":party["partyId"],"kind":party["kind"],"name":party["name"],"identifiers":party["identifiers"],"roles":roles,"history":party["history"]}),
    )
}
fn document(tx: &Connection, id: i64) -> Result<Value, Error> {
    type DocumentColumns = (
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    );
    let row:Option<DocumentColumns>=tx.query_row("SELECT sha256_hash,payload_json,supplier_country_code,supplier_identifier_kind,sender_vat_cvr FROM documents WHERE id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional().map_err(sql)?;
    let (sha, payload, country, kind, identifier) =
        row.ok_or_else(|| refused("DOCUMENT_NOT_FOUND"))?;
    if !hex(&sha) {
        return Err(refused("DOCUMENT_SOURCE_REFUSED"));
    }
    let payload: Value = serde_json::from_str(
        payload
            .as_deref()
            .ok_or_else(|| refused("DOCUMENT_SOURCE_REFUSED"))?,
    )
    .map_err(sql)?;
    Ok(
        json!({"id":id,"sha256":sha,"payloadHash":hash(&payload)?,"country":country,"identifierKind":kind,"identifier":identifier}),
    )
}
pub fn plan(tx: &Transaction<'_>, input: &PlanInput, snapshot: &Value) -> Result<Value, Error> {
    let evidence = evidence(input)?;
    let doc = document(tx, input.document_id)?;
    if evidence["kind"] == "exact_identifier"
        && (doc["country"] != evidence["jurisdiction"]
            || doc["identifierKind"] != evidence["identifierKind"]
            || doc["identifier"] != evidence["identifier"])
    {
        return Err(refused("NO_IDENTIFIER_EVIDENCE"));
    }
    let party = snapshot["partyId"]
        .as_str()
        .ok_or_else(|| refused("PARTY_NOT_FOUND"))?;
    if snapshot["roles"]
        .as_array()
        .is_none_or(|r| r.is_empty() || r.iter().any(|r| r["role"] != input.role))
    {
        return Err(refused("SCOPE_MISMATCH"));
    }
    let mut p = json!({"documentId":input.document_id,"documentSha256":doc["sha256"],"documentPayloadSha256":doc["payloadHash"],"partyId":party,"partySnapshot":snapshot,"role":input.role,"evidence":evidence});
    p["planHash"] = json!(hash(&p)?);
    Ok(json!({"ok":true,"plan":p}))
}
fn checked_history(tx: &Connection, document_id: i64) -> Result<Vec<Value>, Error> {
    let mut q=tx.prepare("SELECT id,party_id,party_role,event_type,evidence_kind,evidence_json,document_sha256,document_payload_sha256,party_snapshot_json,plan_hash,reason,actor,principal,created_at,idempotency_key FROM document_party_link_events WHERE document_id=?1 ORDER BY id").map_err(sql)?;
    let mut rows = q.query([document_id]).map_err(sql)?;
    let mut out = vec![];
    let mut live = std::collections::BTreeMap::new();
    while let Some(r) = rows.next().map_err(sql)? {
        let id: i64 = r.get(0).map_err(sql)?;
        let party: String = r.get(1).map_err(sql)?;
        let role: String = r.get(2).map_err(sql)?;
        let event: String = r.get(3).map_err(sql)?;
        let evidence_kind: String = r.get(4).map_err(sql)?;
        let evidence_raw: String = r.get(5).map_err(sql)?;
        let sha: String = r.get(6).map_err(sql)?;
        let payload_sha: String = r.get(7).map_err(sql)?;
        let snapshot_raw: String = r.get(8).map_err(sql)?;
        let plan_hash: String = r.get(9).map_err(sql)?;
        let reason: Option<String> = r.get(10).map_err(sql)?;
        let actor: String = r.get(11).map_err(sql)?;
        let principal: String = r.get(12).map_err(sql)?;
        let created: String = r.get(13).map_err(sql)?;
        let key: Option<String> = r.get(14).map_err(sql)?;
        let evidence: Value = serde_json::from_str(&evidence_raw).map_err(sql)?;
        let snapshot: Value = serde_json::from_str(&snapshot_raw).map_err(sql)?;
        let payload = json!({"documentId":document_id,"documentSha256":sha,"documentPayloadSha256":payload_sha,"partyId":party,"partySnapshot":snapshot,"role":role,"evidence":evidence});
        let source = document(tx, document_id)?;
        valid_role(&role)?;
        validate_actor(&actor, &principal, &created)?;
        if source["sha256"] != sha || source["payloadHash"] != payload_sha {
            return Err(refused("DOCUMENT_PARTY_HISTORY_SOURCE_MISMATCH"));
        }
        if id <= 0
            || !hex(&sha)
            || !hex(&payload_sha)
            || hash(&payload)? != plan_hash
            || evidence["kind"] != evidence_kind
            || snapshot["partyId"] != party
            || actor.trim().is_empty()
            || principal.trim().is_empty()
        {
            return Err(refused("DOCUMENT_PARTY_HISTORY_REFUSED"));
        }
        match event.as_str() {
            "linked" => {
                if live.insert(role.clone(), plan_hash.clone()).is_some() {
                    return Err(refused("CURRENT_STATE_CONFLICT"));
                }
            }
            "superseded" => {
                if live.remove(&role).as_ref() != Some(&plan_hash)
                    || reason.as_ref().is_none_or(|r| r.trim().is_empty())
                {
                    return Err(refused("DOCUMENT_PARTY_HISTORY_REFUSED"));
                }
            }
            _ => return Err(refused("DOCUMENT_PARTY_HISTORY_REFUSED")),
        }
        out.push(json!({"id":id,"party_id":party,"party_role":role,"event_type":event,"evidence_kind":evidence_kind,"evidence_json":evidence_raw,"document_sha256":sha,"document_payload_sha256":payload_sha,"party_snapshot_json":snapshot_raw,"plan_hash":plan_hash,"reason":reason,"actor":actor,"principal":principal,"created_at":created,"idempotency_key":key}));
    }
    Ok(out)
}
/// Historical validation on a signed snapshot; no current registry grants are adopted.
pub fn validate_backup(db: &Connection) -> Result<(), Error> {
    let ids = db
        .prepare("SELECT DISTINCT document_id FROM document_party_link_events ORDER BY document_id")
        .map_err(sql)?
        .query_map([], |r| r.get::<_, i64>(0))
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    for id in ids {
        let source = document(db, id)?;
        for event in checked_history(db, id)? {
            if event["document_sha256"] != source["sha256"]
                || event["document_payload_sha256"] != source["payloadHash"]
            {
                return Err(refused("DOCUMENT_PARTY_HISTORY_SOURCE_MISMATCH"));
            }
            let snapshot: Value = serde_json::from_str(
                event["party_snapshot_json"]
                    .as_str()
                    .ok_or_else(|| refused("DOCUMENT_PARTY_HISTORY_REFUSED"))?,
            )
            .map_err(sql)?;
            let role = event["party_role"]
                .as_str()
                .ok_or_else(|| refused("DOCUMENT_PARTY_HISTORY_REFUSED"))?;
            if !snapshot["roles"]
                .as_array()
                .is_some_and(|roles| !roles.is_empty() && roles.iter().all(|r| r["role"] == role))
            {
                return Err(refused("DOCUMENT_PARTY_HISTORY_ROLE_MISMATCH"));
            }
            crate::cookie_session::timestamp(
                event["created_at"]
                    .as_str()
                    .ok_or_else(|| refused("DOCUMENT_PARTY_HISTORY_REFUSED"))?,
            )?;
        }
    }
    validate_internal_history(db)?;
    Ok(())
}
pub fn history(tx: &Transaction<'_>, document_id: i64) -> Result<Value, Error> {
    document(tx, document_id)?;
    validate_backup(tx)?;
    Ok(json!({"ok":true,"links":checked_history(tx, document_id)?}))
}
pub fn list(tx: &Transaction<'_>, status: Option<&str>, limit: usize) -> Result<Value, Error> {
    validate_backup(tx)?;
    let status = match status {
        Some("linked") => Some("resolved"),
        Some("unlinked") => Some("unresolved"),
        s => s,
    };
    if !(1..=100).contains(&limit)
        || status
            .is_some_and(|s| !["resolved", "unresolved", "internal_no_external_party"].contains(&s))
    {
        return Err(refused("INVALID_DOCUMENT_LINK_FILTER"));
    }
    let mut q=tx.prepare("SELECT d.id,d.document_no,d.sha256_hash,EXISTS(SELECT 1 FROM current_document_party_links l WHERE l.document_id=d.id),CASE WHEN EXISTS(SELECT 1 FROM current_document_party_resolution_events p WHERE p.document_id=d.id) THEN 'internal_no_external_party' WHEN EXISTS(SELECT 1 FROM current_document_party_links l WHERE l.document_id=d.id) THEN 'resolved' ELSE 'unresolved' END AS resolution_state FROM documents d WHERE ?1 IS NULL OR resolution_state=?1 ORDER BY d.id DESC LIMIT ?2").map_err(sql)?;
    let rows=q.query_map(params![status,limit as i64],|r|Ok(json!({"id":r.get::<_,i64>(0)?,"document_no":r.get::<_,Option<String>>(1)?,"sha256_hash":r.get::<_,String>(2)?,"linked":r.get::<_,i64>(3)?,"resolution_state":r.get::<_,String>(4)?}))).map_err(sql)?.collect::<Result<Vec<_>,_>>().map_err(sql)?;
    for r in &rows {
        checked_history(tx, r["id"].as_i64().unwrap())?;
    }
    Ok(json!({"ok":true,"links":rows}))
}
pub fn apply(
    tx: &Transaction<'_>,
    input: &Apply,
    snapshot: &Value,
    actor: &str,
    principal: &str,
    now: &str,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    if !input.confirm {
        return Err(refused("CONFIRMATION_REQUIRED"));
    }
    validate_actor(actor, principal, now)?;
    let planned = plan(tx, &input.input(), snapshot)?;
    let p = &planned["plan"];
    if p["planHash"] != input.plan_hash {
        return Err(refused("PLAN_HASH_MISMATCH"));
    }
    if input
        .idempotency_key
        .as_ref()
        .is_some_and(|s| s.trim().is_empty() || s.len() > 128)
    {
        return Err(refused("IDEMPOTENCY_CONFLICT"));
    }
    validate_backup(tx)?;
    let history = checked_history(tx, input.document_id)?;
    let keyed:Option<(i64,i64,String,String)>=input.idempotency_key.as_ref().map(|key|tx.query_row("SELECT id,document_id,party_role,plan_hash FROM document_party_link_events WHERE idempotency_key=?1",[key],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(sql)).transpose()?.flatten();
    if keyed.as_ref().is_some_and(|(_, doc, role, hash)| {
        *doc != input.document_id || role != &input.role || hash != &input.plan_hash
    }) {
        return Err(refused("IDEMPOTENCY_CONFLICT"));
    }
    if tx.query_row("SELECT EXISTS(SELECT 1 FROM current_document_party_resolution_events WHERE document_id=?1)",[input.document_id],|r|r.get::<_,bool>(0)).map_err(sql)? {return Err(refused("CURRENT_STATE_CONFLICT"))}
    let prior = history.iter().rev().find(|r| r["party_role"] == input.role);
    if let Some(prior) = prior {
        if prior["event_type"] == "linked" && prior["plan_hash"] == input.plan_hash {
            return Ok(
                json!({"ok":true,"id":prior["id"],"idempotent":true,"planHash":input.plan_hash}),
            );
        }
        if prior["event_type"] == "linked" || prior["plan_hash"] == input.plan_hash {
            return Err(refused("CURRENT_STATE_CONFLICT"));
        }
    }
    if keyed.is_some() {
        return Err(refused("CURRENT_STATE_CONFLICT"));
    }
    validate_actor(actor, principal, now)?;
    let id:i64=tx.query_row("INSERT INTO document_party_link_events(document_id,party_id,party_role,event_type,evidence_kind,evidence_json,document_sha256,document_payload_sha256,party_snapshot_json,plan_hash,idempotency_key,actor,principal,created_at)VALUES(?1,?2,?3,'linked',?13,?4,?5,?6,?7,?8,?9,?10,?11,?12) RETURNING id",params![input.document_id,p["partyId"].as_str(),input.role,serde_json::to_string(&p["evidence"]).map_err(sql)?,p["documentSha256"].as_str(),p["documentPayloadSha256"].as_str(),serde_json::to_string(&p["partySnapshot"]).map_err(sql)?,input.plan_hash,input.idempotency_key,actor,principal,now,p["evidence"]["kind"].as_str()],|r|r.get(0)).map_err(sql)?;
    Ok(json!({"ok":true,"id":id,"idempotent":false,"planHash":input.plan_hash}))
}
fn validate_actor(actor: &str, principal: &str, now: &str) -> Result<(), Error> {
    // Historical source attribution may be agent/system and a distinct principal.
    // Current authority and equality assertions belong to application admission.
    if actor.trim().is_empty()
        || actor.len() > 160
        || principal.trim().is_empty()
        || principal.len() > 160
    {
        return Err(refused("ACTOR_REQUIRED"));
    }
    crate::cookie_session::timestamp(now)?;
    Ok(())
}
pub fn supersede(
    tx: &Transaction<'_>,
    input: &Supersede,
    actor: &str,
    principal: &str,
    now: &str,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    valid_role(&input.role)?;
    validate_actor(actor, principal, now)?;
    if !input.confirm {
        return Err(refused("CONFIRMATION_REQUIRED"));
    }
    let reason = input.reason.trim();
    if reason.is_empty() || reason.len() > 1000 {
        return Err(refused("SUPERSEDE_REASON_REQUIRED"));
    }
    document(tx, input.document_id)?;
    validate_backup(tx)?;
    let history = checked_history(tx, input.document_id)?;
    let prior = history
        .iter()
        .rev()
        .find(|r| r["party_role"] == input.role && r["plan_hash"] == input.plan_hash)
        .ok_or_else(|| refused("LINK_NOT_FOUND"))?;
    if prior["event_type"] == "superseded" {
        if prior["reason"] != reason {
            return Err(refused("IDEMPOTENCY_CONFLICT"));
        }
        return Ok(
            json!({"ok":true,"id":prior["id"],"idempotent":true,"planHash":input.plan_hash}),
        );
    }
    let id:i64=tx.query_row("INSERT INTO document_party_link_events(document_id,party_id,party_role,event_type,evidence_kind,evidence_json,document_sha256,document_payload_sha256,party_snapshot_json,plan_hash,reason,actor,principal,created_at)VALUES(?1,?2,?3,'superseded',?4,?5,?6,?7,?8,?9,?10,?11,?12,?13) RETURNING id",params![input.document_id,prior["party_id"].as_str(),input.role,prior["evidence_kind"].as_str(),prior["evidence_json"].as_str(),prior["document_sha256"].as_str(),prior["document_payload_sha256"].as_str(),prior["party_snapshot_json"].as_str(),input.plan_hash,reason,actor,principal,now],|r|r.get(0)).map_err(sql)?;
    Ok(json!({"ok":true,"id":id,"idempotent":false,"planHash":input.plan_hash}))
}
#[cfg(test)]
mod tests;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Internal {
    pub document_id: i64,
    pub reason: String,
    pub confirm: bool,
    pub idempotency_key: Option<String>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InternalSupersede {
    pub document_id: i64,
    pub decision_hash: String,
    pub reason: String,
    pub confirm: bool,
}
fn reason(value: &str) -> Result<&str, Error> {
    let value = value.trim();
    if value.is_empty() || value.len() > 1000 {
        return Err(refused("SUPERSEDE_REASON_REQUIRED"));
    }
    Ok(value)
}
fn internal_source(db: &Connection, id: i64) -> Result<Value, Error> {
    let source = document(db, id)?;
    let kind: String = db
        .query_row(
            "SELECT document_type FROM documents WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .map_err(sql)?;
    if kind != "internal_voucher" {
        return Err(refused("DOCUMENT_NOT_FOUND"));
    }
    Ok(source)
}
fn decision(id: i64, source: &Value, reason: &str) -> Result<String, Error> {
    hash(
        &json!({"documentId":id,"documentSha256":source["sha256"],"documentPayloadSha256":source["payloadHash"],"state":"internal_no_external_party","reason":reason}),
    )
}
fn validate_internal_history(db: &Connection) -> Result<(), Error> {
    let mut q = db.prepare("SELECT document_id,state,reason,document_sha256,document_payload_sha256,decision_hash,supersedes_hash,actor,principal,created_at FROM document_party_resolution_events ORDER BY id").map_err(sql)?;
    let mut rows = q.query([]).map_err(sql)?;
    let mut live = std::collections::BTreeMap::new();
    while let Some(r) = rows.next().map_err(sql)? {
        let id: i64 = r.get(0).map_err(sql)?;
        let state: String = r.get(1).map_err(sql)?;
        let why: String = r.get(2).map_err(sql)?;
        let sha: String = r.get(3).map_err(sql)?;
        let payload: String = r.get(4).map_err(sql)?;
        let digest: String = r.get(5).map_err(sql)?;
        let supersedes: Option<String> = r.get(6).map_err(sql)?;
        let actor: String = r.get(7).map_err(sql)?;
        let principal: String = r.get(8).map_err(sql)?;
        let now: String = r.get(9).map_err(sql)?;
        validate_actor(&actor, &principal, &now)?;
        reason(&why)?;
        let source = internal_source(db, id)?;
        if source["sha256"] != sha || source["payloadHash"] != payload || !hex(&digest) {
            return Err(refused("DOCUMENT_PARTY_HISTORY_SOURCE_MISMATCH"));
        }
        match state.as_str() {
            "internal_no_external_party" => {
                if supersedes.is_some()
                    || decision(id, &source, &why)? != digest
                    || live.insert(id, digest).is_some()
                {
                    return Err(refused("DOCUMENT_PARTY_HISTORY_REFUSED"));
                }
            }
            "superseded" => {
                if supersedes.as_ref() != Some(&digest)
                    || live.remove(&id).as_ref() != Some(&digest)
                {
                    return Err(refused("DOCUMENT_PARTY_HISTORY_REFUSED"));
                }
            }
            _ => return Err(refused("DOCUMENT_PARTY_HISTORY_REFUSED")),
        }
    }
    for id in live.keys() {
        let linked: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM current_document_party_links WHERE document_id=?1)",
                [id],
                |r| r.get(0),
            )
            .map_err(sql)?;
        if linked {
            return Err(refused("CURRENT_STATE_CONFLICT"));
        }
    }
    Ok(())
}
pub fn internal(
    tx: &Transaction<'_>,
    input: &Internal,
    actor: &str,
    principal: &str,
    now: &str,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    validate_actor(actor, principal, now)?;
    if !input.confirm {
        return Err(refused("CONFIRMATION_REQUIRED"));
    }
    let why = reason(&input.reason)?;
    let source = internal_source(tx, input.document_id)?;
    validate_backup(tx)?;
    let digest = decision(input.document_id, &source, why)?;
    if input
        .idempotency_key
        .as_ref()
        .is_some_and(|k| k.trim().is_empty() || k.len() > 128)
    {
        return Err(refused("IDEMPOTENCY_CONFLICT"));
    }
    let linked: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM current_document_party_links WHERE document_id=?1)",
            [input.document_id],
            |r| r.get(0),
        )
        .map_err(sql)?;
    if linked {
        return Err(refused("CURRENT_STATE_CONFLICT"));
    }
    let keyed: Option<(i64,String)> = input.idempotency_key.as_ref().map(|k| tx.query_row("SELECT document_id,decision_hash FROM document_party_resolution_events WHERE idempotency_key=?1",[k],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(sql)).transpose()?.flatten();
    if keyed
        .as_ref()
        .is_some_and(|(id, h)| *id != input.document_id || h != &digest)
    {
        return Err(refused("IDEMPOTENCY_CONFLICT"));
    }
    let current: Option<(i64,String)> = tx.query_row("SELECT id,decision_hash FROM current_document_party_resolution_events WHERE document_id=?1",[input.document_id],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(sql)?;
    if let Some((id, h)) = current {
        if h != digest {
            return Err(refused("IDEMPOTENCY_CONFLICT"));
        }
        return Ok(json!({"ok":true,"id":id,"idempotent":true,"decisionHash":digest}));
    }
    let old: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM document_party_resolution_events WHERE document_id=?1 AND decision_hash=?2)",params![input.document_id,digest],|r|r.get(0)).map_err(sql)?;
    if keyed.is_some() || old {
        return Err(refused("CURRENT_STATE_CONFLICT"));
    }
    let id: i64 = tx.query_row("INSERT INTO document_party_resolution_events(document_id,state,reason,document_sha256,document_payload_sha256,decision_hash,idempotency_key,actor,principal,created_at) VALUES(?1,'internal_no_external_party',?2,?3,?4,?5,?6,?7,?8,?9) RETURNING id",params![input.document_id,why,source["sha256"].as_str(),source["payloadHash"].as_str(),digest,input.idempotency_key,actor,principal,now],|r|r.get(0)).map_err(sql)?;
    Ok(json!({"ok":true,"id":id,"idempotent":false,"decisionHash":digest}))
}
pub fn internal_supersede(
    tx: &Transaction<'_>,
    input: &InternalSupersede,
    actor: &str,
    principal: &str,
    now: &str,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    validate_actor(actor, principal, now)?;
    if !input.confirm {
        return Err(refused("CONFIRMATION_REQUIRED"));
    }
    let why = reason(&input.reason)?;
    let source = internal_source(tx, input.document_id)?;
    validate_backup(tx)?;
    let prior: Option<(i64,String,String)> = tx.query_row("SELECT id,state,reason FROM document_party_resolution_events WHERE document_id=?1 AND decision_hash=?2 ORDER BY id DESC LIMIT 1",params![input.document_id,input.decision_hash],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(sql)?;
    let (id, state, old_reason) = prior.ok_or_else(|| refused("LINK_NOT_FOUND"))?;
    if state == "superseded" {
        if old_reason != why {
            return Err(refused("IDEMPOTENCY_CONFLICT"));
        }
        return Ok(json!({"ok":true,"id":id,"idempotent":true}));
    }
    let id: i64 = tx.query_row("INSERT INTO document_party_resolution_events(document_id,state,reason,document_sha256,document_payload_sha256,decision_hash,supersedes_hash,actor,principal,created_at) VALUES(?1,'superseded',?2,?3,?4,?5,?5,?6,?7,?8) RETURNING id",params![input.document_id,why,source["sha256"].as_str(),source["payloadHash"].as_str(),input.decision_hash,actor,principal,now],|r|r.get(0)).map_err(sql)?;
    Ok(json!({"ok":true,"id":id,"idempotent":false}))
}
