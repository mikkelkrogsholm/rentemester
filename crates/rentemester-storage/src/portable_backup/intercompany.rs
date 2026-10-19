//! Authoritative V1 groups and reviewed reciprocal mappings. Historical actors are not grants.
use super::*;
use crate::intercompany as source;
use rentemester_engine::intercompany::{Manifest, Mapping};
use std::collections::BTreeSet;
const FORMAT: &str = "rentemester-native-intercompany/2";
const V1: &str = "rentemester-native-intercompany/1";
const MAX: usize = 24 * 1024 * 1024;
const TABLES: [&str; 6] = [
    "rm_group_manifest_events",
    "rm_intercompany_mapping_events",
    "rm_intercompany_dispositions",
    "rm_intercompany_disposition_events",
    "rm_intercompany_disposition_journal_links",
    "rm_intercompany_disposition_lifecycle_events",
];
const COLS: [&[&str]; 6] = [
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
    &[
        "disposition_id",
        "canonical_payload",
        "payload_hash",
        "created_at",
    ],
    &[
        "id",
        "disposition_id",
        "event_type",
        "payload_hash",
        "canonical_payload",
        "actor",
        "principal_kind",
        "principal_id",
        "previous_hash",
        "event_hash",
        "created_at",
    ],
    &[
        "id",
        "disposition_id",
        "side",
        "company_slug",
        "journal_entry_id",
        "journal_entry_no",
        "journal_entry_hash",
        "ledger_head_hash",
        "linked_at",
        "actor",
        "principal_kind",
        "principal_id",
    ],
    &[
        "id",
        "disposition_id",
        "event_type",
        "payload_hash",
        "canonical_payload",
        "actor",
        "principal_kind",
        "principal_id",
        "previous_hash",
        "event_hash",
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
            "intercompany schema differs from codec v2",
        )?;
    }
    Ok(())
}
fn dump_version(db: &Connection, legacy: bool) -> Result<Vec<u8>, Error> {
    shape(db)?;
    let base = export_tables(db, &TABLES[..2], if legacy { V1 } else { FORMAT })?;
    if legacy {
        return Ok(base);
    }
    let mut codec: Business = serde_json::from_slice(&base).map_err(sql)?;
    for (index, table) in TABLES.iter().enumerate().skip(2) {
        let order = if index == 2 { "disposition_id" } else { "id" };
        let mut query = db
            .prepare(&format!("SELECT * FROM {table} ORDER BY {order}"))
            .map_err(sql)?;
        let mut cursor = query.query([]).map_err(sql)?;
        let mut rows = vec![];
        while let Some(row) = cursor.next().map_err(sql)? {
            ensure(rows.len() < 100000, "disposition codec row bound")?;
            let mut cells = vec![];
            for i in 0..COLS[index].len() {
                cells.push(match row.get_ref(i).map_err(sql)? {
                    ValueRef::Null => Cell::Null,
                    ValueRef::Integer(n) => Cell::Integer(n),
                    ValueRef::Text(v) => Cell::Text(std::str::from_utf8(v).map_err(sql)?.into()),
                    _ => return Err(refused("unsupported disposition codec cell")),
                });
            }
            rows.push(cells);
        }
        codec.tables.insert(
            (*table).into(),
            Table {
                columns: COLS[index].iter().map(|s| (*s).into()).collect(),
                rows,
            },
        );
    }
    let bytes = serde_json::to_vec(&codec).map_err(sql)?;
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
    let mut q = db
        .prepare(
            "SELECT canonical_payload FROM rm_intercompany_dispositions ORDER BY disposition_id",
        )
        .map_err(sql)?;
    for row in q.query_map([], |r| r.get::<_, String>(0)).map_err(sql)? {
        let d: crate::intercompany_dispositions::Disposition =
            serde_json::from_str(&row.map_err(sql)?).map_err(sql)?;
        result.extend([d.left.company_slug, d.right.company_slug]);
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
    crate::intercompany_dispositions::validate_history(tx)?;
    Ok(())
}
pub(super) fn export(tx: &Transaction<'_>) -> Result<Vec<u8>, Error> {
    let bytes = dump_version(tx, false)?;
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
        (codec.format == FORMAT && codec.tables.len() == TABLES.len())
            || (codec.format == V1 && codec.tables.len() == 2),
        "unsupported intercompany codec",
    )?;
    let legacy = codec.format == V1;
    let count = if legacy { 2 } else { TABLES.len() };
    for (name, cols) in TABLES[..count].iter().zip(COLS) {
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
                        Cell::Null => ["previous_hash", "ledger_head_hash"].contains(col),
                        Cell::Integer(n) => {
                            ["id", "journal_entry_id"].contains(col)
                                && *n > 0
                                && *n <= 9_007_199_254_740_991
                        }
                        Cell::Text(_) => !["id", "journal_entry_id"].contains(col),
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
    insert_tables(&tx, &codec, &TABLES[..count])?;
    facts(&tx, &registered.iter().cloned().collect())?;
    ensure(
        dump_version(&tx, legacy)? == bytes,
        "intercompany codec ordering/types changed",
    )?;
    crate::control_migration::validate_current_control(&tx)?;
    tx.commit().map_err(sql)
}
#[cfg(test)]
mod tests;
