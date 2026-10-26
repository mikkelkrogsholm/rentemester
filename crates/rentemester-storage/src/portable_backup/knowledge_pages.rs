//! Complete page/relation history, independent of live access, in a closed business section.
use super::*;
const FORMAT: &str = "rentemester-native-knowledge-pages/1";
const TABLES: [&str; 2] = ["rm_knowledge_page_events", "rm_knowledge_relation_events"];
const COLUMNS: [&[&str]; 2] = [
    &[
        "id",
        "page_id",
        "version",
        "event_type",
        "scope_kind",
        "company_slug",
        "slug",
        "title",
        "body_markdown",
        "provenance_kind",
        "provenance_ref",
        "effective_from",
        "effective_to_exclusive",
        "prior_event_hash",
        "event_hash",
        "actor",
        "principal",
        "created_at",
    ],
    &[
        "id",
        "relation_id",
        "version",
        "event_type",
        "scope_kind",
        "company_slug",
        "relation_type",
        "subject_kind",
        "subject_ref",
        "object_kind",
        "object_ref",
        "provenance_kind",
        "provenance_ref",
        "effective_from",
        "effective_to_exclusive",
        "prior_event_hash",
        "event_hash",
        "actor",
        "principal",
        "created_at",
    ],
];
fn shape(db: &Connection) -> Result<(), Error> {
    for (table, columns_) in TABLES.iter().zip(COLUMNS) {
        if columns(db, table)? != columns_ {
            return Err(refused("knowledge pages schema differs"));
        }
    }
    Ok(())
}
pub(super) fn export(db: &Connection) -> Result<Vec<u8>, Error> {
    shape(db)?;
    crate::knowledge_pages::validate(db, None)?;
    export_tables(db, &TABLES, FORMAT)
}
pub fn validate_companies(db: &Connection, companies: &[String]) -> Result<(), Error> {
    shape(db)?;
    crate::knowledge_pages::validate(db, Some(companies))
}
pub fn initialize(db: &mut Connection, bytes: &[u8], companies: &[String]) -> Result<(), Error> {
    crate::control_migration::validate_current_control(db)?;
    shape(db)?;
    if bytes.len() > 32 * 1024 * 1024 {
        return Err(refused("knowledge pages byte bound"));
    }
    let b: Business = serde_json::from_slice(bytes).map_err(sql)?;
    if b.format != FORMAT || b.tables.len() != TABLES.len() {
        return Err(refused("knowledge pages codec differs"));
    }
    for (table, cols) in TABLES.iter().zip(COLUMNS) {
        let t = b
            .tables
            .get(*table)
            .ok_or_else(|| refused("knowledge pages table missing"))?;
        if t.columns != cols || t.rows.len() > 100000 {
            return Err(refused("knowledge pages columns/row bound"));
        }
        for row in &t.rows {
            if row.len() != cols.len() {
                return Err(refused("knowledge pages row differs"));
            }
            for (col, c) in cols.iter().zip(row) {
                let valid = match (*col, c) {
                    ("id" | "version", Cell::Integer(n)) => *n > 0,
                    (
                        "company_slug" | "effective_to_exclusive" | "prior_event_hash",
                        Cell::Null,
                    ) => true,
                    ("id" | "version", _) => false,
                    (_, Cell::Text(_)) => true,
                    _ => false,
                };
                if !valid {
                    return Err(refused("knowledge pages cell differs"));
                }
            }
        }
    }
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(sql)?;
    for table in TABLES {
        if tx
            .query_row::<i64, _, _>(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .map_err(sql)?
            != 0
        {
            return Err(refused("fresh knowledge pages required"));
        }
    }
    insert_tables(&tx, &b, &TABLES)?;
    validate_companies(&tx, companies)?;
    tx.commit().map_err(sql)
}
#[cfg(test)]
mod tests;
