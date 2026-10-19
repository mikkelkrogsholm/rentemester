//! Canonical schema10 elimination evidence. All transitions append control events only.
use crate::intercompany::sha;
use rentemester_engine::{
    Error,
    eliminations::{BalancePayload, refused},
    fiscal_year::Date,
};
use rusqlite::{OptionalExtension, Transaction, params};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
fn err(_: impl std::fmt::Display) -> Error {
    refused()
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub payload: BalancePayload,
    pub payload_hash: String,
    pub event_hash: String,
    pub status: String,
    pub actor: String,
    #[serde(skip)]
    pub proposer: String,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Applied {
    pub elimination_id: String,
    pub payload_hash: String,
    pub event_hash: String,
    pub payload: BalancePayload,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Event<'a> {
    previous_hash: Option<&'a str>,
    id: i64,
    elimination_id: &'a str,
    event_type: &'a str,
    payload_hash: &'a str,
    canonical_payload: &'a str,
    actor: &'a str,
    created_at: &'a str,
}
fn digest(e: &Event<'_>) -> Result<String, Error> {
    Ok(sha(&serde_json::to_string(e).map_err(err)?))
}
fn transition(old: Option<&State>, kind: &str, hash: &str, actor: &str) -> bool {
    match kind {
        "proposed" => old.is_none_or(|s| matches!(s.status.as_str(), "rejected" | "reversed")),
        "approved" => old.is_some_and(|s| {
            s.status == "proposed" && s.payload_hash == hash && s.proposer != actor
        }),
        "rejected" => old.is_some_and(|s| s.status == "proposed" && s.payload_hash == hash),
        "applied" => old.is_some_and(|s| {
            s.status == "approved" && s.payload_hash == hash && s.proposer != actor
        }),
        "reversed" => old.is_some_and(|s| s.status == "applied" && s.payload_hash == hash),
        _ => false,
    }
}
fn duplicate(states: &BTreeMap<String, State>, payload: &BalancePayload) -> bool {
    states.values().any(|s| {
        s.status == "applied"
            && s.payload.mapping_id == payload.mapping_id
            && s.payload.as_of == payload.as_of
    })
}
pub fn states(tx: &Transaction<'_>) -> Result<BTreeMap<String, State>, Error> {
    let mut q=tx.prepare("SELECT id,elimination_id,event_type,payload_hash,canonical_payload,previous_hash,event_hash,actor,created_at FROM main.rm_consolidation_elimination_events ORDER BY id").map_err(err)?;
    let rows = q
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, Option<String>>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, String>(7)?,
                r.get::<_, String>(8)?,
            ))
        })
        .map_err(err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(err)?;
    let mut previous: Option<String> = None;
    let mut states: BTreeMap<String, State> = BTreeMap::new();
    for (i, (id, eid, kind, hash, canonical, p, eh, actor, at)) in rows.into_iter().enumerate() {
        if id != i as i64 + 1
            || previous != p
            || sha(&canonical) != hash
            || digest(&Event {
                previous_hash: p.as_deref(),
                id,
                elimination_id: &eid,
                event_type: &kind,
                payload_hash: &hash,
                canonical_payload: &canonical,
                actor: &actor,
                created_at: &at,
            })? != eh
        {
            return Err(refused());
        }
        let payload: BalancePayload = serde_json::from_str(&canonical).map_err(err)?;
        if payload.id != eid
            || payload.canonical()? != canonical
            || !transition(states.get(&eid), &kind, &hash, &actor)
            || (kind == "applied" && duplicate(&states, &payload))
        {
            return Err(refused());
        }
        let proposer = if kind == "proposed" {
            actor.clone()
        } else {
            states.get(&eid).ok_or_else(refused)?.proposer.clone()
        };
        states.insert(
            eid,
            State {
                payload,
                payload_hash: hash,
                event_hash: eh.clone(),
                status: kind,
                actor,
                proposer,
            },
        );
        previous = Some(eh);
    }
    Ok(states)
}
/// Validates EVERY persisted row before selecting current applied evidence for the EXACT report date.
/// Fresh ledger/mapping comparison and current authority remain the caller's separate fence.
pub fn applied(tx: &Transaction<'_>, as_of: &str) -> Result<Vec<Applied>, Error> {
    Date::parse(as_of)?;
    Ok(states(tx)?
        .into_iter()
        .filter(|(_, s)| s.status == "applied" && s.payload.as_of == as_of)
        .map(|(id, s)| Applied {
            elimination_id: id,
            payload_hash: s.payload_hash,
            event_hash: s.event_hash,
            payload: s.payload,
        })
        .collect())
}
pub fn append(
    tx: &Transaction<'_>,
    payload: &BalancePayload,
    kind: &str,
    actor: &str,
    at: &str,
) -> Result<Value, Error> {
    let states = states(tx)?;
    let canonical = payload.canonical()?;
    if canonical.len() > 262144 {
        return Err(refused());
    }
    let hash = sha(&canonical);
    if !transition(states.get(&payload.id), kind, &hash, actor)
        || (kind == "applied" && duplicate(&states, payload))
    {
        return Err(refused());
    }
    let old:Option<(i64,String)>=tx.query_row("SELECT id,event_hash FROM main.rm_consolidation_elimination_events ORDER BY id DESC LIMIT 1",[],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(err)?;
    let id = old.as_ref().map_or(1, |v| v.0 + 1);
    let previous = old.as_ref().map(|v| v.1.as_str());
    let event_hash = digest(&Event {
        previous_hash: previous,
        id,
        elimination_id: &payload.id,
        event_type: kind,
        payload_hash: &hash,
        canonical_payload: &canonical,
        actor,
        created_at: at,
    })?;
    tx.execute("INSERT INTO main.rm_consolidation_elimination_events(id,elimination_id,event_type,payload_hash,canonical_payload,previous_hash,event_hash,actor,created_at) VALUES(?,?,?,?,?,?,?,?,?)",params![id,payload.id,kind,hash,canonical,previous,event_hash,actor,at]).map_err(err)?;
    tx.execute("INSERT INTO main.workspace_audit(event_type,entity_type,entity_id,actor,created_at) VALUES(?,'consolidation_elimination',?,?,?)",params![format!("consolidation_elimination_{kind}"),payload.id,actor,at]).map_err(err)?;
    Ok(json!({"eliminationId":payload.id,"payloadHash":hash,"eventHash":event_hash,"status":kind}))
}
#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn payload(version: u8, reverse: bool) -> BalancePayload {
        let mut v = json!({"version":version,"id":"eliminate","kind":"intercompany-balance","groupId":"synthetic","mappingId":"mapping","mappingHash":"a".repeat(64),"asOf":"2026-01-01","currency":"DKK","amountOre":"1234","left":{"companySlug":"a","accountNos":["1400"],"selectionHash":"b".repeat(64),"ledgerHeadHash":"c".repeat(64),"entryCount":1,"creditOre":"1234"},"right":{"companySlug":"b","accountNos":["2400"],"selectionHash":"d".repeat(64),"ledgerHeadHash":"e".repeat(64),"entryCount":1,"debitOre":"1234"},"evidenceRefs":["synthetic"]});
        if version == 2 {
            v["left"]["creditOre"] = json!(if reverse { "0" } else { "1234" });
            v["left"]["debitOre"] = json!(if reverse { "1234" } else { "0" });
            v["left"]["position"] = json!(if reverse { "payable" } else { "receivable" });
            v["right"]["creditOre"] = json!(if reverse { "1234" } else { "0" });
            v["right"]["debitOre"] = json!(if reverse { "0" } else { "1234" });
            v["right"]["position"] = json!(if reverse { "receivable" } else { "payable" });
        }
        serde_json::from_value(v).unwrap()
    }
    fn control() -> rusqlite::Connection {
        let mut c = rusqlite::Connection::open_in_memory().unwrap();
        let mut l = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic").unwrap();
        c
    }
    const AT: &str = "2026-01-01T00:00:00.000Z";
    #[test]
    fn v1_source_canonical_bytes_and_v2_both_polarity_goldens() {
        for (version, reverse, expected) in [
            (
                1,
                false,
                "0eec273a2bb76bdeda639e4cd6f14fe52af76cf7cf38877ed8b75c8a6500ec1d",
            ),
            (
                2,
                false,
                "bfd2dd256576a9c7c7be7119cb891aa1ffd931221ae72af9d5af98a6243b9565",
            ),
            (
                2,
                true,
                "f34a4766d19ab0472a1a8d1b6cb377f1fc4f78345ca2a80825b9227222905b26",
            ),
        ] {
            let p = payload(version, reverse);
            assert_eq!(sha(&p.canonical().unwrap()), expected);
            let a = p.adjustments().unwrap();
            assert_eq!(a[0].company_slug, "a");
            assert_eq!(a[1].company_slug, "b");
            assert_eq!(a[0].debit_ore, if reverse { "1234" } else { "0" });
            assert_eq!(a[1].credit_ore, if reverse { "1234" } else { "0" });
            let again: BalancePayload = serde_json::from_str(&p.canonical().unwrap()).unwrap();
            assert_eq!(again.canonical().unwrap(), p.canonical().unwrap());
        }
        let legacy = payload(1, false);
        assert!(legacy.same_current_snapshot(&payload(2, false)).unwrap());
        assert!(!legacy.same_current_snapshot(&payload(2, true)).unwrap());
        let mut p = payload(2, true);
        p.left.credit_ore = Some("1234".into());
        assert!(p.validate().is_err());
        p = payload(1, false);
        p.right.debit_ore = Some("1235".into());
        assert!(p.validate().is_err());
    }
    #[test]
    fn append_only_review_apply_reverse_duplicate_guard_and_exact_date() {
        let mut c = control();
        let tx = c.transaction().unwrap();
        let p = payload(2, false);
        assert!(applied(&tx, "2026-01-01").unwrap().is_empty());
        append(&tx, &p, "proposed", "user:maker", AT).unwrap();
        assert!(append(&tx, &p, "applied", "user:reviewer", AT).is_err());
        assert!(append(&tx, &p, "approved", "user:maker", AT).is_err());
        append(&tx, &p, "approved", "user:reviewer", AT).unwrap();
        assert!(append(&tx, &p, "applied", "user:maker", AT).is_err());
        append(&tx, &p, "applied", "user:reviewer", AT).unwrap();
        assert_eq!(applied(&tx, "2026-01-01").unwrap().len(), 1);
        assert!(applied(&tx, "2026-01-02").unwrap().is_empty());
        let mut replacement = p.clone();
        replacement.id = "replacement".into();
        append(&tx, &replacement, "proposed", "user:maker", AT).unwrap();
        append(&tx, &replacement, "approved", "user:reviewer", AT).unwrap();
        assert!(append(&tx, &replacement, "applied", "user:reviewer", AT).is_err());
        append(&tx, &p, "reversed", "user:maker", AT).unwrap();
        append(&tx, &replacement, "applied", "user:reviewer", AT).unwrap();
        let rows = applied(&tx, "2026-01-01").unwrap();
        assert_eq!(rows[0].elimination_id, "replacement");
        assert_eq!(
            tx.query_row(
                "SELECT count(*) FROM rm_consolidation_elimination_events",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            7
        );
    }
    #[test]
    fn rejected_lifecycle_and_entire_chain_corruption_refuse_before_date_filter() {
        let mut c = control();
        let tx = c.transaction().unwrap();
        let p = payload(1, false);
        append(&tx, &p, "proposed", "user:maker", AT).unwrap();
        append(&tx, &p, "rejected", "user:maker", AT).unwrap();
        append(&tx, &p, "proposed", "user:newmaker", AT).unwrap();
        append(&tx, &p, "approved", "user:maker", AT).unwrap();
        append(&tx, &p, "applied", "user:maker", AT).unwrap();
        assert_eq!(applied(&tx, "2026-01-01").unwrap().len(), 1);
        tx.execute_batch("DROP TRIGGER rm_consolidation_elimination_events_no_update;UPDATE rm_consolidation_elimination_events SET actor='user:tampered' WHERE id=1").unwrap();
        assert!(applied(&tx, "2099-01-01").is_err());
    }
    #[test]
    fn unknown_payload_version_refused_even_with_correct_stored_hashes() {
        let mut c = control();
        let tx = c.transaction().unwrap();
        let p = payload(2, false);
        append(&tx, &p, "proposed", "user:maker", AT).unwrap();
        let mut v = serde_json::to_value(&p).unwrap();
        v["version"] = json!(3);
        let canonical = v.to_string();
        let hash = sha(&canonical);
        let eh = digest(&Event {
            previous_hash: None,
            id: 1,
            elimination_id: &p.id,
            event_type: "proposed",
            payload_hash: &hash,
            canonical_payload: &canonical,
            actor: "user:maker",
            created_at: AT,
        })
        .unwrap();
        tx.execute_batch("DROP TRIGGER rm_consolidation_elimination_events_no_update")
            .unwrap();
        tx.execute("UPDATE rm_consolidation_elimination_events SET canonical_payload=?,payload_hash=?,event_hash=? WHERE id=1",params![canonical,hash,eh]).unwrap();
        assert!(applied(&tx, "2026-01-01").is_err());
    }
}
