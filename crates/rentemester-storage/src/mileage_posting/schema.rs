use super::hash;
use crate::{
    invalid,
    migration::{SchemaObject, schema},
};
use rentemester_engine::Error;
use rusqlite::{Connection, Transaction};
const DDL: &str = "CREATE TABLE native_mileage_posting_schema(id INTEGER PRIMARY KEY CHECK(id=1),version INTEGER NOT NULL CHECK(version=1),ddl_sha256 TEXT NOT NULL);
CREATE TABLE native_mileage_posting_versions(version TEXT PRIMARY KEY,payload TEXT NOT NULL,payload_hash TEXT NOT NULL);
CREATE TABLE native_mileage_posting_receipts(mileage_entry_id INTEGER PRIMARY KEY REFERENCES mileage_entries(id),operation_key TEXT NOT NULL UNIQUE,principal TEXT NOT NULL,intent TEXT NOT NULL,payload TEXT NOT NULL,payload_hash TEXT NOT NULL,journal_entry_id INTEGER NOT NULL UNIQUE REFERENCES journal_entries(id),journal_hash TEXT NOT NULL);
CREATE TRIGGER native_mileage_posting_schema_no_update BEFORE UPDATE ON native_mileage_posting_schema BEGIN SELECT RAISE(ABORT,'immutable mileage schema'); END;
CREATE TRIGGER native_mileage_posting_schema_no_delete BEFORE DELETE ON native_mileage_posting_schema BEGIN SELECT RAISE(ABORT,'immutable mileage schema'); END;
CREATE TRIGGER native_mileage_posting_versions_no_update BEFORE UPDATE ON native_mileage_posting_versions BEGIN SELECT RAISE(ABORT,'immutable mileage version'); END;
CREATE TRIGGER native_mileage_posting_versions_no_delete BEFORE DELETE ON native_mileage_posting_versions BEGIN SELECT RAISE(ABORT,'immutable mileage version'); END;
CREATE TRIGGER native_mileage_posting_receipts_no_update BEFORE UPDATE ON native_mileage_posting_receipts BEGIN SELECT RAISE(ABORT,'immutable mileage receipt'); END;
CREATE TRIGGER native_mileage_posting_receipts_no_delete BEFORE DELETE ON native_mileage_posting_receipts BEGIN SELECT RAISE(ABORT,'immutable mileage receipt'); END;";
fn error() -> Error {
    Error::new(
        "unsupported_schema",
        "Native mileage posting schema/history differs",
    )
}
fn owned(o: &SchemaObject) -> bool {
    o.name.starts_with("native_mileage_posting_") || o.table.starts_with("native_mileage_posting_")
}
pub(crate) fn strip_schema(
    db: &Connection,
    actual: &[SchemaObject],
) -> Result<Vec<SchemaObject>, Error> {
    let extension: Vec<_> = actual.iter().filter(|o| owned(o)).cloned().collect();
    if extension.is_empty() {
        return Ok(actual.to_vec());
    }
    let reference = Connection::open_in_memory().map_err(invalid)?;
    reference.execute_batch(DDL).map_err(invalid)?;
    if extension != schema(&reference)? {
        return Err(error());
    }
    let identity: Vec<(i64, i64, String)> = db
        .prepare("SELECT id,version,ddl_sha256 FROM native_mileage_posting_schema ORDER BY id")
        .map_err(invalid)?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .map_err(invalid)?
        .collect::<Result<_, _>>()
        .map_err(invalid)?;
    if identity != vec![(1, 1, hash(DDL))] {
        return Err(error());
    }
    for table in [
        "native_mileage_posting_versions",
        "native_mileage_posting_receipts",
    ] {
        let mut q = db
            .prepare(&format!("SELECT payload,payload_hash FROM {table}"))
            .map_err(invalid)?;
        for row in q
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(invalid)?
        {
            let (payload, digest) = row.map_err(invalid)?;
            if hash(&payload) != digest
                || serde_json::from_str::<serde_json::Value>(&payload).is_err()
            {
                return Err(error());
            }
        }
    }
    let broken:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM native_mileage_posting_receipts r LEFT JOIN journal_entries j ON j.id=r.journal_entry_id WHERE j.id IS NULL OR j.entry_hash!=r.journal_hash)",[],|r|r.get(0)).map_err(invalid)?;
    if broken {
        return Err(super::refused(
            "Mileage journal missing, changed or reversed; receipt is not an effective posting",
        ));
    }
    Ok(actual.iter().filter(|o| !owned(o)).cloned().collect())
}
pub fn installed(db: &Connection) -> Result<bool, Error> {
    let actual = schema(db)?;
    strip_schema(db, &actual)?;
    Ok(actual.iter().any(owned))
}
pub(super) fn install(tx: &Transaction<'_>) -> Result<(), Error> {
    crate::journal_post::require_current(tx)?;
    if installed(tx)? {
        return Ok(());
    }
    tx.execute_batch(DDL).map_err(invalid)?;
    tx.execute(
        "INSERT INTO native_mileage_posting_schema VALUES(1,1,?1)",
        [hash(DDL)],
    )
    .map_err(invalid)?;
    Ok(())
}
