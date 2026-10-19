//! Immutable ownership snapshots/facts in existing control migrations22/24.
use crate::parties::{canonical, hash};
use rentemester_engine::{
    Error,
    ownership::{self as policy, Ending, Endpoint, Fact},
};
use rusqlite::{OptionalExtension, Transaction, params};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
fn sql(_: impl std::fmt::Display) -> Error {
    policy::refused("ownership storage unavailable or corrupt")
}
fn missing() -> Error {
    Error::new("ownership_not_found", "ownership snapshot not found")
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Propose {
    pub snapshot_id: Option<String>,
    pub source: String,
    pub observed_at: String,
    pub facts: Vec<Fact>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Review {
    pub snapshot_id: String,
    pub decision: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Apply {
    pub snapshot_id: String,
    pub snapshot_hash: String,
    pub diff_hash: String,
}
pub struct Snapshot {
    pub facts: Vec<Fact>,
    pub public: Value,
}
pub fn normalized(facts: &[Fact]) -> Result<Vec<Fact>, Error> {
    if facts.is_empty() || facts.len() > 2048 {
        return Err(policy::refused("1..2048 facts required"));
    }
    facts.iter().cloned().map(Fact::normalize).collect()
}
fn hashed(facts: &[Fact]) -> Result<Vec<(String, Fact)>, Error> {
    let mut rows = facts
        .iter()
        .map(|f| Ok((hash(&serde_json::to_value(f).map_err(sql)?), f.clone())))
        .collect::<Result<Vec<_>, Error>>()?;
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(rows)
}
pub fn approved(tx: &Transaction<'_>) -> Result<Vec<(String, Fact)>, Error> {
    let mut q=tx.prepare("SELECT fact_hash,canonical_fact FROM main.rm_ownership_facts WHERE review_state='approved' ORDER BY fact_hash").map_err(sql)?;
    let rows = q
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    let mut end=tx.prepare("SELECT fact_hash,MIN(effective_to_exclusive) FROM main.rm_ownership_fact_events WHERE event_type='ended' GROUP BY fact_hash").map_err(sql)?;
    let ends: BTreeMap<String, String> = end
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(sql)?
        .collect::<Result<_, _>>()
        .map_err(sql)?;
    rows.into_iter()
        .map(|(h, body)| {
            let fact: Fact = serde_json::from_str(&body).map_err(sql)?;
            let value = serde_json::to_value(&fact).map_err(sql)?;
            if hash(&value) != h || canonical(&value) != body || fact.clone().normalize()? != fact {
                return Err(sql("fact hash"));
            }
            if let Some(d) = ends.get(&h) {
                policy::date(d)?;
                if d <= &fact.valid_from {
                    return Err(sql("invalid ending"));
                }
            }
            Ok((h.clone(), fact.with_end(ends.get(&h).map(String::as_str))))
        })
        .collect()
}
pub fn load(tx: &Transaction<'_>, id: &str) -> Result<Snapshot, Error> {
    type SnapshotRow = (
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
    );
    let row:Option<SnapshotRow>=tx.query_row("SELECT source,observed_at,snapshot_hash,diff_hash,canonical_facts,diff_json,actor,principal_kind,principal_id FROM main.rm_ownership_source_snapshots WHERE snapshot_id=?",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?,r.get(8)?))).optional().map_err(sql)?;
    let (source, observed, sh, dh, facts_text, diff_text, actor, kind, pid) =
        row.ok_or_else(missing)?;
    let facts: Vec<Fact> = serde_json::from_str(&facts_text).map_err(sql)?;
    let diff: Value = serde_json::from_str(&diff_text).map_err(sql)?;
    if canonical(&json!(facts)) != facts_text
        || canonical(&diff) != diff_text
        || hash(&json!({"source":source,"observedAt":observed,"facts":facts})) != sh
        || hash(&diff) != dh
    {
        return Err(sql("snapshot hash"));
    }
    let mut q=tx.prepare("SELECT event_type,actor,principal_kind,principal_id,created_at FROM main.rm_ownership_snapshot_events WHERE snapshot_id=? ORDER BY id").map_err(sql)?;
    let history=q.query_map([id],|r|Ok(json!({"event_type":r.get::<_,String>(0)?,"actor":r.get::<_,String>(1)?,"principal_kind":r.get::<_,String>(2)?,"principal_id":r.get::<_,String>(3)?,"created_at":r.get::<_,String>(4)?}))).map_err(sql)?.collect::<Result<Vec<_>,_>>().map_err(sql)?;
    let state = history
        .last()
        .and_then(|r| r["event_type"].as_str())
        .ok_or_else(|| sql("missing state"))?;
    // Conflict observations are part of the visible immutable receipt, and
    // retain the same current source/endpoint gate as the incoming facts.
    let mut authority_facts = facts;
    let conflicts: Vec<Fact> = serde_json::from_value(diff["conflicts"].clone()).map_err(sql)?;
    authority_facts.extend(conflicts);
    Ok(Snapshot {
        facts: authority_facts,
        public: json!({"snapshotId":id,"source":source,"observedAt":observed,"snapshotHash":sh,"diffHash":dh,"diff":diff,"state":state,"actor":actor,"principal":{"kind":kind,"id":pid},"history":history}),
    })
}
pub fn ids(tx: &Transaction<'_>) -> Result<Vec<String>, Error> {
    let mut q = tx
        .prepare("SELECT snapshot_id FROM main.rm_ownership_source_snapshots ORDER BY id")
        .map_err(sql)?;
    q.query_map([], |r| r.get(0))
        .map_err(sql)?
        .collect::<Result<_, _>>()
        .map_err(sql)
}
fn event(
    tx: &Transaction<'_>,
    id: &str,
    event: &str,
    actor: &str,
    pid: &str,
    stamp: &str,
) -> Result<(), Error> {
    tx.execute("INSERT INTO main.rm_ownership_snapshot_events(snapshot_id,event_type,actor,principal_kind,principal_id,created_at) VALUES(?,?,?,'user',?,?)",params![id,event,actor,pid,stamp]).map_err(sql)?;
    Ok(())
}
pub fn propose(
    tx: &Transaction<'_>,
    input: &Propose,
    facts: &[Fact],
    generated: &str,
    actor: &str,
    pid: &str,
    stamp: &str,
) -> Result<Value, Error> {
    let approved = approved(tx)?;
    let incoming = hashed(facts)?;
    let endings = policy::integrity(&approved, &incoming)?;
    let add: Vec<_> = incoming
        .iter()
        .filter(|(h, _)| !approved.iter().any(|(old, _)| old == h))
        .map(|(_, f)| f.clone())
        .collect();
    let unchanged: Vec<_> = incoming
        .iter()
        .filter(|(h, _)| approved.iter().any(|(old, _)| old == h))
        .map(|(h, _)| h.clone())
        .collect();
    // Conflicts retain ORIGINAL observations, as source diff does, even after prior endings.
    let mut conflicts = vec![];
    for (id, _) in &approved {
        let body: String = tx
            .query_row(
                "SELECT canonical_fact FROM main.rm_ownership_facts WHERE fact_hash=?",
                [id],
                |r| r.get(0),
            )
            .map_err(sql)?;
        let f: Fact = serde_json::from_str(&body).map_err(sql)?;
        if !incoming.iter().any(|(h, _)| h == id)
            && facts
                .iter()
                .any(|n| n.owner == f.owner && n.owned_company_slug == f.owned_company_slug)
        {
            conflicts.push(f);
        }
    }
    conflicts = hashed(&conflicts)?.into_iter().map(|(_, f)| f).collect();
    let diff = json!({"version":2,"add":add,"unchanged":unchanged,"conflicts":conflicts,"endings":endings,"note":"Only an explicit later fact can end an approved legal fact"});
    let source = rentemester_engine::company_knowledge::text(&input.source, 160)?;
    let observed = rentemester_engine::company_knowledge::iso(&input.observed_at)?;
    let facts: Vec<_> = incoming.into_iter().map(|(_, f)| f).collect();
    let sh = hash(&json!({"source":source,"observedAt":observed,"facts":facts}));
    let dh = hash(&diff);
    let id = rentemester_engine::company_knowledge::text(
        input.snapshot_id.as_deref().unwrap_or(generated),
        80,
    )?;
    let exists: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM main.rm_ownership_source_snapshots WHERE snapshot_id=?)",
            [&id],
            |r| r.get(0),
        )
        .map_err(sql)?;
    if exists {
        let old = load(tx, &id)?.public;
        if old["snapshotHash"] != sh || old["diffHash"] != dh {
            return Err(policy::refused("snapshotId immutable content differs"));
        }
        return Ok(old);
    }
    let body = canonical(&json!(facts));
    let diff_body = canonical(&diff);
    if body.len() > 1048576 || diff_body.len() > 1048576 {
        return Err(policy::refused("snapshot exceeds canonical storage bounds"));
    }
    tx.execute("INSERT INTO main.rm_ownership_source_snapshots(snapshot_id,source,observed_at,snapshot_hash,diff_hash,canonical_facts,diff_json,actor,principal_kind,principal_id,created_at) VALUES(?,?,?,?,?,?,?,?,'user',?,?)",params![id,source,observed,sh,dh,body,diff_body,actor,pid,stamp]).map_err(sql)?;
    event(tx, &id, "proposed", actor, pid, stamp)?;
    Ok(load(tx, &id)?.public)
}
pub fn review(
    tx: &Transaction<'_>,
    input: &Review,
    actor: &str,
    pid: &str,
    stamp: &str,
) -> Result<Value, Error> {
    if !["approved", "rejected"].contains(&input.decision.as_str()) {
        return Err(policy::refused("approved or rejected decision required"));
    }
    let snapshot = load(tx, &input.snapshot_id)?.public;
    if snapshot["principal"]["kind"] == "user" && snapshot["principal"]["id"] == pid {
        return Err(policy::refused(
            "independent authenticated reviewer required",
        ));
    }
    if snapshot["state"] == input.decision {
        return Ok(snapshot);
    }
    if snapshot["state"] != "proposed" {
        return Err(policy::refused("snapshot has already been reviewed"));
    }
    event(tx, &input.snapshot_id, &input.decision, actor, pid, stamp)?;
    Ok(load(tx, &input.snapshot_id)?.public)
}
pub fn apply(
    tx: &Transaction<'_>,
    input: &Apply,
    actor: &str,
    pid: &str,
    stamp: &str,
) -> Result<Value, Error> {
    let s = load(tx, &input.snapshot_id)?.public;
    if s["snapshotHash"] != input.snapshot_hash || s["diffHash"] != input.diff_hash {
        return Err(policy::refused("exact snapshot and diff hashes required"));
    }
    if s["state"] == "applied" {
        return Ok(json!({"status":"unchanged","snapshot":s}));
    }
    if s["state"] != "approved" {
        return Err(policy::refused("approved snapshot required"));
    }
    let additions: Vec<Fact> = serde_json::from_value(s["diff"]["add"].clone()).map_err(sql)?;
    let planned: Vec<Ending> = serde_json::from_value(s["diff"]["endings"].clone()).map_err(sql)?;
    let incoming = hashed(&additions)?;
    let endings = policy::integrity(&approved(tx)?, &incoming)?;
    if endings != planned {
        return Err(policy::refused(
            "snapshot stale against current approved facts",
        ));
    }
    for (h, f) in incoming {
        let value = serde_json::to_value(&f).map_err(sql)?;
        let body = canonical(&value);
        if body.len() > 32768 {
            return Err(policy::refused("fact exceeds canonical storage bound"));
        }
        let (owner_kind, owner_id) = match &f.owner {
            Endpoint::Company { company_slug } => ("company", company_slug),
            Endpoint::Party { party_id } => ("party", party_id),
        };
        tx.execute("INSERT OR IGNORE INTO main.rm_ownership_facts(fact_id,snapshot_id,fact_hash,canonical_fact,owner_kind,owner_id,owned_company_slug,valid_from,valid_to_exclusive,economic_exact_bp,economic_min_bp,economic_max_bp,voting_bp,control_type,share_class,jurisdiction,review_state,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,'approved',?)",params![format!("ownership-fact-{}",&h[..32]),input.snapshot_id,h,body,owner_kind,owner_id,f.owned_company_slug,f.valid_from,f.valid_to_exclusive,f.economic_basis_points,f.economic_interval_basis_points.as_ref().map(|i|i.min),f.economic_interval_basis_points.as_ref().map(|i|i.max),f.voting_basis_points,f.control_type,f.share_class,f.jurisdiction,stamp]).map_err(sql)?;
    }
    for e in endings {
        tx.execute("INSERT INTO main.rm_ownership_fact_events(fact_hash,event_type,effective_to_exclusive,successor_fact_hash,snapshot_id,actor,principal_kind,principal_id,created_at) VALUES(?,'ended',?,?,?,?,'user',?,?)",params![e.fact_hash,e.effective_to_exclusive,e.successor_fact_hash,input.snapshot_id,actor,pid,stamp]).map_err(sql)?;
    }
    event(tx, &input.snapshot_id, "applied", actor, pid, stamp)?;
    Ok(json!({"status":"applied","snapshot":load(tx,&input.snapshot_id)?.public}))
}
pub fn graph(as_of: &str, all: &[Fact], visible: &[Fact]) -> Value {
    let hidden = all.len() != visible.len();
    let reason = if hidden {
        "hidden endpoints"
    } else if all.iter().any(|f| {
        matches!(f.owner, Endpoint::Party { .. })
            || f.economic_basis_points.is_none()
            || f.valid_to_exclusive.is_some()
    }) {
        "party, interval or non-exact ownership requires explicit supported scope"
    } else {
        "existing v1 manifest remains authoritative"
    };
    json!({"asOf":as_of,"facts":visible,"partial":hidden,"consolidation":{"eligible":false,"reason":reason}})
}
pub fn projection(graph: &Value) -> Value {
    if graph["consolidation"]["reason"] != "existing v1 manifest remains authoritative" {
        return json!({"eligible":false,"reason":graph["consolidation"]["reason"],"edges":[]});
    }
    let mut totals: BTreeMap<&str, u64> = BTreeMap::new();
    for f in graph["facts"].as_array().unwrap() {
        *totals
            .entry(f["ownedCompanySlug"].as_str().unwrap())
            .or_default() += f["economicBasisPoints"].as_u64().unwrap();
    }
    if totals.values().any(|n| *n != 10000) {
        return json!({"eligible":false,"reason":"incomplete or minority ownership totals","edges":[]});
    }
    json!({"eligible":true,"edges":graph["facts"].as_array().unwrap().iter().map(|f|json!({"parentCompanySlug":f["owner"]["companySlug"],"childCompanySlug":f["ownedCompanySlug"],"basisPoints":f["economicBasisPoints"]})).collect::<Vec<_>>()})
}
#[cfg(test)]
mod tests;
