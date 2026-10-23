//! Checksummed native disposal extension, migrated only at explicit schema admission.
use crate::{
    invalid,
    migration::{SchemaObject, schema},
};
use rentemester_engine::Error;
use rusqlite::{Connection, Transaction};
use sha2::{Digest, Sha256};
const DDL:&str = "CREATE TABLE native_asset_disposal_schema(id INTEGER PRIMARY KEY CHECK(id=1),version INTEGER NOT NULL CHECK(version=1),ddl_sha256 TEXT NOT NULL);
CREATE TABLE native_asset_disposals(id INTEGER PRIMARY KEY,asset_id INTEGER NOT NULL UNIQUE REFERENCES assets(id),transaction_date TEXT NOT NULL,document_id INTEGER NOT NULL REFERENCES documents(id),reason TEXT NOT NULL,loss_account_no TEXT,cost TEXT NOT NULL,accumulated TEXT NOT NULL,carrying TEXT NOT NULL,plan_hash TEXT NOT NULL,journal_entry_id INTEGER NOT NULL UNIQUE REFERENCES journal_entries(id),receipt_json TEXT NOT NULL);
CREATE TRIGGER native_asset_disposals_no_update BEFORE UPDATE ON native_asset_disposals BEGIN SELECT RAISE(ABORT,'asset disposal is immutable'); END;
CREATE TRIGGER native_asset_disposals_no_delete BEFORE DELETE ON native_asset_disposals BEGIN SELECT RAISE(ABORT,'asset disposal is immutable'); END;
CREATE TRIGGER native_asset_disposal_schema_no_update BEFORE UPDATE ON native_asset_disposal_schema BEGIN SELECT RAISE(ABORT,'schema identity is immutable'); END;
CREATE TRIGGER native_asset_disposal_schema_no_delete BEFORE DELETE ON native_asset_disposal_schema BEGIN SELECT RAISE(ABORT,'schema identity is immutable'); END;";
fn identity() -> String {
    format!("{:x}", Sha256::digest(DDL.as_bytes()))
}
fn refused() -> Error {
    Error::new("unsupported_schema", "native asset disposal schema differs")
}
pub(crate) fn strip_schema(
    db: &Connection,
    actual: &[SchemaObject],
) -> Result<Vec<SchemaObject>, Error> {
    let owned = |o: &&SchemaObject| {
        o.name.starts_with("native_asset_disposal") || o.table.starts_with("native_asset_disposal")
    };
    let extension: Vec<_> = actual.iter().filter(owned).cloned().collect();
    if extension.is_empty() {
        return Ok(actual.to_vec());
    }
    let reference = Connection::open_in_memory().map_err(invalid)?;
    reference.execute_batch(DDL).map_err(invalid)?;
    if extension != schema(&reference)? {
        return Err(refused());
    }
    let rows: Vec<(i64, i64, String)> = db
        .prepare("SELECT id,version,ddl_sha256 FROM native_asset_disposal_schema ORDER BY id")
        .map_err(invalid)?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .map_err(invalid)?
        .collect::<Result<_, _>>()
        .map_err(invalid)?;
    if rows != vec![(1, 1, identity())] {
        return Err(refused());
    }
    Ok(actual.iter().filter(|o| !owned(o)).cloned().collect())
}
pub fn installed(db: &Connection) -> Result<bool, Error> {
    let actual = schema(db)?;
    Ok(strip_schema(db, &actual)?.len() != actual.len())
}
pub(crate) fn install(tx: &Transaction<'_>) -> Result<(), Error> {
    crate::journal_post::require_current(tx)?;
    if installed(tx)? {
        return Ok(());
    }
    tx.execute_batch(DDL).map_err(invalid)?;
    tx.execute(
        "INSERT INTO native_asset_disposal_schema(id,version,ddl_sha256) VALUES(1,1,?1)",
        [identity()],
    )
    .map_err(invalid)?;
    crate::journal_post::require_current(tx)
}
/// Explicit migration on an admitted current ledger; never called by asset requests.
pub fn upgrade(db: &mut Connection) -> Result<(), Error> {
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(invalid)?;
    install(&tx)?;
    tx.commit().map_err(invalid)
}
