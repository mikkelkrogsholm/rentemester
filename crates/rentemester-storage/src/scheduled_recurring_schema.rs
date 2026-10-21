//! Exact native extension1 over unchanged legacy36; never repairs partial/foreign schema.
use crate::{
    invalid,
    migration::{SchemaObject, schema},
};
use rentemester_engine::Error;
use rusqlite::{Connection, Transaction};
use sha2::{Digest, Sha256};
const DDL: &str = "CREATE TABLE native_scheduled_recurring_schema(id INTEGER PRIMARY KEY CHECK(id=1),version INTEGER NOT NULL CHECK(version=1),ddl_sha256 TEXT NOT NULL);\nCREATE TABLE native_scheduled_recurring_events(id INTEGER PRIMARY KEY,template_id INTEGER NOT NULL REFERENCES recurring_invoice_templates(id),kind TEXT NOT NULL CHECK(kind IN ('enabled','paused','resumed','cancelled','queued','skipped','attempted','succeeded','failed')),template_hash TEXT NOT NULL,actor TEXT NOT NULL,canonical_payload TEXT NOT NULL,previous_hash TEXT NOT NULL,event_hash TEXT NOT NULL UNIQUE,created_at TEXT NOT NULL);\nCREATE TRIGGER native_scheduled_events_no_update BEFORE UPDATE ON native_scheduled_recurring_events BEGIN SELECT RAISE(ABORT,'scheduled evidence is append-only'); END;\nCREATE TRIGGER native_scheduled_events_no_delete BEFORE DELETE ON native_scheduled_recurring_events BEGIN SELECT RAISE(ABORT,'scheduled evidence is append-only'); END;\nCREATE TRIGGER native_scheduled_schema_no_update BEFORE UPDATE ON native_scheduled_recurring_schema BEGIN SELECT RAISE(ABORT,'native schema identity is immutable'); END;\nCREATE TRIGGER native_scheduled_schema_no_delete BEFORE DELETE ON native_scheduled_recurring_schema BEGIN SELECT RAISE(ABORT,'native schema identity is immutable'); END;";
pub fn identity() -> String {
    format!("{:x}", Sha256::digest(DDL.as_bytes()))
}
fn error() -> Error {
    Error::new(
        "unsupported_schema",
        "native scheduled recurring extension1 physical schema/history differs",
    )
}
/// Exact base OR exact base+known extension. Unknown extra objects always refuse.
pub(crate) fn validate(db: &Connection, base: &[SchemaObject]) -> Result<bool, Error> {
    let actual = crate::native_fx_roles::strip_schema(db, &schema(db)?)?;
    let actual = crate::invoice_compensation_schema::strip_schema(db, &actual)?;
    if actual == base {
        return Ok(false);
    }
    let reference = Connection::open_in_memory().map_err(invalid)?;
    reference.execute_batch(DDL).map_err(invalid)?;
    let mut expected = base.to_vec();
    expected.extend(schema(&reference)?);
    expected.sort_by(|a, b| a.name.cmp(&b.name));
    if actual != expected {
        return Err(error());
    }
    let rows: Vec<(i64, i64, String)> = db
        .prepare("SELECT id,version,ddl_sha256 FROM native_scheduled_recurring_schema ORDER BY id")
        .map_err(invalid)?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .map_err(invalid)?
        .collect::<Result<_, _>>()
        .map_err(invalid)?;
    if rows != vec![(1, 1, identity())] {
        return Err(error());
    }
    Ok(true)
}
pub fn installed(db: &Connection) -> Result<bool, Error> {
    validate(db, &crate::company_baseline::load_images()?.current)
}
/// Caller must have current authority and an admitted owned write transaction. Rollback undoes ALL DDL.
pub(crate) fn install(tx: &Transaction<'_>) -> Result<(), Error> {
    crate::journal_post::require_current(tx)?;
    if installed(tx)? {
        return Ok(());
    }
    tx.execute_batch(DDL).map_err(invalid)?;
    tx.execute(
        "INSERT INTO native_scheduled_recurring_schema(id,version,ddl_sha256) VALUES(1,1,?1)",
        [identity()],
    )
    .map_err(invalid)?;
    if !installed(tx)? {
        return Err(error());
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extension_exact_identity_rollback_and_partial_refusal() {
        let mut c = Connection::open_in_memory().unwrap();
        let mut d = Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut c, &mut d, "Synthetic queue").unwrap();
        let before = schema(&d).unwrap();
        {
            let tx = d
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .unwrap();
            install(&tx).unwrap();
            assert!(installed(&tx).unwrap());
            tx.rollback().unwrap();
        }
        assert_eq!(schema(&d).unwrap(), before);
        let tx = d
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        install(&tx).unwrap();
        tx.commit().unwrap();
        assert!(installed(&d).unwrap());
        crate::journal_preview::require_current(&d).unwrap();
        d.execute_batch("DROP TRIGGER native_scheduled_schema_no_update; UPDATE native_scheduled_recurring_schema SET ddl_sha256='corrupt';").unwrap();
        assert!(installed(&d).is_err());
        assert!(crate::journal_preview::require_current(&d).is_err());
    }
}
