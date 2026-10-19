//! Exact ownership observations and append-only lifecycle, never imported authority.
use super::*;
use crate::{
    ownership as source,
    parties::{canonical, hash},
};
use rentemester_engine::ownership::{Endpoint, Fact};
use serde_json::{Value, json};
use std::collections::BTreeSet;
const FORMAT: &str = "rentemester-native-ownership/1";
const MAX: usize = 24 * 1024 * 1024;
const TABLES: [&str; 4] = [
    "rm_ownership_source_snapshots",
    "rm_ownership_snapshot_events",
    "rm_ownership_facts",
    "rm_ownership_fact_events",
];
const COLS: [&[&str]; 4] = [
    &[
        "id",
        "snapshot_id",
        "source",
        "observed_at",
        "snapshot_hash",
        "diff_hash",
        "canonical_facts",
        "diff_json",
        "actor",
        "principal_kind",
        "principal_id",
        "created_at",
    ],
    &[
        "id",
        "snapshot_id",
        "event_type",
        "actor",
        "principal_kind",
        "principal_id",
        "created_at",
    ],
    &[
        "id",
        "fact_id",
        "snapshot_id",
        "fact_hash",
        "canonical_fact",
        "owner_kind",
        "owner_id",
        "owned_company_slug",
        "valid_from",
        "valid_to_exclusive",
        "economic_exact_bp",
        "economic_min_bp",
        "economic_max_bp",
        "voting_bp",
        "control_type",
        "share_class",
        "jurisdiction",
        "review_state",
        "created_at",
    ],
    &[
        "id",
        "fact_hash",
        "event_type",
        "effective_to_exclusive",
        "successor_fact_hash",
        "snapshot_id",
        "actor",
        "principal_kind",
        "principal_id",
        "created_at",
    ],
];
fn ensure(ok: bool, why: &str) -> Result<(), Error> {
    if ok { Ok(()) } else { Err(refused(why)) }
}
fn text(s: &str, max: usize) -> Result<(), Error> {
    ensure(
        rentemester_engine::company_knowledge::text(s, max)? == s,
        "noncanonical ownership text",
    )
}
fn stamp(s: &str) -> Result<(), Error> {
    ensure(
        rentemester_engine::company_knowledge::iso(s)? == s,
        "noncanonical ownership timestamp",
    )
}
fn principal(kind: &str, id: &str, actor: &str, at: &str) -> Result<(), Error> {
    ensure(
        ["user", "service_account", "local_operator"].contains(&kind),
        "ownership principal kind differs",
    )?;
    text(id, 160)?;
    text(actor, 160)?;
    stamp(at)
}
fn shape(db: &Connection) -> Result<(), Error> {
    for (name, cols) in TABLES.iter().zip(COLS) {
        ensure(
            columns(db, name)? == cols,
            "ownership schema differs from codec v1",
        )?;
    }
    Ok(())
}
fn dump(db: &Connection) -> Result<Vec<u8>, Error> {
    shape(db)?;
    let bytes = export_tables(db, &TABLES, FORMAT)?;
    ensure(bytes.len() <= MAX, "ownership signed section byte bound")?;
    Ok(bytes)
}
pub(super) fn export(tx: &Transaction<'_>) -> Result<Vec<u8>, Error> {
    let bytes = dump(tx)?;
    facts(tx)?;
    Ok(bytes)
}
pub fn initialize(db: &mut Connection, bytes: &[u8], companies: &[String]) -> Result<(), Error> {
    crate::control_migration::validate_current_control(db)?;
    shape(db)?;
    ensure(bytes.len() <= MAX, "ownership signed section byte bound")?;
    let codec: Business = serde_json::from_slice(bytes).map_err(sql)?;
    ensure(
        codec.format == FORMAT && codec.tables.len() == TABLES.len(),
        "unsupported ownership codec",
    )?;
    for (name, cols) in TABLES.iter().zip(COLS) {
        let table = codec
            .tables
            .get(*name)
            .ok_or_else(|| refused("ownership table missing"))?;
        ensure(
            table.columns == cols && table.rows.len() <= 100000,
            "ownership table shape differs",
        )?;
        for row in &table.rows {
            ensure(row.len() == cols.len(), "ownership row shape differs")?;
            for (col, cell) in cols.iter().zip(row) {
                let nullable = *name == TABLES[2]
                    && [
                        "valid_to_exclusive",
                        "economic_exact_bp",
                        "economic_min_bp",
                        "economic_max_bp",
                        "voting_bp",
                        "share_class",
                    ]
                    .contains(col);
                let integer = *col == "id"
                    || (*name == TABLES[2]
                        && [
                            "economic_exact_bp",
                            "economic_min_bp",
                            "economic_max_bp",
                            "voting_bp",
                        ]
                        .contains(col));
                ensure(
                    match cell {
                        Cell::Null => nullable,
                        Cell::Integer(n) => integer && (*col != "id" || *n > 0),
                        Cell::Text(_) => !integer,
                    },
                    "ownership cell type differs",
                )?;
            }
        }
    }
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(sql)?;
    for name in TABLES {
        ensure(
            tx.query_row(&format!("SELECT COUNT(*) FROM {name}"), [], |r| {
                r.get::<_, i64>(0)
            })
            .map_err(sql)?
                == 0,
            "fresh ownership target required",
        )?;
    }
    insert_tables(&tx, &codec, &TABLES)?;
    facts(&tx)?;
    validate_companies(&tx, companies)?;
    ensure(
        dump(&tx)? == bytes,
        "ownership codec ordering/types changed",
    )?;
    crate::control_migration::validate_current_control(&tx)?;
    tx.commit().map_err(sql)
}
fn fact_refs(tx: &Transaction<'_>, f: &Fact) -> Result<(), Error> {
    if let Endpoint::Party { party_id } = &f.owner {
        ensure(
            crate::parties::exists(tx, party_id)?,
            "ownership party source missing",
        )?;
    }
    // Evidence strings are opaque in the source. When an ID names a corporate
    // record, its normal native full-scope/sensitivity reader remains authoritative.
    // Never infer a required corporate record from a prefix or invent a grant.
    Ok(())
}
pub fn validate_companies(db: &Connection, companies: &[String]) -> Result<(), Error> {
    let known: BTreeSet<_> = companies.iter().collect();
    for (name, col) in [
        (TABLES[0], "canonical_facts"),
        (TABLES[2], "canonical_fact"),
    ] {
        let mut q = db
            .prepare(&format!("SELECT {col} FROM {name}"))
            .map_err(sql)?;
        for row in q.query_map([], |r| r.get::<_, String>(0)).map_err(sql)? {
            let raw = row.map_err(sql)?;
            let facts: Vec<Fact> = if name == TABLES[0] {
                serde_json::from_str(&raw).map_err(sql)?
            } else {
                vec![serde_json::from_str(&raw).map_err(sql)?]
            };
            for f in facts {
                ensure(
                    f.companies().iter().all(|s| known.contains(s)),
                    "ownership refers to unregistered company",
                )?;
            }
        }
    }
    Ok(())
}
struct Observation {
    input: source::Propose,
    actor: String,
    kind: String,
    pid: String,
    state: String,
    scratch_hashes: Option<(String, String)>,
    legacy: bool,
}
/// Rebuild only four business tables in private memory through the existing
/// canonical proposal/apply implementation. Original IDs and provenance remain
/// untouched; replay is validation, not imported authentication or an operation.
fn facts(tx: &Transaction<'_>) -> Result<(), Error> {
    shape(tx)?;
    let mut scratch = Connection::open_in_memory().map_err(sql)?;
    crate::control_migration::initialize_empty(&mut scratch)?;
    let replay = scratch.transaction().map_err(sql)?;
    let mut observations = BTreeMap::<String, Observation>::new();
    let mut q=tx.prepare("SELECT id,snapshot_id,source,observed_at,snapshot_hash,diff_hash,canonical_facts,diff_json,actor,principal_kind,principal_id,created_at FROM rm_ownership_source_snapshots ORDER BY id").map_err(sql)?;
    let mut rows = q.query([]).map_err(sql)?;
    while let Some(r) = rows.next().map_err(sql)? {
        ensure(
            r.get::<_, i64>(0).map_err(sql)? > 0,
            "ownership snapshot id differs",
        )?;
        let id: String = r.get(1).map_err(sql)?;
        text(&id, 80)?;
        let src: String = r.get(2).map_err(sql)?;
        text(&src, 160)?;
        let observed: String = r.get(3).map_err(sql)?;
        stamp(&observed)?;
        let raw: String = r.get(6).map_err(sql)?;
        let fs: Vec<Fact> = serde_json::from_str(&raw).map_err(sql)?;
        ensure(
            raw.len() <= 1048576 && source::normalized(&fs)? == fs && canonical(&json!(fs)) == raw,
            "ownership canonical facts differ",
        )?;
        let hashes: Vec<_> = fs.iter().map(|f| hash(&json!(f))).collect();
        ensure(
            hashes.windows(2).all(|w| w[0] < w[1]),
            "ownership facts order/duplicates differ",
        )?;
        for f in &fs {
            fact_refs(tx, f)?;
        }
        let diff_raw: String = r.get(7).map_err(sql)?;
        let diff: Value = serde_json::from_str(&diff_raw).map_err(sql)?;
        ensure(
            diff_raw.len() <= 1048576
                && canonical(&diff) == diff_raw
                && hash(&diff) == r.get::<_, String>(5).map_err(sql)?
                && hash(&json!({"source":src,"observedAt":observed,"facts":fs}))
                    == r.get::<_, String>(4).map_err(sql)?,
            "ownership snapshot/diff hash differs",
        )?;
        let actor: String = r.get(8).map_err(sql)?;
        let kind: String = r.get(9).map_err(sql)?;
        let pid: String = r.get(10).map_err(sql)?;
        principal(&kind, &pid, &actor, &r.get::<_, String>(11).map_err(sql)?)?;
        // The native read validates the immutable envelope and conflict observations.
        let _ = source::load(tx, &id)?;
        observations.insert(
            id.clone(),
            Observation {
                input: source::Propose {
                    snapshot_id: Some(id),
                    source: src,
                    observed_at: observed,
                    facts: fs,
                },
                actor,
                kind,
                pid,
                state: String::new(),
                scratch_hashes: None,
                legacy: diff["version"] == 1,
            },
        );
    }
    let mut applied = BTreeMap::<String, (String, String, String)>::new();
    let mut q=tx.prepare("SELECT id,snapshot_id,event_type,actor,principal_kind,principal_id,created_at FROM rm_ownership_snapshot_events ORDER BY id").map_err(sql)?;
    let mut rows = q.query([]).map_err(sql)?;
    while let Some(r) = rows.next().map_err(sql)? {
        ensure(
            r.get::<_, i64>(0).map_err(sql)? > 0,
            "ownership event id differs",
        )?;
        let id: String = r.get(1).map_err(sql)?;
        let event: String = r.get(2).map_err(sql)?;
        let actor: String = r.get(3).map_err(sql)?;
        let kind: String = r.get(4).map_err(sql)?;
        let pid: String = r.get(5).map_err(sql)?;
        let at: String = r.get(6).map_err(sql)?;
        principal(&kind, &pid, &actor, &at)?;
        let o = observations
            .get_mut(&id)
            .ok_or_else(|| refused("ownership event snapshot missing"))?;
        match event.as_str() {
            "proposed" => {
                ensure(
                    o.state.is_empty() && o.actor == actor && o.kind == kind && o.pid == pid,
                    "ownership proposal provenance/state differs",
                )?;
                let actual = source::load(tx, &id)?.public;
                let got = if o.legacy {
                    let diff = legacy_diff(&replay, &o.input.facts)?;
                    ensure(
                        diff == actual["diff"],
                        "legacy ownership source-bound diff differs",
                    )?;
                    // Scratch uses a v2-shaped envelope only to reuse native load/state reads.
                    // Signed legacy content, version/hash/provenance are never rewritten.
                    let mut internal = diff.clone();
                    internal.as_object_mut().unwrap().remove("automaticEndings");
                    internal["version"] = json!(2);
                    internal["endings"] = json!([]);
                    internal["note"] =
                        json!("Only an explicit later fact can end an approved legal fact");
                    let sh = actual["snapshotHash"].as_str().unwrap();
                    let dh = hash(&internal);
                    replay.execute("INSERT INTO rm_ownership_source_snapshots(snapshot_id,source,observed_at,snapshot_hash,diff_hash,canonical_facts,diff_json,actor,principal_kind,principal_id,created_at)VALUES(?,?,?,?,?,?,?,?,?,?,?)",params![id,o.input.source,o.input.observed_at,sh,dh,canonical(&json!(o.input.facts)),canonical(&internal),actor,kind,pid,at]).map_err(sql)?;
                    replay.execute("INSERT INTO rm_ownership_snapshot_events(snapshot_id,event_type,actor,principal_kind,principal_id,created_at)VALUES(?,'proposed',?,?,?,?)",params![id,actor,kind,pid,at]).map_err(sql)?;
                    source::load(&replay, &id)?.public
                } else {
                    let got =
                        source::propose(&replay, &o.input, &o.input.facts, &id, &actor, &pid, &at)?;
                    ensure(
                        got["diff"] == actual["diff"]
                            && got["snapshotHash"] == actual["snapshotHash"],
                        "ownership source-bound diff differs",
                    )?;
                    got
                };
                o.scratch_hashes = Some((
                    got["snapshotHash"].as_str().unwrap().into(),
                    got["diffHash"].as_str().unwrap().into(),
                ));
            }
            "approved" | "rejected" => {
                ensure(
                    o.state == "proposed" && (o.legacy || o.kind != kind || o.pid != pid),
                    "ownership review state/independence differs",
                )?;
                replay.execute("INSERT INTO rm_ownership_snapshot_events(snapshot_id,event_type,actor,principal_kind,principal_id,created_at)VALUES(?,?,?,?,?,?)",params![id,event,actor,kind,pid,at]).map_err(sql)?;
            }
            "applied" => {
                ensure(o.state == "approved", "ownership apply state differs")?;
                let (sh, dh) = o
                    .scratch_hashes
                    .as_ref()
                    .ok_or_else(|| refused("ownership proposal missing"))?;
                if o.legacy {
                    legacy_apply(&replay, &id, &at)?;
                    replay.execute("INSERT INTO rm_ownership_snapshot_events(snapshot_id,event_type,actor,principal_kind,principal_id,created_at)VALUES(?,'applied',?,?,?,?)",params![id,actor,kind,pid,at]).map_err(sql)?;
                } else {
                    source::apply(
                        &replay,
                        &source::Apply {
                            snapshot_id: id.clone(),
                            snapshot_hash: sh.clone(),
                            diff_hash: dh.clone(),
                        },
                        &actor,
                        &pid,
                        &at,
                    )?;
                }
                applied.insert(id.clone(), (actor, kind, pid));
            }
            _ => return Err(refused("unsupported ownership event")),
        }
        o.state = event;
    }
    ensure(
        observations.values().all(|o| !o.state.is_empty()),
        "ownership snapshot lacks proposal",
    )?;
    // Semantic replay validates all canonical fact fields, source snapshot links,
    // deterministic fact IDs and ending edges without rewriting original row IDs.
    compare_effects(tx, &replay, &applied)?;
    source::approved(tx)?;
    Ok(())
}
/// The pre-24 source admitted unchanged hashes and same-principal review.
/// Reuse canonical hashes and integrity, but do not retroactively give that
/// profile implicit endings or reject its INSERT OR IGNORE exact fact replay.
fn legacy_diff(tx: &Transaction<'_>, facts: &[Fact]) -> Result<Value, Error> {
    let known = source::approved(tx)?;
    let incoming: Vec<_> = facts.iter().map(|f| (hash(&json!(f)), f.clone())).collect();
    let add: Vec<_> = incoming
        .iter()
        .filter(|(h, _)| !known.iter().any(|(old, _)| old == h))
        .cloned()
        .collect();
    ensure(
        rentemester_engine::ownership::integrity(&known, &add)?.is_empty(),
        "legacy ownership cannot implicitly end facts",
    )?;
    let unchanged: Vec<_> = incoming
        .iter()
        .filter(|(h, _)| known.iter().any(|(old, _)| old == h))
        .map(|(h, _)| h.clone())
        .collect();
    let mut conflicts = vec![];
    for (h, _) in &known {
        let raw: String = tx
            .query_row(
                "SELECT canonical_fact FROM rm_ownership_facts WHERE fact_hash=?",
                [h],
                |r| r.get(0),
            )
            .map_err(sql)?;
        let old: Fact = serde_json::from_str(&raw).map_err(sql)?;
        if !incoming.iter().any(|(n, _)| n == h)
            && facts
                .iter()
                .any(|n| n.owner == old.owner && n.owned_company_slug == old.owned_company_slug)
        {
            conflicts.push(old);
        }
    }
    Ok(
        json!({"version":1,"add":add.into_iter().map(|(_,f)|f).collect::<Vec<_>>(),"unchanged":unchanged,"conflicts":conflicts,"automaticEndings":[],"note":"Registry observations never automatically end approved legal facts"}),
    )
}
fn legacy_apply(tx: &Transaction<'_>, id: &str, at: &str) -> Result<(), Error> {
    let public = source::load(tx, id)?.public;
    let additions: Vec<Fact> =
        serde_json::from_value(public["diff"]["add"].clone()).map_err(sql)?;
    let known = source::approved(tx)?;
    let incoming: Vec<_> = additions
        .into_iter()
        .map(|f| (hash(&json!(f)), f))
        .filter(|(h, _)| !known.iter().any(|(old, _)| old == h))
        .collect();
    ensure(
        rentemester_engine::ownership::integrity(&known, &incoming)?.is_empty(),
        "legacy apply cannot implicitly end facts",
    )?;
    for (h, f) in incoming {
        let (owner_kind, owner_id) = match &f.owner {
            Endpoint::Company { company_slug } => ("company", company_slug),
            Endpoint::Party { party_id } => ("party", party_id),
        };
        tx.execute("INSERT INTO rm_ownership_facts(fact_id,snapshot_id,fact_hash,canonical_fact,owner_kind,owner_id,owned_company_slug,valid_from,valid_to_exclusive,economic_exact_bp,economic_min_bp,economic_max_bp,voting_bp,control_type,share_class,jurisdiction,review_state,created_at)VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,'approved',?)",params![format!("ownership-fact-{}",&h[..32]),id,h,canonical(&json!(f)),owner_kind,owner_id,f.owned_company_slug,f.valid_from,f.valid_to_exclusive,f.economic_basis_points,f.economic_interval_basis_points.as_ref().map(|v|v.min),f.economic_interval_basis_points.as_ref().map(|v|v.max),f.voting_basis_points,f.control_type,f.share_class,f.jurisdiction,at]).map_err(sql)?;
    }
    Ok(())
}
fn compare_effects(
    tx: &Transaction<'_>,
    replay: &Transaction<'_>,
    applied: &BTreeMap<String, (String, String, String)>,
) -> Result<(), Error> {
    for (name, cols) in [(TABLES[2], COLS[2]), (TABLES[3], COLS[3])] {
        let selected: Vec<_> = cols
            .iter()
            .filter(|c| {
                ![
                    "id",
                    "created_at",
                    "actor",
                    "principal_kind",
                    "principal_id",
                ]
                .contains(c)
            })
            .copied()
            .collect();
        let read = |db: &Connection| -> Result<Vec<Vec<rusqlite::types::Value>>, Error> {
            let mut q = db
                .prepare(&format!(
                    "SELECT {} FROM {name} ORDER BY id",
                    selected.join(",")
                ))
                .map_err(sql)?;
            q.query_map([], |r| (0..selected.len()).map(|i| r.get(i)).collect())
                .map_err(sql)?
                .collect::<Result<_, _>>()
                .map_err(sql)
        };
        ensure(
            read(tx)? == read(replay)?,
            "ownership fact/ending mirror differs from source apply",
        )?;
        let mut q = tx
            .prepare(&format!("SELECT * FROM {name} ORDER BY id"))
            .map_err(sql)?;
        let mut rows = q.query([]).map_err(sql)?;
        while let Some(r) = rows.next().map_err(sql)? {
            ensure(
                r.get::<_, i64>(0).map_err(sql)? > 0,
                "ownership effect id differs",
            )?;
            stamp(&r.get::<_, String>(cols.len() - 1).map_err(sql)?)?;
            if name == TABLES[3] {
                let id: String = r.get(5).map_err(sql)?;
                let actor: String = r.get(6).map_err(sql)?;
                let kind: String = r.get(7).map_err(sql)?;
                let pid: String = r.get(8).map_err(sql)?;
                ensure(
                    applied.get(&id) == Some(&(actor, kind, pid)),
                    "ownership ending apply provenance differs",
                )?;
            }
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests;
