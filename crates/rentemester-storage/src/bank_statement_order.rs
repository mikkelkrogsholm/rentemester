//! Immutable source chronology beside unchanged legacy bank rows. Signed SQLite
//! images preserve this exact versioned native extension, including all evidence.
use crate::{
    invalid,
    migration::{SchemaObject, schema},
};
use rentemester_engine::Error;
use rusqlite::{Connection, Transaction, params};
const DDL: &str = "CREATE TABLE native_bank_statement_order_v1(transaction_id INTEGER PRIMARY KEY REFERENCES bank_transactions(id),source_row_index INTEGER NOT NULL CHECK(source_row_index>=0),direction TEXT NOT NULL CHECK(direction IN ('ascending','descending')),provenance TEXT NOT NULL CHECK(length(trim(provenance))>0));
CREATE TRIGGER native_bank_statement_order_v1_no_update BEFORE UPDATE ON native_bank_statement_order_v1 BEGIN SELECT RAISE(ABORT,'bank statement source order is immutable'); END;
CREATE TRIGGER native_bank_statement_order_v1_no_delete BEFORE DELETE ON native_bank_statement_order_v1 BEGIN SELECT RAISE(ABORT,'bank statement source order is immutable'); END;";
fn refused() -> Error {
    Error::new(
        "unsupported_schema",
        "native bank statement order schema or source evidence differs",
    )
}
pub(crate) fn strip_schema(
    db: &Connection,
    actual: &[SchemaObject],
) -> Result<Vec<SchemaObject>, Error> {
    let owned = |o: &&SchemaObject| {
        o.name.starts_with("native_bank_statement_order")
            || o.table.starts_with("native_bank_statement_order")
    };
    let found: Vec<_> = actual.iter().filter(owned).cloned().collect();
    if found.is_empty() {
        return Ok(actual.to_vec());
    }
    let reference = Connection::open_in_memory().map_err(invalid)?;
    reference.execute_batch(DDL).map_err(invalid)?;
    if found != schema(&reference)? {
        return Err(refused());
    }
    validate_rows(db)?;
    Ok(actual.iter().filter(|o| !owned(o)).cloned().collect())
}
pub fn installed(db: &Connection) -> Result<bool, Error> {
    let actual = schema(db)?;
    Ok(strip_schema(db, &actual)?.len() != actual.len())
}
pub(crate) fn install(tx: &Transaction<'_>) -> Result<(), Error> {
    crate::journal_post::require_current(tx)?;
    if !installed(tx)? {
        tx.execute_batch(DDL).map_err(invalid)?;
    }
    crate::journal_post::require_current(tx)
}
/// Explicit stopped/operator admission, matching other native company extensions.
/// Ordinary reads/import requests do not migrate old ledgers.
pub fn upgrade(db: &mut Connection) -> Result<(), Error> {
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(invalid)?;
    install(&tx)?;
    tx.commit().map_err(invalid)
}
pub fn direction(
    profile: Option<&str>,
    requested: Option<&str>,
) -> Result<Option<&'static str>, Error> {
    let requested = match requested {
        None => None,
        Some("ascending") => Some("ascending"),
        Some("descending") => Some("descending"),
        _ => {
            return Err(Error::new(
                "invalid_bank_csv",
                "statementOrder must be ascending or descending",
            ));
        }
    };
    // The actual original profile declares newest first. Explicit generic input
    // cannot override a named profile's source contract.
    Ok(
        if profile.is_some_and(|p| p.trim().eq_ignore_ascii_case("danske-bank")) {
            Some("descending")
        } else {
            requested
        },
    )
}
fn provenance(source: &str, profile: Option<&str>, direction: &str) -> String {
    if let Some(profile) = profile {
        format!("sha256:{source};profile:{profile};order:{direction}")
    } else {
        format!("sha256:{source};explicit:bank-import;order:{direction}")
    }
}
pub(crate) fn record(
    tx: &Transaction<'_>,
    id: i64,
    index: usize,
    source: &str,
    profile: Option<&str>,
    direction: &str,
) -> Result<(), Error> {
    tx.execute("INSERT INTO native_bank_statement_order_v1(transaction_id,source_row_index,direction,provenance) VALUES(?1,?2,?3,?4)",params![id,index as i64,direction,provenance(source,profile,direction)]).map_err(invalid)?;
    Ok(())
}
fn validate_rows(db: &Connection) -> Result<(), Error> {
    let mut q=db.prepare("SELECT o.transaction_id,o.source_row_index,o.direction,o.provenance,b.source_file_hash FROM native_bank_statement_order_v1 o LEFT JOIN bank_transactions b ON b.id=o.transaction_id ORDER BY o.transaction_id").map_err(invalid)?;
    let mut rows = q.query([]).map_err(invalid)?;
    let mut indices = std::collections::BTreeSet::new();
    while let Some(r) = rows.next().map_err(invalid)? {
        let id: i64 = r.get(0).map_err(invalid)?;
        let index: i64 = r.get(1).map_err(invalid)?;
        let dir: String = r.get(2).map_err(invalid)?;
        let proof: String = r.get(3).map_err(invalid)?;
        let source: Option<String> = r.get(4).map_err(invalid)?;
        let source = source.ok_or_else(refused)?;
        if id <= 0
            || index < 0
            || !matches!(dir.as_str(), "ascending" | "descending")
            || source.len() != 64
            || !source
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || !(proof == provenance(&source, None, &dir)
                || dir == "descending" && proof == provenance(&source, Some("danske-bank"), &dir))
            || !indices.insert((
                proof,
                index,
                db.query_row(
                    "SELECT COALESCE(bank_account_id,0) FROM bank_transactions WHERE id=?1",
                    [id],
                    |r| r.get::<_, i64>(0),
                )
                .map_err(invalid)?,
            ))
        {
            return Err(refused());
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn pair() -> (Connection, Connection) {
        let (mut c, mut d) = (
            Connection::open_in_memory().unwrap(),
            Connection::open_in_memory().unwrap(),
        );
        crate::fresh::initialize_empty_pair(&mut c, &mut d, "Synthetic bank order").unwrap();
        (c, d)
    }
    fn import(tx: &Transaction<'_>, csv: &str, order: Option<&str>) -> serde_json::Value {
        let rows = rentemester_engine::bank::parse(csv).unwrap();
        crate::bank::import_ordered_bytes_with_principal(
            tx,
            csv.as_bytes(),
            &rows,
            None,
            "synthetic",
            "2026-10-26T12:00:00Z",
            "synthetic-original",
            crate::idempotency::PrincipalKind::User,
            false,
            order,
        )
        .unwrap()
    }
    #[test]
    fn declared_direction_zero_net_cycle_reimport_and_reopen_preserve_evidence() {
        for (order, body) in [
            (
                "descending",
                "2026-10-24,final,-100,DKK,100\n2026-10-24,middle,100,DKK,200\n2026-10-24,opening,1,DKK,100",
            ),
            (
                "ascending",
                "2026-10-24,opening,1,DKK,100\n2026-10-24,middle,100,DKK,200\n2026-10-24,final,-100,DKK,100",
            ),
        ] {
            let (_c, mut d) = pair();
            let tx = d
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .unwrap();
            let csv = format!("date,text,amount,currency,balance_after\n{body}\n");
            assert_eq!(import(&tx, &csv, Some(order))["imported"], 3);
            let balance = crate::bank::statement_balance(&tx, "2026-10-24").unwrap();
            assert_eq!(balance.status, "known");
            assert_eq!(balance.balance.unwrap().decimal(), "100.00");
            let original: Vec<(i64, i64, String, String)> = tx
                .prepare("SELECT * FROM native_bank_statement_order_v1 ORDER BY transaction_id")
                .unwrap()
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap();
            assert_eq!(
                import(
                    &tx,
                    &csv,
                    Some(if order == "ascending" {
                        "descending"
                    } else {
                        "ascending"
                    })
                )["imported"],
                0
            );
            assert_eq!(
                tx.query_row("SELECT count(*) FROM bank_transactions", [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                3
            );
            assert!(
                tx.execute(
                    "UPDATE native_bank_statement_order_v1 SET direction='ascending'",
                    []
                )
                .is_err()
            );
            assert!(
                tx.execute("DELETE FROM native_bank_statement_order_v1", [])
                    .is_err()
            );
            let again: Vec<(i64, i64, String, String)> = tx
                .prepare("SELECT * FROM native_bank_statement_order_v1 ORDER BY transaction_id")
                .unwrap()
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap();
            assert_eq!(original, again);
            tx.commit().unwrap();
            let tx = d
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .unwrap();
            assert_eq!(
                crate::bank::statement_balance(&tx, "2026-10-24")
                    .unwrap()
                    .balance
                    .unwrap()
                    .decimal(),
                "100.00"
            );
            crate::portable_backup::company(&tx).unwrap();
        }
    }
    #[test]
    fn missing_colliding_inconsistent_and_partial_streams_are_unavailable() {
        for (first, second, order) in [
            ("2026-10-24,a,1,DKK,100\n2026-10-24,b,1,DKK,101", None, None),
            (
                "2026-10-24,a,1,DKK,100",
                Some("2026-10-24,b,1,DKK,101"),
                Some("ascending"),
            ),
            (
                "2026-10-24,a,1,DKK,100\n2026-10-24,b,1,DKK,102",
                None,
                Some("ascending"),
            ),
            (
                "2026-10-24,a,1,DKK,100\n2026-10-25,b,1,DKK,102",
                None,
                Some("ascending"),
            ),
            (
                "2026-10-24,a,1,DKK,100\n2026-10-25,b,1,DKK,",
                None,
                Some("ascending"),
            ),
        ] {
            let (_c, mut d) = pair();
            let tx = d
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .unwrap();
            let header = "date,text,amount,currency,balance_after\n";
            import(&tx, &format!("{header}{first}\n"), order);
            if let Some(second) = second {
                import(&tx, &format!("{header}{second}\n"), order);
            }
            let result = crate::bank::statement_balance(&tx, "2026-10-26").unwrap();
            assert_eq!(result.status, "unavailable");
            assert!(result.balance.is_none());
        }
        assert!(direction(None, Some("newest")).is_err());
        assert_eq!(
            direction(Some(" DANSKE-BANK "), Some("ascending")).unwrap(),
            Some("descending")
        );
    }
    #[test]
    fn corrupt_provenance_or_orphan_refuses_complete_backup() {
        let (_c, mut d) = pair();
        let tx = d
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        import(
            &tx,
            "date,text,amount,currency,balance_after\n2026-10-24,a,1,DKK,100\n",
            Some("ascending"),
        );
        tx.execute_batch("DROP TRIGGER native_bank_statement_order_v1_no_update; UPDATE native_bank_statement_order_v1 SET provenance='forged';").unwrap();
        assert!(crate::portable_backup::company(&tx).is_err());
    }
    #[test]
    fn exact_extension_and_refusals_preserve_legacy_history() {
        let (mut c, mut d) = (
            Connection::open_in_memory().unwrap(),
            Connection::open_in_memory().unwrap(),
        );
        crate::fresh::initialize_empty_pair(&mut c, &mut d, "Synthetic bank order").unwrap();
        assert!(installed(&d).unwrap());
        let before: String = d
            .query_row(
                "SELECT group_concat(checksum) FROM schema_migrations",
                [],
                |r| r.get(0),
            )
            .unwrap();
        upgrade(&mut d).unwrap();
        let after: String = d
            .query_row(
                "SELECT group_concat(checksum) FROM schema_migrations",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(before, after);
        d.execute_batch("DROP TRIGGER native_bank_statement_order_v1_no_update")
            .unwrap();
        assert!(installed(&d).is_err());
        assert!(crate::journal_preview::require_current(&d).is_err());
        assert!(crate::portable_backup::company(&d).is_err());
    }
}
