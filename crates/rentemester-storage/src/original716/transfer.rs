//! Source-profile storage admission. Unknown/nonempty source business fails closed.
use rentemester_engine::Error;
use rusqlite::{Connection, MAIN_DB, Transaction};
use serde_json::{Value, json};
fn err() -> Error {
    crate::portable_backup::refused(
        "source snapshot business/schema unsupported; explicit versioned migration required",
    )
}
/// The source format cannot encode native-only business. Refuse rather than omit it.
pub fn require_source_export_coverage(tx: &Transaction<'_>) -> Result<(), Error> {
    let b = crate::portable_backup::export_control(tx)?;
    for bytes in [
        b.knowledge_bytes,
        b.parties_bytes,
        b.inbox_bytes,
        b.corporate_bytes,
        b.ownership_bytes,
        b.intercompany_bytes,
        b.consolidation_bytes,
    ] {
        let v: Value = serde_json::from_slice(&bytes).map_err(|_| err())?;
        if v["tables"].as_object().is_none_or(|tables| {
            tables
                .values()
                .any(|t| t["rows"].as_array().is_none_or(|r| !r.is_empty()))
        }) {
            return Err(err());
        }
    }
    Ok(())
}
pub fn access_plan(tx: &Transaction<'_>) -> Result<Value, Error> {
    let mut q = tx
        .prepare("SELECT id,name,email FROM user ORDER BY id")
        .map_err(|_| err())?;
    let users = q
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(|_| err())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| err())?;
    let mut out = vec![];
    for (id, name, email) in users {
        let Some(role) = crate::workspace_members::access(tx, &id)? else {
            continue;
        };
        let mut memberships = vec![];
        // Latest membership decisions, not credentials, are advisory re-invite data.
        let mut q = tx.prepare("SELECT DISTINCT company_slug FROM rm_company_membership_events ORDER BY company_slug").map_err(|_|err())?;
        let slugs = q
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|_| err())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| err())?;
        for slug in slugs {
            if let Some(role) = crate::workspace_members::membership(tx, &id, &slug)? {
                memberships.push(json!({"companySlug":slug,"role":role}));
            }
        }
        out.push(json!({"name":name,"email":email,"workspaceRole":role,"memberships":memberships}));
    }
    Ok(json!({"version":1,"recovery":"bootstrap-owner-then-reinvite","users":out}))
}
/// Validate opaque SQLite bytes privately. Legacy layouts must go through an
/// explicit conversion, never a writable open of the source or silent migration.
pub fn validate_ledger(bytes: &[u8]) -> Result<crate::portable_backup::Company, Error> {
    let mut db = Connection::open_in_memory().map_err(|_| err())?;
    db.deserialize_read_exact(MAIN_DB, bytes, bytes.len(), true)
        .map_err(|_| err())?;
    db.execute_batch("PRAGMA trusted_schema=OFF;PRAGMA foreign_keys=ON;PRAGMA query_only=ON;")
        .map_err(|_| err())?;
    let facts = crate::portable_backup::company(&db)?;
    db.close().map_err(|_| err())?;
    Ok(facts)
}
pub fn initialize_source_control(db: &mut Connection, hash: &str) -> Result<(), Error> {
    crate::portable_backup::initialize_source_control(db, hash)
}

/// Manifest row counts must describe the actual independently validated ledger.
pub fn validate_stats(bytes: &[u8], manifest: &[u8]) -> Result<(), Error> {
    let v: Value = serde_json::from_slice(manifest).map_err(|_| err())?;
    let mut db = Connection::open_in_memory().map_err(|_| err())?;
    db.deserialize_read_exact(MAIN_DB, bytes, bytes.len(), true)
        .map_err(|_| err())?;
    db.execute_batch("PRAGMA trusted_schema=OFF;PRAGMA query_only=ON")
        .map_err(|_| err())?;
    for (table, key) in [
        ("journal_entries", "journalEntries"),
        ("documents", "documents"),
        ("bank_transactions", "bankTransactions"),
    ] {
        let count: u64 = db
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .map_err(|_| err())?;
        if v["ledgerStats"][key].as_u64() != Some(count) {
            return Err(err());
        }
    }
    db.close().map_err(|_| err())?;
    Ok(())
}
