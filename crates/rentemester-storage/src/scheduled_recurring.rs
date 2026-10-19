//! Durable company-local generation consent and append-only occurrence decisions; no live grants.
use crate::{invalid, recurring_invoices as recurring, scheduled_recurring_schema as schema};
use rentemester_engine::{
    Error,
    scheduled_recurring::{self as engine, Occurrence, refused},
};
use rusqlite::{OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
pub fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Data {
    reason: Option<String>,
    occurrences: Vec<Occurrence>,
    generation: Option<Value>,
    error: Option<String>,
    reviewed_plan: Option<Value>,
}
impl Data {
    fn empty() -> Self {
        Self {
            reason: None,
            occurrences: vec![],
            generation: None,
            error: None,
            reviewed_plan: None,
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Payload {
    version: u8,
    template_id: i64,
    template_hash: String,
    kind: String,
    actor: String,
    at: String,
    data: Data,
}
#[derive(Clone)]
struct Event {
    id: i64,
    payload: Payload,
    hash: String,
}
struct State {
    state: &'static str,
    owner: Option<String>,
    hash: String,
    queued: BTreeMap<u32, Occurrence>,
    resolved: BTreeSet<u32>,
    last: Option<Value>,
    error: Option<String>,
    history: Vec<Value>,
}
fn spec(tx: &Transaction<'_>, id: i64) -> Result<(String, bool), Error> {
    let v = recurring::scheduled_spec(tx, id)?;
    Ok((
        sha(v["specification"].to_string().as_bytes()),
        v["active"] == true,
    ))
}
fn chain_hash(previous: &str, payload: &str) -> String {
    sha(json!([previous, payload]).to_string().as_bytes())
}
fn events(tx: &Transaction<'_>) -> Result<Vec<Event>, Error> {
    if !schema::installed(tx)? {
        return Ok(vec![]);
    }
    let mut q=tx.prepare("SELECT id,template_id,kind,template_hash,actor,canonical_payload,previous_hash,event_hash,created_at FROM native_scheduled_recurring_events ORDER BY id").map_err(invalid)?;
    let rows = q
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, String>(7)?,
                r.get::<_, String>(8)?,
            ))
        })
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    let mut previous = "0".repeat(64);
    let mut result = vec![];
    for (id, template, kind, hash, actor, canonical, prev, event_hash, at) in rows {
        let p: Payload = serde_json::from_str(&canonical).map_err(invalid)?;
        if id != result.len() as i64 + 1
            || p.version != 1
            || p.template_id != template
            || p.kind != kind
            || p.template_hash != hash
            || p.actor != actor
            || p.at != at
            || p.actor.is_empty()
            || !engine::hash(&hash)
            || serde_json::to_string(&p).map_err(invalid)? != canonical
            || prev != previous
            || event_hash != chain_hash(&prev, &canonical)
            || spec(tx, template)?.0 != hash
        {
            return Err(refused());
        }
        for o in &p.data.occurrences {
            o.validate()?;
            if recurring::scheduled_date(tx, template, o.period_index)? != o.issue_date {
                return Err(refused());
            }
        }
        if p.data
            .reason
            .as_ref()
            .is_some_and(|r| engine::reason(r).is_err())
        {
            return Err(refused());
        }
        previous = event_hash.clone();
        result.push(Event {
            id,
            payload: p,
            hash: event_hash,
        });
    }
    Ok(result)
}
fn state(tx: &Transaction<'_>, id: i64, hash: &str, all: &[Event]) -> Result<State, Error> {
    let mut s = State {
        state: "disabled",
        owner: None,
        hash: sha(format!("{id}:{hash}:disabled").as_bytes()),
        queued: BTreeMap::new(),
        resolved: BTreeSet::new(),
        last: None,
        error: None,
        history: vec![],
    };
    let mut pending: Option<Occurrence> = None;
    for e in all.iter().filter(|e| e.payload.template_id == id) {
        let p = &e.payload;
        let d = &p.data;
        let planned = matches!(p.kind.as_str(), "queued" | "skipped" | "attempted");
        if planned != d.reviewed_plan.is_some()
            || (!matches!(p.kind.as_str(), "succeeded") && d.generation.is_some())
            || (!matches!(p.kind.as_str(), "failed") && d.error.is_some())
            || (!matches!(
                p.kind.as_str(),
                "paused" | "resumed" | "cancelled" | "skipped"
            ) && d.reason.is_some())
        {
            return Err(refused());
        }
        if let Some(review) = &d.reviewed_plan {
            let mut canonical = review.clone();
            let reviewed_hash = canonical
                .as_object_mut()
                .ok_or_else(refused)?
                .remove("planHash")
                .ok_or_else(refused)?;
            if reviewed_hash != sha(canonical.to_string().as_bytes())
                || review["templateId"] != id
                || review["templateHash"] != hash
                || review["stateHash"] != s.hash
            {
                return Err(refused());
            }
            let mut allowed = if p.kind == "attempted" {
                review["queued"].as_array().ok_or_else(refused)?.clone()
            } else {
                review["missed"].as_array().ok_or_else(refused)?.clone()
            };
            if p.kind == "attempted" && !review["normalDue"].is_null() {
                allowed.push(review["normalDue"].clone());
            }
            if !d.occurrences.iter().all(|o| allowed.contains(&json!(o))) {
                return Err(refused());
            }
        }

        if pending.is_some() && !matches!(p.kind.as_str(), "succeeded" | "failed") {
            return Err(refused());
        }
        match p.kind.as_str() {
            "enabled" if s.state == "disabled" && d.occurrences.is_empty() => {
                s.state = "enabled";
                s.owner = Some(p.actor.clone());
            }
            "paused" if s.state == "enabled" && d.reason.is_some() => s.state = "paused",
            "resumed"
                if s.state == "paused"
                    && s.owner.as_deref() == Some(&p.actor)
                    && d.reason.is_some() =>
            {
                s.state = "enabled"
            }
            "cancelled" if matches!(s.state, "enabled" | "paused") && d.reason.is_some() => {
                s.state = "cancelled";
                s.queued.clear();
            }
            "queued" | "skipped"
                if matches!(s.state, "enabled" | "paused") && !d.occurrences.is_empty() =>
            {
                for o in &d.occurrences {
                    if s.resolved.contains(&o.period_index)
                        || s.queued.contains_key(&o.period_index)
                    {
                        return Err(refused());
                    }
                    if p.kind == "queued" {
                        s.queued.insert(o.period_index, o.clone());
                    } else {
                        if d.reason.is_none() {
                            return Err(refused());
                        }
                        s.resolved.insert(o.period_index);
                    }
                }
            }
            "attempted"
                if s.state == "enabled"
                    && s.owner.as_deref() == Some(&p.actor)
                    && d.occurrences.len() == 1 =>
            {
                let o = d.occurrences[0].clone();
                if s.resolved.contains(&o.period_index) {
                    return Err(refused());
                }
                s.queued.remove(&o.period_index);
                s.resolved.insert(o.period_index);
                pending = Some(o.clone());
                s.last = Some(
                    json!({"periodIndex":o.period_index,"issueDate":o.issue_date,"status":"unresolved","at":p.at}),
                );
                s.error = Some("unresolved_attempt".into());
            }
            "succeeded" | "failed"
                if pending.is_some()
                    && d.occurrences.len() == 1
                    && pending.as_ref() == d.occurrences.first()
                    && s.owner.as_deref() == Some(&p.actor) =>
            {
                let o = pending.take().unwrap();
                if p.kind == "succeeded" {
                    let g = d.generation.as_ref().ok_or_else(refused)?;
                    let actual:Option<i64>=tx.query_row("SELECT document_id FROM recurring_invoice_generations WHERE template_id=?1 AND period_index=?2",params![id,o.period_index],|r|r.get(0)).optional().map_err(invalid)?;
                    if actual != g["documentId"].as_i64()
                        || actual.is_none()
                        || g["templateId"] != id
                        || g["periodIndex"] != o.period_index
                    {
                        return Err(refused());
                    }
                    s.error = None;
                } else {
                    if d.error.as_deref().is_none_or(str::is_empty) || d.generation.is_some() {
                        return Err(refused());
                    }
                    s.error = d.error.clone();
                }
                s.last = Some(
                    json!({"periodIndex":o.period_index,"issueDate":o.issue_date,"status":p.kind,"generation":d.generation,"error":d.error,"at":p.at}),
                );
            }
            _ => return Err(refused()),
        }
        s.hash = e.hash.clone();
        s.history.push(
            json!({"id":e.id,"kind":p.kind,"actor":p.actor,"at":p.at,"data":d,"eventHash":e.hash}),
        );
    }
    Ok(s)
}
fn append(
    tx: &Transaction<'_>,
    id: i64,
    kind: &str,
    actor: &str,
    at: &str,
    data: Data,
) -> Result<(), Error> {
    let all = events(tx)?;
    let hash = spec(tx, id)?.0;
    let prev = all
        .last()
        .map(|e| e.hash.clone())
        .unwrap_or_else(|| "0".repeat(64));
    let p = Payload {
        version: 1,
        template_id: id,
        template_hash: hash.clone(),
        kind: kind.into(),
        actor: actor.into(),
        at: at.into(),
        data,
    };
    let canonical = serde_json::to_string(&p).map_err(invalid)?;
    let event_hash = chain_hash(&prev, &canonical);
    tx.execute("INSERT INTO native_scheduled_recurring_events(id,template_id,kind,template_hash,actor,canonical_payload,previous_hash,event_hash,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![all.len()as i64+1,id,kind,hash,actor,canonical,prev,event_hash,at]).map_err(invalid)?;
    let updated = events(tx)?;
    state(tx, id, &hash, &updated)?;
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES('scheduled_recurring','recurring_invoice_template',?1,?2,?3)",params![id.to_string(),kind,format!("user:{actor}")]).map_err(invalid)?;
    Ok(())
}
fn resolved(tx: &Transaction<'_>, id: i64, s: &State) -> Result<BTreeSet<u32>, Error> {
    let mut set = s.resolved.clone();
    let mut q = tx
        .prepare("SELECT period_index FROM recurring_invoice_generations WHERE template_id=?1")
        .map_err(invalid)?;
    for index in q.query_map([id], |r| r.get::<_, u32>(0)).map_err(invalid)? {
        set.insert(index.map_err(invalid)?);
    }
    Ok(set)
}
fn plan_from(
    tx: &Transaction<'_>,
    id: i64,
    today: &str,
    hash: &str,
    s: &State,
) -> Result<Value, Error> {
    let completed = resolved(tx, id, s)?;
    let latest = recurring::scheduled_latest(tx, id, today)?;
    let mut missed = vec![];
    let mut missed_count = 0u32;
    let mut normal = None;
    if let Some(last) = latest {
        if last > 1_000_000 {
            return Err(refused());
        }
        for index in 0..=last {
            if completed.contains(&index) || s.queued.contains_key(&index) {
                continue;
            }
            let date = if index == last || missed.len() < engine::MAX_SELECTION {
                Some(recurring::scheduled_date(tx, id, index)?)
            } else {
                None
            };
            if date.as_deref() == Some(today) {
                if s.state == "enabled"
                    && spec(tx, id)?.1
                    && s.error.as_deref() != Some("unresolved_attempt")
                {
                    normal = Some(Occurrence {
                        period_index: index,
                        issue_date: date.unwrap(),
                    });
                }
            } else {
                missed_count += 1;
                if missed.len() < engine::MAX_SELECTION {
                    missed.push(Occurrence {
                        period_index: index,
                        issue_date: date.unwrap(),
                    });
                }
            }
        }
    }
    let mut p = json!({"templateId":id,"template":recurring::scheduled_spec(tx,id)?["specification"],"templateHash":hash,"stateHash":s.hash,"asOfDate":today,"normalDue":normal,"missed":missed,"missedCount":missed_count,"truncated":missed_count as usize>missed.len(),"queued":s.queued.values().collect::<Vec<_>>()});
    let h = sha(p.to_string().as_bytes());
    p["planHash"] = json!(h);
    Ok(p)
}
pub fn plan(tx: &Transaction<'_>, id: i64, today: &str) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    let (hash, _) = spec(tx, id)?;
    let s = state(tx, id, &hash, &events(tx)?)?;
    plan_from(tx, id, today, &hash, &s)
}
pub fn current(tx: &Transaction<'_>, id: i64, today: &str) -> Result<Value, Error> {
    let (hash, active) = spec(tx, id)?;
    let s = state(tx, id, &hash, &events(tx)?)?;
    let p = plan_from(tx, id, today, &hash, &s)?;
    let next = if let Some(last) = recurring::scheduled_latest(tx, id, today)? {
        let date = recurring::scheduled_date(tx, id, last)?;
        if date == today && !resolved(tx, id, &s)?.contains(&last) {
            Occurrence {
                period_index: last,
                issue_date: date,
            }
        } else {
            Occurrence {
                period_index: last + 1,
                issue_date: recurring::scheduled_date(tx, id, last + 1)?,
            }
        }
    } else {
        Occurrence {
            period_index: 0,
            issue_date: recurring::scheduled_date(tx, id, 0)?,
        }
    };
    Ok(
        json!({"templateId":id,"templateHash":hash,"state":s.state,"stateHash":s.hash,"ownerUserId":s.owner,"lastRun":s.last,"nextOccurrence":if active{Some(next)}else{None},"missedCount":p["missedCount"],"queued":p["queued"],"error":if !active{Some("template_retired".into())}else{s.error},"history":s.history}),
    )
}
pub fn list(tx: &Transaction<'_>, slug: &str, today: &str) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    let templates = recurring::list(tx, slug)?;
    let mut rows = vec![];
    for row in templates["templates"].as_array().ok_or_else(refused)? {
        rows.push(current(tx, row["id"].as_i64().ok_or_else(refused)?, today)?);
    }
    Ok(json!({"asOfDate":today,"timeZone":"Europe/Copenhagen","localService":false,"rows":rows}))
}
pub fn enable(
    tx: &Transaction<'_>,
    i: &engine::Enable,
    actor: &str,
    today: &str,
    at: &str,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    let (hash, active) = spec(tx, i.template_id)?;
    let s = state(tx, i.template_id, &hash, &events(tx)?)?;
    if !i.confirm || !active || i.template_hash != hash || s.state != "disabled" {
        return Err(refused());
    }
    schema::install(tx)?;
    append(tx, i.template_id, "enabled", actor, at, Data::empty())?;
    current(tx, i.template_id, today)
}
pub fn change(
    tx: &Transaction<'_>,
    kind: &str,
    i: &engine::Change,
    actor: &str,
    today: &str,
    at: &str,
) -> Result<Value, Error> {
    let (hash, active) = spec(tx, i.template_id)?;
    let s = state(tx, i.template_id, &hash, &events(tx)?)?;
    let reason = engine::reason(&i.reason)?;
    if !i.confirm
        || i.expected_state_hash != s.hash
        || !match kind {
            "paused" => s.state == "enabled",
            "resumed" => s.state == "paused" && active && s.owner.as_deref() == Some(actor),
            "cancelled" => matches!(s.state, "enabled" | "paused"),
            _ => false,
        }
    {
        return Err(refused());
    }
    let mut d = Data::empty();
    d.reason = Some(reason.into());
    append(tx, i.template_id, kind, actor, at, d)?;
    current(tx, i.template_id, today)
}
#[allow(clippy::too_many_arguments)]
pub fn select(
    tx: &Transaction<'_>,
    id: i64,
    hash: &str,
    indices: &[u32],
    reason: Option<&str>,
    confirmed: bool,
    actor: &str,
    today: &str,
    at: &str,
) -> Result<Value, Error> {
    engine::selection(indices)?;
    let (template_hash, active) = spec(tx, id)?;
    let s = state(tx, id, &template_hash, &events(tx)?)?;
    let p = plan_from(tx, id, today, &template_hash, &s)?;
    if !confirmed || !active || p["planHash"] != hash || !matches!(s.state, "enabled" | "paused") {
        return Err(refused());
    }
    let available: Vec<Occurrence> =
        serde_json::from_value(p["missed"].clone()).map_err(invalid)?;
    let mut data = Data::empty();
    for index in indices {
        data.occurrences.push(
            available
                .iter()
                .find(|o| o.period_index == *index)
                .ok_or_else(refused)?
                .clone(),
        );
    }
    data.reviewed_plan = Some(p.clone());
    data.reason = reason.map(engine::reason).transpose()?.map(str::to_owned);
    append(
        tx,
        id,
        if reason.is_some() {
            "skipped"
        } else {
            "queued"
        },
        actor,
        at,
        data,
    )?;
    current(tx, id, today)
}
pub fn begin(
    tx: &Transaction<'_>,
    i: &engine::Dispatch,
    actor: &str,
    today: &str,
    at: &str,
) -> Result<Occurrence, Error> {
    let (hash, active) = spec(tx, i.template_id)?;
    let s = state(tx, i.template_id, &hash, &events(tx)?)?;
    let p = plan_from(tx, i.template_id, today, &hash, &s)?;
    if !i.confirm
        || !active
        || s.state != "enabled"
        || s.owner.as_deref() != Some(actor)
        || p["planHash"] != i.plan_hash
    {
        return Err(refused());
    }
    let mut allowed = s.queued.values().cloned().collect::<Vec<_>>();
    if !p["normalDue"].is_null() {
        allowed.push(serde_json::from_value(p["normalDue"].clone()).map_err(invalid)?);
    }
    let o = allowed
        .into_iter()
        .find(|o| o.period_index == i.period_index)
        .ok_or_else(refused)?;
    if resolved(tx, i.template_id, &s)?.contains(&o.period_index) {
        return Err(refused());
    }
    let mut d = Data::empty();
    d.occurrences.push(o.clone());
    d.reviewed_plan = Some(p);
    append(tx, i.template_id, "attempted", actor, at, d)?;
    Ok(o)
}
pub fn finish(
    tx: &Transaction<'_>,
    id: i64,
    o: Occurrence,
    result: Result<Value, String>,
    actor: &str,
    today: &str,
    at: &str,
) -> Result<Value, Error> {
    let mut d = Data::empty();
    d.occurrences.push(o);
    let mut response = json!({});
    match result {
        Ok(g) => {
            d.generation = Some(g.clone());
            response["generation"] = g;
            response["dispatch"] = json!({"status":"succeeded"});
        }
        Err(e) => {
            d.error = Some(e.clone());
            response["dispatch"] = json!({"status":"failed","error":e});
        }
    }
    append(
        tx,
        id,
        if d.error.is_some() {
            "failed"
        } else {
            "succeeded"
        },
        actor,
        at,
        d,
    )?;
    response["scheduledRecurring"] = current(tx, id, today)?;
    Ok(response)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dated_dst_windows_bounded_missed_and_read_only_plans() {
        let mut control = rusqlite::Connection::open_in_memory().unwrap();
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic dated scheduler")
            .unwrap();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        tx.execute_batch("UPDATE companies SET address='Synthetic1',cvr='87654321',vat_period_type='quarter' WHERE id=1").unwrap();
        let customer = crate::sales::create_customer(
            &tx,
            &serde_json::from_value(
                json!({"name":"Synthetic customer","address":"Synthetic2","vatOrCvr":"DK12345678"}),
            )
            .unwrap(),
            "owner",
        )
        .unwrap()["id"]
            .as_i64()
            .unwrap();
        for (first, today, missing) in [
            ("2026-03-15", "2026-03-29", 2),
            ("2026-10-11", "2026-10-25", 2),
            ("2020-01-01", "2026-01-01", 314),
        ] {
            let input=serde_json::from_value(json!({"name":"Synthetic dated recurring","interval":"weekly","intervalCount":1,"firstIssueDate":first,"paymentTermsDays":14,"vatRatePercent":25,"currency":"DKK","customerId":customer,"lines":[{"description":"Synthetic service","quantity":1,"unitPriceExVat":100}],"confirm":true})).unwrap();
            let id=recurring::create(&tx,&input,"owner","2026-01-01T00:00:00Z").unwrap()["templateId"].as_i64().unwrap();
            let p = plan(&tx, id, today).unwrap();
            assert_eq!(p["missedCount"], missing);
            assert_eq!(
                p["missed"].as_array().unwrap().len(),
                usize::min(missing, 100)
            );
            assert_eq!(p["truncated"], missing > 100);
            assert_eq!(p, plan(&tx, id, today).unwrap());
            assert!(!schema::installed(&tx).unwrap());
        }
        assert_eq!(
            tx.query_row("SELECT count(*) FROM documents", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            tx.query_row(
                "SELECT count(*) FROM recurring_invoice_generations",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
    }
}
