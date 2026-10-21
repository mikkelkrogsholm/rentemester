//! Exact optional native capability for DKK compensation on foreign invoices.
//! Historical schema artifacts remain unchanged; every journal guard remains.
use crate::{
    invalid,
    migration::{SchemaObject, schema},
};
use rentemester_engine::Error;
use rusqlite::{Connection, Transaction};
use sha2::{Digest, Sha256};
const GUARD: &str = "invoice_compensation_postings_validate_insert";
const PREFIX: &str = "native_compensation_";
const META: &str = "CREATE TABLE native_compensation_schema(id INTEGER PRIMARY KEY CHECK(id=1),version INTEGER NOT NULL CHECK(version=1),ddl_sha256 TEXT NOT NULL);\nCREATE TRIGGER native_compensation_schema_no_update BEFORE UPDATE ON native_compensation_schema BEGIN SELECT RAISE(ABORT,'native compensation schema identity is immutable'); END;\nCREATE TRIGGER native_compensation_schema_no_delete BEFORE DELETE ON native_compensation_schema BEGIN SELECT RAISE(ABORT,'native compensation schema identity is immutable'); END;";
fn refused() -> Error {
    Error::new(
        "unsupported_schema",
        "native compensation extension1 schema or identity differs",
    )
}
fn guards() -> Result<(SchemaObject, SchemaObject), Error> {
    let old = crate::company_baseline::load_images()?
        .current
        .into_iter()
        .find(|object| object.name == GUARD && object.kind == "trigger")
        .ok_or_else(refused)?;
    let predicate = "         AND UPPER(TRIM(COALESCE(d.currency, 'DKK'))) = 'DKK'\n";
    if old.sql.matches(predicate).count() != 1 {
        return Err(refused());
    }
    let mut new = old.clone();
    // A compensation is always DKK. Its original invoice may have another
    // currency; evidence.currency and every exact DKK journal guard are retained.
    new.sql = old.sql.replacen(predicate, "", 1);
    Ok((old, new))
}
fn identity(sql: &str) -> String {
    format!("{:x}", Sha256::digest(format!("{META}\n{sql}").as_bytes()))
}
fn owned(object: &SchemaObject) -> bool {
    object.name.starts_with(PREFIX) || object.table.starts_with(PREFIX)
}
/// Remove only a complete, known optional extension and restore the exact
/// historical guard for comparison by the canonical base validator.
pub(crate) fn strip_schema(
    db: &Connection,
    actual: &[SchemaObject],
) -> Result<Vec<SchemaObject>, Error> {
    let extension = actual
        .iter()
        .filter(|object| owned(object))
        .cloned()
        .collect::<Vec<_>>();
    if extension.is_empty() {
        return Ok(actual.to_vec());
    }
    let reference = Connection::open_in_memory().map_err(invalid)?;
    reference.execute_batch(META).map_err(invalid)?;
    if extension != schema(&reference)? {
        return Err(refused());
    }
    let (old, new) = guards()?;
    if actual.iter().find(|object| object.name == GUARD) != Some(&new) {
        return Err(refused());
    }
    let rows = db
        .prepare("SELECT id,version,ddl_sha256 FROM native_compensation_schema ORDER BY id")
        .map_err(invalid)?
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    if rows != vec![(1, 1, identity(&new.sql))] {
        return Err(refused());
    }
    Ok(actual
        .iter()
        .filter(|object| !owned(object))
        .map(|object| {
            if object.name == GUARD {
                old.clone()
            } else {
                object.clone()
            }
        })
        .collect())
}
/// Installs transactionally only after admission of the exact supported schema.
/// Missing/partial/foreign DDL is refused, never repaired.
pub(crate) fn install(tx: &Transaction<'_>) -> Result<(), Error> {
    crate::journal_post::require_current(tx)?;
    let actual = schema(tx)?;
    strip_schema(tx, &actual)?;
    if actual.iter().any(owned) {
        return Ok(());
    }
    let (_, new) = guards()?;
    tx.execute_batch(META).map_err(invalid)?;
    tx.execute_batch(&format!("DROP TRIGGER {GUARD};\n{}", new.sql))
        .map_err(invalid)?;
    tx.execute(
        "INSERT INTO native_compensation_schema(id,version,ddl_sha256)VALUES(1,1,?1)",
        [identity(&new.sql)],
    )
    .map_err(invalid)?;
    crate::journal_preview::require_current(tx)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_extension_is_transactional_and_unknown_or_partial_guards_are_refused() {
        let mut control = Connection::open_in_memory().unwrap();
        let mut db = Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic compensation schema")
            .unwrap();
        let before = schema(&db).unwrap();
        {
            let tx = db
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .unwrap();
            install(&tx).unwrap();
            crate::journal_preview::require_current(&tx).unwrap();
            tx.rollback().unwrap();
        }
        assert_eq!(schema(&db).unwrap(), before);
        {
            let tx = db
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .unwrap();
            install(&tx).unwrap();
            install(&tx).unwrap();
            tx.commit().unwrap();
        }
        crate::journal_preview::require_current(&db).unwrap();
        let normalized = strip_schema(&db, &schema(&db).unwrap()).unwrap();
        assert_eq!(normalized, before);
        // Replacing only the old trigger while claiming the new identity cannot pass.
        let (old, _) = guards().unwrap();
        db.execute_batch(&format!("DROP TRIGGER {GUARD};{}", old.sql))
            .unwrap();
        assert!(crate::journal_preview::require_current(&db).is_err());
        assert!(strip_schema(&db, &schema(&db).unwrap()).is_err());
    }
}
