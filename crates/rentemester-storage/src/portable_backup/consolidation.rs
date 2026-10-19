//! Exact reviewed reporting profiles and elimination history; no imported authority.
use super::*;
use rentemester_engine::{consolidation::Profile, intercompany::Manifest};
use std::collections::BTreeSet;
#[path = "consolidation/eliminations.rs"]
mod eliminations;
const FORMAT: &str = "rentemester-native-consolidation/1";
const MAX: usize = 24 * 1024 * 1024;
const TABLES: [&str; 2] = [
    "rm_consolidation_profile_events",
    "rm_consolidation_elimination_events",
];
const COLS: [&[&str]; 2] = [
    &[
        "id",
        "profile_id",
        "event_type",
        "profile_hash",
        "canonical_profile",
        "previous_hash",
        "event_hash",
        "actor",
        "created_at",
    ],
    &[
        "id",
        "elimination_id",
        "event_type",
        "payload_hash",
        "canonical_payload",
        "previous_hash",
        "event_hash",
        "actor",
        "created_at",
    ],
];
fn ensure(ok: bool, why: &str) -> Result<(), Error> {
    if ok { Ok(()) } else { Err(refused(why)) }
}
fn shape(db: &Connection) -> Result<(), Error> {
    for (table, cols) in TABLES.iter().zip(COLS) {
        ensure(
            columns(db, table)? == cols,
            "consolidation schema differs from codec v1",
        )?;
    }
    Ok(())
}
fn dump(db: &Connection) -> Result<Vec<u8>, Error> {
    shape(db)?;
    let bytes = export_tables(db, &TABLES, FORMAT)?;
    ensure(
        bytes.len() <= MAX,
        "consolidation signed section byte bound",
    )?;
    Ok(bytes)
}
fn profiles(db: &Connection) -> Result<Vec<Profile>, Error> {
    let mut q = db
        .prepare("SELECT canonical_profile FROM rm_consolidation_profile_events ORDER BY id")
        .map_err(sql)?;
    q.query_map([], |r| r.get::<_, String>(0))
        .map_err(sql)?
        .map(|r| {
            serde_json::from_str(&r.map_err(sql)?)
                .map_err(|_| refused("consolidation profile evidence malformed"))
        })
        .collect()
}
fn companies(tx: &Transaction<'_>) -> Result<BTreeSet<String>, Error> {
    let mut refs = BTreeSet::new();
    for p in profiles(tx)? {
        refs.extend(p.account_mappings.into_iter().map(|m| m.company_slug));
    }
    refs.extend(eliminations::companies(tx)?);
    Ok(refs)
}
fn facts(tx: &Transaction<'_>, registered: &BTreeSet<String>) -> Result<(), Error> {
    shape(tx)?;
    ensure(
        companies(tx)?.is_subset(registered),
        "consolidation refers to unregistered signed company",
    )?;
    // Group/mapping histories are authenticated before they are used as references.
    super::intercompany::validate_companies(tx, &registered.iter().cloned().collect::<Vec<_>>())?;
    let current = crate::intercompany::current(tx, registered)
        .map_err(|_| refused("consolidation group history refused"))?;
    let mut q = tx
        .prepare("SELECT canonical_manifest FROM rm_group_manifest_events ORDER BY id")
        .map_err(sql)?;
    let manifests = q
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(sql)?
        .map(|r| serde_json::from_str::<Manifest>(&r.map_err(sql)?).map_err(sql))
        .collect::<Result<Vec<_>, _>>()?;
    let historical = profiles(tx)?;
    for p in &historical {
        let canonical = p.canonical()?;
        ensure(
            manifests.iter().any(|m| {
                p.clone()
                    .normalize(m)
                    .and_then(|p| p.canonical())
                    .is_ok_and(|c| c == canonical)
            }),
            "historical consolidation profile lacks canonical group source",
        )?;
    }
    if let Some((manifest, _)) = current {
        crate::consolidation::states(tx, &manifest)
            .map_err(|_| refused("consolidation profile lifecycle or hash chain refused"))?;
    } else {
        ensure(
            historical.is_empty(),
            "consolidation profile lacks group source",
        )?;
    }
    eliminations::validate(tx)?;
    Ok(())
}
pub(super) fn export(tx: &Transaction<'_>) -> Result<Vec<u8>, Error> {
    let bytes = dump(tx)?;
    let mut registered = companies(tx)?;
    // The live host subsequently checks the union against the signed catalog.
    let mut q = tx
        .prepare("SELECT canonical_manifest FROM rm_group_manifest_events ORDER BY id")
        .map_err(sql)?;
    for raw in q.query_map([], |r| r.get::<_, String>(0)).map_err(sql)? {
        let m: Manifest = serde_json::from_str(&raw.map_err(sql)?).map_err(sql)?;
        registered.extend(m.companies());
    }
    facts(tx, &registered)?;
    Ok(bytes)
}
pub fn validate_companies(tx: &Transaction<'_>, registered: &[String]) -> Result<(), Error> {
    facts(tx, &registered.iter().cloned().collect())
}
pub fn initialize(db: &mut Connection, bytes: &[u8], registered: &[String]) -> Result<(), Error> {
    crate::control_migration::validate_current_control(db)?;
    shape(db)?;
    ensure(
        bytes.len() <= MAX,
        "consolidation signed section byte bound",
    )?;
    let codec: Business = serde_json::from_slice(bytes).map_err(sql)?;
    ensure(
        codec.format == FORMAT && codec.tables.len() == TABLES.len(),
        "unsupported consolidation codec",
    )?;
    for (name, cols) in TABLES.iter().zip(COLS) {
        let t = codec
            .tables
            .get(*name)
            .ok_or_else(|| refused("consolidation table missing"))?;
        ensure(
            t.columns == cols && t.rows.len() <= 100000,
            "consolidation table shape differs",
        )?;
        for row in &t.rows {
            ensure(row.len() == cols.len(), "consolidation row shape differs")?;
            for (col, cell) in cols.iter().zip(row) {
                ensure(
                    match cell {
                        Cell::Null => *col == "previous_hash",
                        Cell::Integer(n) => *col == "id" && *n > 0,
                        Cell::Text(_) => *col != "id",
                    },
                    "consolidation cell type differs",
                )?;
            }
        }
    }
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(sql)?;
    for table in TABLES {
        ensure(
            tx.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| {
                r.get::<_, i64>(0)
            })
            .map_err(sql)?
                == 0,
            "fresh consolidation target required",
        )?;
    }
    insert_tables(&tx, &codec, &TABLES)?;
    facts(&tx, &registered.iter().cloned().collect())?;
    ensure(
        dump(&tx)? == bytes,
        "consolidation codec ordering/types changed",
    )?;
    crate::control_migration::validate_current_control(&tx)?;
    tx.commit().map_err(sql)
}
#[cfg(test)]
mod tests;
