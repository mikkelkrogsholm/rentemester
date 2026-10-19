//! Canonical control evidence and exact per-company sources. No legal-entity writes.
use rentemester_engine::{
    Error,
    intercompany::{Manifest, Mapping, Position, refused},
    money::Money,
};
use rusqlite::{Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
fn err(_: impl std::fmt::Display) -> Error {
    refused()
}
pub fn sha(s: &str) -> String {
    format!("{:x}", Sha256::digest(s.as_bytes()))
}
pub fn current(
    tx: &Transaction<'_>,
    registered: &BTreeSet<String>,
) -> Result<Option<(Manifest, String)>, Error> {
    let mut q=tx.prepare("SELECT id,manifest_hash,previous_hash,canonical_manifest,actor,created_at FROM main.rm_group_manifest_events ORDER BY id").map_err(err)?;
    let rows = q
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
            ))
        })
        .map_err(err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(err)?;
    let mut prev: Option<String> = None;
    let mut last = None;
    for (index, (id, hash, p, canonical, actor, at)) in rows.into_iter().enumerate() {
        if id != index as i64 + 1
            || p != prev
            || sha(&format!(
                "{}\n{id}\n{actor}\n{at}\n{canonical}",
                p.as_deref().unwrap_or("GENESIS")
            )) != hash
        {
            return Err(refused());
        }
        let m: Manifest = serde_json::from_str(&canonical).map_err(err)?;
        let normalized = m.normalize(registered)?;
        if normalized.canonical()? != canonical {
            return Err(refused());
        }
        prev = Some(hash.clone());
        last = Some((normalized, hash));
    }
    Ok(last)
}
fn audit(tx: &Transaction<'_>, kind: &str, id: &str, actor: &str, at: &str) -> Result<(), Error> {
    tx.execute("INSERT INTO main.workspace_audit(event_type,entity_type,entity_id,actor,created_at) VALUES(?,'intercompany',?,?,?)",params![kind,id,actor,at]).map_err(err)?;
    Ok(())
}
pub fn apply_manifest(
    tx: &Transaction<'_>,
    m: &Manifest,
    old: Option<&str>,
    actor: &str,
    at: &str,
) -> Result<Value, Error> {
    let canonical = m.canonical()?;
    let previous:Option<(i64,String,String)>=tx.query_row("SELECT id,manifest_hash,canonical_manifest FROM main.rm_group_manifest_events ORDER BY id DESC LIMIT 1",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(err)?;
    if let Some((_, h, c)) = &previous
        && c == &canonical
    {
        return Ok(json!({"status":"unchanged","manifestHash":h}));
    }
    if previous.as_ref().map(|v| v.1.as_str()) != old {
        return Err(refused());
    }
    let id = previous.map_or(1, |v| v.0 + 1);
    let hash = sha(&format!(
        "{}\n{id}\n{actor}\n{at}\n{canonical}",
        old.unwrap_or("GENESIS")
    ));
    tx.execute("INSERT INTO main.rm_group_manifest_events(id,manifest_hash,previous_hash,canonical_manifest,actor,created_at) VALUES(?,?,?,?,?,?)",params![id,hash,old,canonical,actor,at]).map_err(err)?;
    audit(tx, "group_manifest_applied", &hash, actor, at)?;
    Ok(json!({"status":"applied","manifestHash":hash}))
}
use rusqlite::OptionalExtension;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub mapping: Mapping,
    pub mapping_hash: String,
    pub status: String,
    pub actor: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Event<'a> {
    previous_hash: Option<&'a str>,
    id: i64,
    mapping_id: &'a str,
    event_type: &'a str,
    mapping_hash: &'a str,
    canonical_mapping: &'a str,
    actor: &'a str,
    created_at: &'a str,
}
fn event_hash(e: &Event<'_>) -> Result<String, Error> {
    Ok(sha(&serde_json::to_string(e).map_err(err)?))
}
pub fn states(tx: &Transaction<'_>, manifest: &Manifest) -> Result<BTreeMap<String, State>, Error> {
    let mut q=tx.prepare("SELECT id,mapping_id,event_type,mapping_hash,canonical_mapping,previous_hash,event_hash,actor,created_at FROM main.rm_intercompany_mapping_events ORDER BY id").map_err(err)?;
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
    let mut prev: Option<String> = None;
    let mut states: BTreeMap<String, State> = BTreeMap::new();
    for (i, (id, mid, kind, hash, canonical, p, eh, actor, at)) in rows.into_iter().enumerate() {
        if id != i as i64 + 1
            || p != prev
            || event_hash(&Event {
                previous_hash: p.as_deref(),
                id,
                mapping_id: &mid,
                event_type: &kind,
                mapping_hash: &hash,
                canonical_mapping: &canonical,
                actor: &actor,
                created_at: &at,
            })? != eh
            || sha(&canonical) != hash
        {
            return Err(refused());
        }
        let mapping: Mapping = serde_json::from_str(&canonical).map_err(err)?;
        let mapping = mapping.normalize(manifest)?;
        if mapping.id != mid || mapping.canonical()? != canonical {
            return Err(refused());
        }
        let old = states.get(&mid);
        let valid = match kind.as_str() {
            "proposed" => old.is_none_or(|s| s.status == "revoked"),
            "approved" => old.is_some_and(|s| {
                s.status == "proposed" && s.mapping_hash == hash && s.actor != actor
            }),
            "revoked" => old.is_some_and(|s| s.status == "approved" && s.mapping_hash == hash),
            _ => false,
        };
        if !valid {
            return Err(refused());
        }
        if kind == "approved"
            && states
                .values()
                .any(|s| s.status == "approved" && mapping.overlaps(&s.mapping))
        {
            return Err(refused());
        }
        states.insert(
            mid,
            State {
                mapping,
                mapping_hash: hash,
                status: kind,
                actor,
            },
        );
        prev = Some(eh);
    }
    Ok(states)
}
pub fn append(
    tx: &Transaction<'_>,
    states: &BTreeMap<String, State>,
    m: &Mapping,
    kind: &str,
    actor: &str,
    at: &str,
) -> Result<Value, Error> {
    let old = states.get(&m.id);
    let canonical = m.canonical()?;
    let hash = sha(&canonical);
    match kind {
        "proposed" if old.is_none_or(|s| s.status == "revoked") => {}
        "approved"
            if old.is_some_and(|s| {
                s.status == "proposed" && s.mapping_hash == hash && s.actor != actor
            }) && !states
                .values()
                .any(|s| s.status == "approved" && m.overlaps(&s.mapping)) => {}
        "revoked" if old.is_some_and(|s| s.status == "approved" && s.mapping_hash == hash) => {}
        _ => return Err(refused()),
    }
    let previous:Option<(i64,String)>=tx.query_row("SELECT id,event_hash FROM main.rm_intercompany_mapping_events ORDER BY id DESC LIMIT 1",[],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(err)?;
    let id = previous.as_ref().map_or(1, |v| v.0 + 1);
    let p = previous.as_ref().map(|v| v.1.as_str());
    let eh = event_hash(&Event {
        previous_hash: p,
        id,
        mapping_id: &m.id,
        event_type: kind,
        mapping_hash: &hash,
        canonical_mapping: &canonical,
        actor,
        created_at: at,
    })?;
    tx.execute("INSERT INTO main.rm_intercompany_mapping_events(id,mapping_id,event_type,mapping_hash,canonical_mapping,previous_hash,event_hash,actor,created_at) VALUES(?,?,?,?,?,?,?,?,?)",params![id,m.id,kind,hash,canonical,p,eh,actor,at]).map_err(err)?;
    audit(
        tx,
        &format!("intercompany_mapping_{kind}"),
        &m.id,
        actor,
        at,
    )?;
    Ok(json!({"mappingId":m.id,"mappingHash":hash,"status":kind}))
}
/// Caller holds admitted company descriptor and current all-company authorization.
pub fn side(
    tx: &Transaction<'_>,
    slug: &str,
    accounts: &[String],
    position: &Position,
    as_of: &str,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    let verified = crate::journal_hash::verify(tx)?;
    if !verified.ok {
        return Err(refused());
    }
    let currency: String = tx
        .query_row("SELECT currency FROM companies WHERE id=1", [], |r| {
            r.get(0)
        })
        .map_err(err)?;
    if currency.trim().is_empty() {
        return Err(refused());
    }
    for a in accounts {
        let active: bool = tx
            .query_row(
                "SELECT active=1 FROM accounts WHERE account_no=?",
                [a],
                |r| r.get(0),
            )
            .map_err(err)?;
        if !active {
            return Err(refused());
        }
    }
    let mut q=tx.prepare("SELECT j.id,j.entry_no,l.id,a.account_no,j.transaction_date,l.debit_amount,l.credit_amount,l.currency FROM journal_lines l JOIN accounts a ON a.id=l.account_id JOIN journal_entries j ON j.id=l.journal_entry_id WHERE j.transaction_date<=? AND j.status IN('posted','reversed') ORDER BY j.id,l.id").map_err(err)?;
    let mut rows = q.query([as_of]).map_err(err)?;
    let mut balance = Money::default();
    let mut refs = vec![];
    while let Some(r) = rows.next().map_err(err)? {
        let a: String = r.get(3).map_err(err)?;
        if !accounts.contains(&a) {
            continue;
        }
        if r.get::<_, String>(7).map_err(err)?.trim().to_uppercase()
            != currency.trim().to_uppercase()
        {
            return Err(refused());
        }
        let debit = crate::legacy_money(r.get_ref(5).map_err(err)?)?;
        let credit = crate::legacy_money(r.get_ref(6).map_err(err)?)?;
        let amount = if *position == Position::Receivable {
            debit.subtract(&credit)
        } else {
            credit.subtract(&debit)
        };
        balance.add(&amount);
        refs.push(json!({"entryId":r.get::<_,i64>(0).map_err(err)?,"entryNo":r.get::<_,String>(1).map_err(err)?,"lineId":r.get::<_,i64>(2).map_err(err)?,"accountNo":a,"transactionDate":r.get::<_,String>(4).map_err(err)?,"amount":amount.decimal()}));
    }
    let head: Option<String> = tx
        .query_row(
            "SELECT entry_hash FROM journal_entries ORDER BY id DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(err)?;
    let selected = json!({"companySlug":slug,"asOf":as_of,"accountNos":accounts,"position":position,"sourceRefs":refs});
    Ok(
        json!({"companySlug":slug,"currency":currency.trim().to_uppercase(),"position":position,"balance":balance.decimal(),"accountNos":accounts,"sourceRefs":refs,"sourceSnapshot":{"ledgerHeadHash":head,"entryCount":verified.entries_checked,"selectionHash":sha(&selected.to_string()),"integrityScope":"native_journal_hash_chain_and_selected_canonical_lines"}}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    fn manifest() -> Manifest {
        serde_json::from_value(json!({"version":1,"groups":[{"id":"synthetic-group","name":"Synthetic group","memberships":[{"id":"a-member","companySlug":"a","validFrom":"2026-01-01"},{"id":"b-member","companySlug":"b","validFrom":"2026-01-01"}],"ownership":[]}]})).unwrap()
    }
    fn mapping(id: &str) -> Mapping {
        serde_json::from_value(json!({"id":id,"groupId":"synthetic-group","leftCompanySlug":"a","rightCompanySlug":"b","leftAccountNos":["1400"],"rightAccountNos":["2400"],"leftPosition":"receivable","rightPosition":"payable","evidenceRefs":["synthetic-contract"],"validFrom":"2026-01-01","validToExclusive":"2027-01-01"})).unwrap()
    }
    fn db() -> rusqlite::Connection {
        let mut c = rusqlite::Connection::open_in_memory().unwrap();
        let mut d = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut c, &mut d, "Synthetic").unwrap();
        c
    }
    #[test]
    fn source_typescript_unicode_canonical_hashes_match() {
        // Golden values produced by the actual TS parse/canonicalize functions.
        let raw = json!({"version":1,"groups":[{"id":"synthetic","name":" Cafe\u{301} ","memberships":[{"id":"a-member","companySlug":"a","validFrom":"2026-01-01"},{"id":"b-member","companySlug":"b","validFrom":"2026-01-01"}],"ownership":[]}]});
        let manifest: Manifest = serde_json::from_value(raw).unwrap();
        let manifest = manifest
            .normalize(&BTreeSet::from(["a".into(), "b".into()]))
            .unwrap();
        assert_eq!(
            sha(&manifest.canonical().unwrap()),
            "149250fd6306067ad37a04a1db7773140a970cb3e379fdf6dbea1fcfb6fb4ea9"
        );
        let raw = json!({"id":"map","groupId":"synthetic","leftCompanySlug":"a","rightCompanySlug":"b","leftAccountNos":["1400"],"rightAccountNos":["2400"],"leftPosition":"receivable","rightPosition":"payable","evidenceRefs":["\u{e000}","😀"," Cafe\u{301} "],"validFrom":"2026-01-01"});
        let map: Mapping = serde_json::from_value(raw).unwrap();
        assert_eq!(
            sha(&map.normalize(&manifest).unwrap().canonical().unwrap()),
            "5eeec6d0fab160649a618164973bdd27ba140091228f3bcfabc73d42b96f6a5a"
        );
    }
    #[test]
    fn lifecycle_hashes_four_eyes_overlap_revoke_and_corruption() {
        let mut c = db();
        let tx = c.transaction().unwrap();
        let reg = BTreeSet::from(["a".into(), "b".into()]);
        let m = manifest().normalize(&reg).unwrap();
        assert!(current(&tx, &reg).unwrap().is_none());
        apply_manifest(&tx, &m, None, "user:maker", "2026-01-01T00:00:00.000Z").unwrap();
        let (read, _) = current(&tx, &reg).unwrap().unwrap();
        assert_eq!(read.canonical().unwrap(), m.canonical().unwrap());
        let map = mapping("test-map").normalize(&m).unwrap();
        append(
            &tx,
            &states(&tx, &m).unwrap(),
            &map,
            "proposed",
            "user:maker",
            "2026-01-01T00:00:00.000Z",
        )
        .unwrap();
        assert!(
            append(
                &tx,
                &states(&tx, &m).unwrap(),
                &map,
                "approved",
                "user:maker",
                "2026-01-01T00:00:00.000Z"
            )
            .is_err()
        );
        append(
            &tx,
            &states(&tx, &m).unwrap(),
            &map,
            "approved",
            "user:reviewer",
            "2026-01-01T00:00:00.000Z",
        )
        .unwrap();
        let b = mapping("other-map").normalize(&m).unwrap();
        append(
            &tx,
            &states(&tx, &m).unwrap(),
            &b,
            "proposed",
            "user:maker",
            "2026-01-01T00:00:00.000Z",
        )
        .unwrap();
        assert!(
            append(
                &tx,
                &states(&tx, &m).unwrap(),
                &b,
                "approved",
                "user:reviewer",
                "2026-01-01T00:00:00.000Z"
            )
            .is_err()
        );
        append(
            &tx,
            &states(&tx, &m).unwrap(),
            &map,
            "revoked",
            "user:reviewer",
            "2026-01-01T00:00:00.000Z",
        )
        .unwrap();
        append(
            &tx,
            &states(&tx, &m).unwrap(),
            &b,
            "approved",
            "user:reviewer",
            "2026-01-01T00:00:00.000Z",
        )
        .unwrap();
        assert_eq!(states(&tx, &m).unwrap()["test-map"].status, "revoked");
        tx.execute_batch("DROP TRIGGER rm_intercompany_mapping_events_no_update;UPDATE rm_intercompany_mapping_events SET actor='user:tampered' WHERE id=1").unwrap();
        assert!(states(&tx, &m).is_err());
    }
    #[test]
    fn manifest_coverage_cycles_unknown_fields_and_half_open_dates() {
        let reg = BTreeSet::from(["a".into(), "b".into()]);
        let m = manifest().normalize(&reg).unwrap();
        let map = mapping("map").normalize(&m).unwrap();
        assert!(map.interval.active("2026-12-31"));
        assert!(!map.interval.active("2027-01-01"));
        let mut bad = mapping("map");
        bad.interval.valid_from = "2025-01-01".into();
        assert!(bad.normalize(&m).is_err());
        let mut raw = serde_json::to_value(&m).unwrap();
        raw["groups"][0]["ownership"] = json!([{"id":"ab","parentCompanySlug":"a","childCompanySlug":"b","basisPoints":10000,"evidenceRefs":["source"],"validFrom":"2026-01-01"},{"id":"ba","parentCompanySlug":"b","childCompanySlug":"a","basisPoints":10000,"evidenceRefs":["source"],"validFrom":"2026-01-01"}]);
        assert!(
            serde_json::from_value::<Manifest>(raw)
                .unwrap()
                .normalize(&reg)
                .is_err()
        );
        let mut raw = serde_json::to_value(&map).unwrap();
        raw["actor"] = json!("forged");
        assert!(serde_json::from_value::<Mapping>(raw).is_err());
    }
    #[test]
    fn selected_actuals_are_exact_asof_hash_bound_and_do_not_write() {
        let mut control = rusqlite::Connection::open_in_memory().unwrap();
        let mut ledger = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut ledger, "Synthetic").unwrap();
        ledger.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('1400','Synthetic receivable','asset','debit'),('8000','Synthetic equity','equity','credit')").unwrap();
        let tx = ledger
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let request=serde_json::from_value(json!({"transactionDate":"2026-01-02","text":"Synthetic reciprocal source","currency":"DKK","lines":[{"accountNo":"1400","debitAmount":"12.34"},{"accountNo":"8000","creditAmount":"12.34"}]})).unwrap();
        crate::journal_post::post(
            &tx,
            &request,
            &crate::journal_preview::manual_journal_rule_version().unwrap(),
            rentemester_engine::fiscal_year::Date::parse("2026-02-01").unwrap(),
            crate::journal_post::Actor {
                subject: "user:synthetic",
                program: "test",
            },
        )
        .unwrap();
        let before: i64 = tx
            .query_row("SELECT count(*) FROM journal_entries", [], |r| r.get(0))
            .unwrap();
        let a = side(
            &tx,
            "a",
            &["1400".into()],
            &Position::Receivable,
            "2026-01-02",
        )
        .unwrap();
        assert_eq!(a["balance"], "12.34");
        assert_eq!(a["sourceRefs"].as_array().unwrap().len(), 1);
        assert_eq!(
            side(
                &tx,
                "a",
                &["1400".into()],
                &Position::Receivable,
                "2026-01-01"
            )
            .unwrap()["balance"],
            "0.00"
        );
        assert!(
            side(
                &tx,
                "a",
                &["missing".into()],
                &Position::Receivable,
                "2026-01-02"
            )
            .is_err()
        );
        assert_eq!(
            tx.query_row("SELECT count(*) FROM journal_entries", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            before
        );
        tx.execute_batch("DROP TRIGGER journal_entries_no_update;UPDATE journal_entries SET entry_hash='tampered'").unwrap();
        assert!(
            side(
                &tx,
                "a",
                &["1400".into()],
                &Position::Receivable,
                "2026-01-02"
            )
            .is_err()
        );
    }
}
