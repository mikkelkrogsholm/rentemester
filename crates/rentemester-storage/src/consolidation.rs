//! Source-compatible append-only reporting profiles and admitted per-company read models.
use crate::intercompany::sha;
use rentemester_engine::{
    Error,
    consolidation::{self as policy, Account, AccountValue, Ledger, Profile},
    intercompany::Manifest,
    money::Money,
};
use rusqlite::{Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
fn err(_: impl std::fmt::Display) -> Error {
    policy::refused()
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub profile: Profile,
    pub profile_hash: String,
    pub status: String,
    pub actor: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EventHash<'a> {
    previous_hash: Option<&'a str>,
    id: i64,
    profile_id: &'a str,
    event_type: &'a str,
    profile_hash: &'a str,
    canonical_profile: &'a str,
    actor: &'a str,
    created_at: &'a str,
}
struct Event {
    id: i64,
    profile_id: String,
    kind: String,
    profile_hash: String,
    body: String,
    hash: String,
    actor: String,
}
fn events(tx: &Transaction<'_>) -> Result<Vec<Event>, Error> {
    let mut q=tx.prepare("SELECT id,profile_id,event_type,profile_hash,canonical_profile,previous_hash,event_hash,actor,created_at FROM main.rm_consolidation_profile_events ORDER BY id").map_err(err)?;
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
    let mut previous = None;
    let mut out = vec![];
    let mut states: BTreeMap<String, (String, String, String)> = BTreeMap::new();
    for (index, (id, pid, kind, ph, body, p, h, actor, at)) in rows.into_iter().enumerate() {
        let profile: Profile = serde_json::from_str(&body).map_err(err)?;
        if id != index as i64 + 1
            || p != previous
            || sha(&serde_json::to_string(&EventHash {
                previous_hash: p.as_deref(),
                id,
                profile_id: &pid,
                event_type: &kind,
                profile_hash: &ph,
                canonical_profile: &body,
                actor: &actor,
                created_at: &at,
            })
            .map_err(err)?)
                != h
            || sha(&body) != ph
            || profile.id != pid
            || profile.canonical()? != body
        {
            return Err(policy::refused());
        }
        let old = states.get(&pid);
        let valid = match kind.as_str() {
            "proposed" => old.is_none_or(|s| s.0 == "revoked"),
            "approved" => old.is_some_and(|s| s.0 == "proposed" && s.1 == ph && s.2 != actor),
            "revoked" => old.is_some_and(|s| s.0 == "approved" && s.1 == ph),
            _ => false,
        };
        if !valid {
            return Err(policy::refused());
        }
        states.insert(pid.clone(), (kind.clone(), ph.clone(), actor.clone()));
        previous = Some(h.clone());
        out.push(Event {
            id,
            profile_id: pid,
            kind,
            profile_hash: ph,
            body,
            hash: h,
            actor,
        });
    }
    Ok(out)
}
pub fn states(tx: &Transaction<'_>, manifest: &Manifest) -> Result<BTreeMap<String, State>, Error> {
    let mut current = BTreeMap::new();
    for e in events(tx)? {
        current.insert(e.profile_id.clone(), e);
    }
    current
        .into_iter()
        .map(|(id, e)| {
            let p: Profile = serde_json::from_str(&e.body).map_err(err)?;
            let profile = p.normalize(manifest)?;
            if profile.canonical()? != e.body {
                return Err(policy::refused());
            }
            Ok((
                id,
                State {
                    profile,
                    profile_hash: e.profile_hash,
                    status: e.kind,
                    actor: e.actor,
                },
            ))
        })
        .collect()
}
fn append(
    tx: &Transaction<'_>,
    profile: &Profile,
    kind: &str,
    actor: &str,
    at: &str,
) -> Result<String, Error> {
    let history = events(tx)?;
    let last = history.last();
    let id = last.map_or(1, |e| e.id + 1);
    let previous = last.map(|e| e.hash.as_str());
    let body = profile.canonical()?;
    let profile_hash = sha(&body);
    let event_hash = sha(&serde_json::to_string(&EventHash {
        previous_hash: previous,
        id,
        profile_id: &profile.id,
        event_type: kind,
        profile_hash: &profile_hash,
        canonical_profile: &body,
        actor,
        created_at: at,
    })
    .map_err(err)?);
    tx.execute("INSERT INTO main.rm_consolidation_profile_events(id,profile_id,event_type,profile_hash,canonical_profile,previous_hash,event_hash,actor,created_at) VALUES(?,?,?,?,?,?,?,?,?)",params![id,profile.id,kind,profile_hash,body,previous,event_hash,actor,at]).map_err(err)?;
    tx.execute("INSERT INTO main.workspace_audit(event_type,entity_type,entity_id,actor,created_at) VALUES(?,'consolidation_profile',?,?,?)",params![format!("consolidation_profile_{kind}"),profile.id,actor,at]).map_err(err)?;
    Ok(profile_hash)
}
pub fn propose(
    tx: &Transaction<'_>,
    manifest: &Manifest,
    profile: &Profile,
    actor: &str,
    at: &str,
) -> Result<Value, Error> {
    if states(tx, manifest)?
        .get(&profile.id)
        .is_some_and(|s| s.status != "revoked")
    {
        return Err(policy::refused());
    }
    let h = append(tx, profile, "proposed", actor, at)?;
    Ok(json!({"profileId":profile.id,"profileHash":h,"status":"proposed"}))
}
pub fn approve(
    tx: &Transaction<'_>,
    state: &State,
    profile_hash: &str,
    actor: &str,
    at: &str,
) -> Result<Value, Error> {
    if state.status != "proposed" || state.profile_hash != profile_hash || state.actor == actor {
        return Err(policy::refused());
    }
    append(tx, &state.profile, "approved", actor, at)?;
    Ok(json!({"profileId":state.profile.id,"profileHash":profile_hash,"status":"approved"}))
}
pub fn revoke(tx: &Transaction<'_>, state: &State, actor: &str, at: &str) -> Result<Value, Error> {
    if state.status != "approved" {
        return Err(policy::refused());
    }
    append(tx, &state.profile, "revoked", actor, at)?;
    Ok(json!({"profileId":state.profile.id,"status":"revoked"}))
}
/// Caller retains NamespaceGuard admission and full current membership authority.
/// Each account provenance hash covers exact native amounts and canonical entry identities.
pub fn ledger(tx: &Transaction<'_>, slug: &str, from: &str, as_of: &str) -> Result<Ledger, Error> {
    crate::journal_post::require_current(tx)?;
    let checked = crate::journal_hash::verify(tx)?;
    if !checked.ok {
        return Err(policy::refused());
    }
    let currency: String = tx
        .query_row("SELECT currency FROM companies WHERE id=1", [], |r| {
            r.get(0)
        })
        .map_err(err)?;
    let currency = currency.trim().to_uppercase();
    if currency.len() != 3 || !currency.bytes().all(|b| b.is_ascii_uppercase()) {
        return Err(policy::refused());
    }
    let mut q = tx
        .prepare("SELECT account_no,type,active FROM accounts ORDER BY account_no")
        .map_err(err)?;
    let accounts = q
        .query_map([], |r| {
            Ok(Account {
                account_no: r.get(0)?,
                account_type: r.get(1)?,
                active: r.get::<_, i64>(2)? == 1,
            })
        })
        .map_err(err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(err)?;
    let mut pnl: BTreeMap<String, (String, Money, Vec<Value>)> = BTreeMap::new();
    let mut cumulative: BTreeMap<String, (String, Money, Vec<Value>)> = BTreeMap::new();
    let mut balance: BTreeMap<String, (String, Money, Vec<Value>)> = BTreeMap::new();
    let mut q=tx.prepare("SELECT a.account_no,a.type,j.id,j.entry_hash,l.id,j.transaction_date,l.debit_amount,l.credit_amount,l.currency FROM journal_lines l JOIN accounts a ON a.id=l.account_id JOIN journal_entries j ON j.id=l.journal_entry_id WHERE j.transaction_date<=? AND j.status IN('posted','reversed') ORDER BY j.id,l.id").map_err(err)?;
    let mut rows = q.query([as_of]).map_err(err)?;
    while let Some(r) = rows.next().map_err(err)? {
        let no: String = r.get(0).map_err(err)?;
        let kind: String = r.get(1).map_err(err)?;
        let date: String = r.get(5).map_err(err)?;
        if r.get::<_, String>(8).map_err(err)?.trim().to_uppercase() != currency {
            return Err(policy::refused());
        }
        let debit = crate::legacy_money(r.get_ref(6).map_err(err)?)?;
        let credit = crate::legacy_money(r.get_ref(7).map_err(err)?)?;
        let amount = debit.subtract(&credit);
        let provenance = json!({"entryId":r.get::<_,i64>(2).map_err(err)?,"entryHash":r.get::<_,String>(3).map_err(err)?,"lineId":r.get::<_,i64>(4).map_err(err)?,"transactionDate":date,"debit":debit,"credit":credit});
        let add = |map: &mut BTreeMap<String, (String, Money, Vec<Value>)>| {
            let row = map
                .entry(no.clone())
                .or_insert_with(|| (kind.clone(), Money::default(), vec![]));
            row.1.add(&amount);
            row.2.push(provenance.clone());
        };
        match kind.as_str() {
            "income" | "expense" => {
                add(&mut cumulative);
                if date.as_str() >= from {
                    add(&mut pnl);
                }
            }
            "asset" | "liability" | "equity" | "vat" => add(&mut balance),
            _ => return Err(policy::refused()),
        }
    }
    let values = |rows: BTreeMap<String, (String, Money, Vec<Value>)>, start: Option<&str>| {
        rows.into_iter().map(|(no,(kind,amount,refs))|AccountValue{source_hash:sha(&json!({"companySlug":slug,"accountNo":no,"from":start,"asOf":as_of,"refs":refs,"integrityScope":"native_exact_consolidation_v1"}).to_string()),account_no:no,account_type:kind,amount}).collect()
    };
    use rusqlite::OptionalExtension;
    let head: Option<String> = tx
        .query_row(
            "SELECT entry_hash FROM journal_entries ORDER BY id DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(err)?;
    Ok(Ledger {
        company_slug: slug.into(),
        currency,
        ledger_head_hash: head,
        entry_count: checked.entries_checked as u64,
        accounts,
        pnl: values(pnl, Some(from)),
        cumulative_result: values(cumulative, None),
        balance: values(balance, None),
    })
}
