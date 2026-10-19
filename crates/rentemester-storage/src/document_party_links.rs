//! Company-local immutable party snapshots. Registry authority stays on the held control writer.
use rentemester_engine::{
    Error,
    parties::{self as policy, Identifier},
};
use rusqlite::{OptionalExtension, Transaction, params};
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
    if ["supplier", "vendor"].contains(&role) {
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
/// Called only after current selected-company read/master-data authorization. No registry writes.
pub fn capture(control: &Transaction<'_>, input: &PlanInput, slug: &str) -> Result<Value, Error> {
    let id = identity(input)?;
    let scope = BTreeSet::from([slug.to_owned()]);
    let mut q=control.prepare("SELECT party_id FROM rm_party_identifiers WHERE jurisdiction=?1 AND identifier_kind=?2 AND identifier=?3 ORDER BY party_id").map_err(sql)?;
    let candidates = q
        .query_map(
            params![id.country, id.identifier_kind, id.identifier],
            |r| r.get::<_, String>(0),
        )
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
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
fn document(tx: &Transaction<'_>, id: i64) -> Result<Value, Error> {
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
    let id = identity(input)?;
    let doc = document(tx, input.document_id)?;
    if doc["country"] != id.country
        || doc["identifierKind"] != id.identifier_kind
        || doc["identifier"] != id.identifier
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
    let mut p = json!({"documentId":input.document_id,"documentSha256":doc["sha256"],"documentPayloadSha256":doc["payloadHash"],"partyId":party,"partySnapshot":snapshot,"role":input.role,"evidence":{"kind":"exact_identifier","jurisdiction":id.country,"identifierKind":id.identifier_kind,"identifier":id.identifier}});
    p["planHash"] = json!(hash(&p)?);
    Ok(json!({"ok":true,"plan":p}))
}
fn checked_history(tx: &Transaction<'_>, document_id: i64) -> Result<Vec<Value>, Error> {
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
pub fn history(tx: &Transaction<'_>, document_id: i64) -> Result<Value, Error> {
    document(tx, document_id)?;
    Ok(json!({"ok":true,"links":checked_history(tx,document_id)?}))
}
pub fn list(tx: &Transaction<'_>, status: Option<&str>, limit: usize) -> Result<Value, Error> {
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
    let planned = plan(tx, &input.input(), snapshot)?;
    let p = &planned["plan"];
    if p["planHash"] != input.plan_hash {
        return Err(refused("PLAN_HASH_MISMATCH"));
    }
    if input
        .idempotency_key
        .as_ref()
        .is_some_and(|s| s.trim().is_empty() || s.len() > 512)
    {
        return Err(refused("IDEMPOTENCY_CONFLICT"));
    }
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
    let id:i64=tx.query_row("INSERT INTO document_party_link_events(document_id,party_id,party_role,event_type,evidence_kind,evidence_json,document_sha256,document_payload_sha256,party_snapshot_json,plan_hash,idempotency_key,actor,principal,created_at)VALUES(?1,?2,?3,'linked','exact_identifier',?4,?5,?6,?7,?8,?9,?10,?11,?12) RETURNING id",params![input.document_id,p["partyId"].as_str(),input.role,serde_json::to_string(&p["evidence"]).map_err(sql)?,p["documentSha256"].as_str(),p["documentPayloadSha256"].as_str(),serde_json::to_string(&p["partySnapshot"]).map_err(sql)?,input.plan_hash,input.idempotency_key,actor,principal,now],|r|r.get(0)).map_err(sql)?;
    Ok(json!({"ok":true,"id":id,"idempotent":false,"planHash":input.plan_hash}))
}
fn validate_actor(actor: &str, principal: &str, now: &str) -> Result<(), Error> {
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
