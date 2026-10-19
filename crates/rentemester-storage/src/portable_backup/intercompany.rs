//! Authoritative V1 groups and reviewed reciprocal mappings. Historical actors are not grants.
use super::*;
use crate::intercompany as source;
use rentemester_engine::intercompany::{Manifest, Mapping};
use std::collections::BTreeSet;
const FORMAT: &str = "rentemester-native-intercompany/1";
const MAX: usize = 24 * 1024 * 1024;
const TABLES: [&str; 2] = ["rm_group_manifest_events", "rm_intercompany_mapping_events"];
const COLS: [&[&str]; 2] = [
    &[
        "id",
        "manifest_hash",
        "previous_hash",
        "canonical_manifest",
        "actor",
        "created_at",
    ],
    &[
        "id",
        "mapping_id",
        "event_type",
        "mapping_hash",
        "canonical_mapping",
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
            "intercompany schema differs from codec v1",
        )?;
    }
    Ok(())
}
fn dump(db: &Connection) -> Result<Vec<u8>, Error> {
    shape(db)?;
    let bytes = export_tables(db, &TABLES, FORMAT)?;
    ensure(bytes.len() <= MAX, "intercompany signed section byte bound")?;
    Ok(bytes)
}
/// Collect every historical reference, including groups removed from the current manifest.
fn companies(db: &Connection) -> Result<BTreeSet<String>, Error> {
    let mut result = BTreeSet::new();
    let mut q = db
        .prepare("SELECT canonical_manifest FROM rm_group_manifest_events ORDER BY id")
        .map_err(sql)?;
    for row in q.query_map([], |r| r.get::<_, String>(0)).map_err(sql)? {
        let m: Manifest = serde_json::from_str(&row.map_err(sql)?)
            .map_err(|_| refused("intercompany canonical evidence is malformed"))?;
        result.extend(m.companies());
    }
    let mut q = db
        .prepare("SELECT canonical_mapping FROM rm_intercompany_mapping_events ORDER BY id")
        .map_err(sql)?;
    for row in q.query_map([], |r| r.get::<_, String>(0)).map_err(sql)? {
        let m: Mapping = serde_json::from_str(&row.map_err(sql)?)
            .map_err(|_| refused("intercompany canonical evidence is malformed"))?;
        result.extend(m.companies());
    }
    Ok(result)
}
fn facts(tx: &Transaction<'_>, registered: &BTreeSet<String>) -> Result<(), Error> {
    shape(tx)?;
    ensure(
        companies(tx)?.is_subset(registered),
        "intercompany refers to unregistered signed company",
    )?;
    // Reuse the native canonical NFC/UTF16/hash and full lifecycle reader. No replay
    // writes/audit are manufactured and no historical reviewer becomes an authority.
    if let Some((manifest, _)) = source::current(tx, registered)
        .map_err(|_| refused("intercompany manifest chain or canonical source differs"))?
    {
        source::states(tx, &manifest)
            .map_err(|_| refused("intercompany mapping lifecycle or canonical source differs"))?;
    } else {
        ensure(
            tx.query_row(
                "SELECT COUNT(*) FROM rm_intercompany_mapping_events",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map_err(sql)?
                == 0,
            "intercompany mapping lacks group manifest",
        )?;
    }
    Ok(())
}
pub(super) fn export(tx: &Transaction<'_>) -> Result<Vec<u8>, Error> {
    let bytes = dump(tx)?;
    facts(tx, &companies(tx)?)?;
    Ok(bytes)
}
pub fn validate_companies(tx: &Transaction<'_>, registered: &[String]) -> Result<(), Error> {
    facts(tx, &registered.iter().cloned().collect())
}
/// Both business tables commit together only after complete source verification.
/// No credentials, permissions, runtime identities or keys are imported.
pub fn initialize(db: &mut Connection, bytes: &[u8], registered: &[String]) -> Result<(), Error> {
    crate::control_migration::validate_current_control(db)?;
    shape(db)?;
    ensure(bytes.len() <= MAX, "intercompany signed section byte bound")?;
    let codec: Business = serde_json::from_slice(bytes).map_err(sql)?;
    ensure(
        codec.format == FORMAT && codec.tables.len() == TABLES.len(),
        "unsupported intercompany codec",
    )?;
    for (name, cols) in TABLES.iter().zip(COLS) {
        let table = codec
            .tables
            .get(*name)
            .ok_or_else(|| refused("intercompany table missing"))?;
        ensure(
            table.columns == cols && table.rows.len() <= 100000,
            "intercompany table shape differs",
        )?;
        for row in &table.rows {
            ensure(row.len() == cols.len(), "intercompany row shape differs")?;
            for (col, cell) in cols.iter().zip(row) {
                ensure(
                    match cell {
                        Cell::Null => *col == "previous_hash",
                        Cell::Integer(n) => *col == "id" && *n > 0,
                        Cell::Text(_) => *col != "id",
                    },
                    "intercompany cell type differs",
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
            "fresh intercompany target required",
        )?;
    }
    insert_tables(&tx, &codec, &TABLES)?;
    facts(&tx, &registered.iter().cloned().collect())?;
    ensure(
        dump(&tx)? == bytes,
        "intercompany codec ordering/types changed",
    )?;
    crate::control_migration::validate_current_control(&tx)?;
    tx.commit().map_err(sql)
}
#[cfg(test)]
mod tests;
