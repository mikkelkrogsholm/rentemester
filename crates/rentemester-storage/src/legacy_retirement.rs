//! Explicit offline source retirement. SQLite fences do not drain network calls:
//! the operator must stop and drain ALL legacy producers before invoking this.
//! Conversion may remove only this exact overlay, and only on an owned copy.
use rentemester_engine::Error;
use rusqlite::{Connection, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
const TABLE: &str = "rm_legacy_retirement_v1";
const PREFIX: &str = "rm_legacy_retired_";
const SQL: &str = "CREATE TABLE rm_legacy_retirement_v1 (id INTEGER PRIMARY KEY CHECK(id=1), operation_id TEXT NOT NULL, source_workspace_id TEXT NOT NULL, operator_actor TEXT NOT NULL, source_stopped_confirmed INTEGER NOT NULL CHECK(source_stopped_confirmed=1))";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetirementBinding {
    pub operation_id: String,
    pub source_workspace_id: String,
    pub operator_actor: String,
    pub source_stopped_confirmed: bool,
}
fn refused(message: impl Into<String>) -> Error {
    Error::new("legacy_retirement_refused", message)
}
fn sql(_: impl std::fmt::Display) -> Error {
    refused("source retirement storage refused")
}
fn hex64(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
impl RetirementBinding {
    pub fn validate(&self) -> Result<(), Error> {
        if !hex64(&self.operation_id)
            || !hex64(&self.source_workspace_id)
            || self.operator_actor.len() > 256
            || self
                .operator_actor
                .chars()
                .any(|c| c.is_control() || c.is_whitespace())
            || !["user:", "agent:", "system:"].iter().any(|p| {
                self.operator_actor
                    .strip_prefix(p)
                    .is_some_and(|s| !s.is_empty() && s.trim() == s)
            })
            || !self.source_stopped_confirmed
        {
            return Err(refused(
                "canonical operation/workspace hashes, actor and explicit stopped-and-drained source confirmation required",
            ));
        }
        Ok(())
    }
}
fn tables(db: &Connection) -> Result<Vec<String>, Error> {
    let mut q = db.prepare("SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").map_err(sql)?;
    let result = q
        .query_map([], |r| r.get(0))
        .map_err(sql)?
        .collect::<Result<Vec<String>, _>>()
        .map_err(sql)?;
    if result.len() > 1024 {
        return Err(refused("source table inventory exceeds limit"));
    }
    Ok(result)
}
fn triggers(tables: &[String]) -> BTreeMap<String, String> {
    let mut result = BTreeMap::new();
    for table in tables {
        for action in ["INSERT", "UPDATE", "DELETE"] {
            let digest = format!(
                "{:x}",
                Sha256::digest(format!("{table}\0{action}").as_bytes())
            );
            let name = format!("{PREFIX}{digest}");
            result.insert(name.clone(), format!("CREATE TRIGGER {} BEFORE {action} ON {} BEGIN SELECT RAISE(ABORT, 'legacy source retired for Rust adoption'); END", crate::migration::quote(&name), crate::migration::quote(table)));
        }
    }
    result
}
fn overlay_objects(db: &Connection) -> Result<BTreeMap<String, (String, String)>, Error> {
    let mut q=db.prepare("SELECT name,type,sql FROM sqlite_schema WHERE name=?1 OR substr(name,1,?2)=?3 ORDER BY name").map_err(sql)?;
    q.query_map(params![TABLE, PREFIX.len() as i64, PREFIX], |r| {
        Ok((r.get(0)?, (r.get(1)?, r.get(2)?)))
    })
    .map_err(sql)?
    .collect::<Result<_, _>>()
    .map_err(sql)
}
/// Verify the entire overlay without changing a connection. Absence is not a
/// retirement proof; the adoption orchestrator must require Some on every DB.
pub fn verify_overlay(db: &Connection) -> Result<Option<RetirementBinding>, Error> {
    let actual = overlay_objects(db)?;
    if actual.is_empty() {
        return Ok(None);
    }
    let expected = triggers(&tables(db)?);
    if actual.get(TABLE) != Some(&("table".into(), SQL.into()))
        || actual.len() != expected.len() + 1
    {
        return Err(refused("retirement overlay is incomplete or altered"));
    }
    for (name, definition) in expected {
        if actual.get(&name) != Some(&("trigger".into(), definition)) {
            return Err(refused("retirement trigger differs"));
        }
    }
    let mut q=db.prepare("SELECT operation_id,source_workspace_id,operator_actor,source_stopped_confirmed FROM rm_legacy_retirement_v1 ORDER BY id").map_err(sql)?;
    let bindings = q
        .query_map([], |r| {
            Ok(RetirementBinding {
                operation_id: r.get(0)?,
                source_workspace_id: r.get(1)?,
                operator_actor: r.get(2)?,
                source_stopped_confirmed: r.get::<_, i64>(3)? == 1,
            })
        })
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    if bindings.len() != 1 {
        return Err(refused("exactly one retirement binding required"));
    }
    let binding = bindings
        .into_iter()
        .next()
        .ok_or_else(|| refused("binding unavailable"))?;
    binding.validate()?;
    Ok(Some(binding))
}
/// Caller owns the admitted source connection and operator authority and has
/// stopped/drained legacy server, CLI, MCP, jobs and external producers. Holds
/// the SQLite write lock until ALL fences commit together. This is not a
/// cross-database transaction; activation requires every source DB to verify.
pub fn retire_owned(db: &mut Connection, binding: &RetirementBinding) -> Result<bool, Error> {
    retire_inner(db, binding, None)
}
/// Bind the reviewed source under the acquired SQLite writer lock. A writer
/// that committed while retirement waited cannot make a stale capture current.
pub fn retire_owned_checked(
    db: &mut Connection,
    binding: &RetirementBinding,
    expected_digest: &str,
) -> Result<bool, Error> {
    if !hex64(expected_digest) {
        return Err(refused("reviewed source logical digest required"));
    }
    retire_inner(db, binding, Some(expected_digest))
}
/// Complete logical source fingerprint, including exact schema and typed cell
/// multiplicities/provenance. Exclude ONLY a fully verified retirement overlay.
/// It is stable across SQLite checkpoint/lock bookkeeping, not across data changes.
pub fn source_digest(db: &Connection) -> Result<String, Error> {
    let retired = verify_overlay(db)?.is_some();
    let mut schema = crate::migration::schema(db)?;
    if retired {
        schema.retain(|o| o.name != TABLE && !o.name.starts_with(PREFIX));
    }
    let names = schema
        .iter()
        .filter(|o| o.kind == "table")
        .map(|o| o.name.clone())
        .collect::<Vec<_>>();
    let columns = crate::migration::projection(db, &names)?;
    let cells = crate::migration::project_digest(db, &columns)?;
    let bytes = serde_json::to_vec(&(schema, cells)).map_err(sql)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
fn retire_inner(
    db: &mut Connection,
    binding: &RetirementBinding,
    expected: Option<&str>,
) -> Result<bool, Error> {
    binding.validate()?;
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(sql)?;
    if let Some(expected) = expected
        && source_digest(&tx)? != expected
    {
        return Err(refused("reviewed source changed before retirement"));
    }
    if let Some(prior) = verify_overlay(&tx)? {
        if &prior != binding {
            return Err(refused("source already belongs to another retirement"));
        }
        tx.commit().map_err(sql)?;
        return Ok(false);
    }
    let before = tables(&tx)?;
    if before
        .iter()
        .any(|name| name.starts_with("rm_legacy_retire"))
    {
        return Err(refused("reserved retirement namespace already exists"));
    }
    tx.execute_batch(SQL).map_err(sql)?;
    tx.execute(
        "INSERT INTO rm_legacy_retirement_v1 VALUES(1,?1,?2,?3,1)",
        params![
            binding.operation_id,
            binding.source_workspace_id,
            binding.operator_actor
        ],
    )
    .map_err(sql)?;
    for definition in triggers(&tables(&tx)?).values() {
        tx.execute_batch(definition).map_err(sql)?;
    }
    if verify_overlay(&tx)?.as_ref() != Some(binding) {
        return Err(refused("retirement verification differs"));
    }
    tx.commit().map_err(sql)?;
    Ok(true)
}
/// Remove exact known retirement objects ONLY from a captured private image.
/// No public source-path helper exposes this operation and no writer authority
/// is conferred. The source's durable retirement must remain intact.
pub(crate) fn strip_verified_overlay(
    db: &mut Connection,
) -> Result<Option<RetirementBinding>, Error> {
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(sql)?;
    let Some(binding) = verify_overlay(&tx)? else {
        tx.commit().map_err(sql)?;
        return Ok(None);
    };
    for name in triggers(&tables(&tx)?).keys() {
        tx.execute_batch(&format!("DROP TRIGGER {}", crate::migration::quote(name)))
            .map_err(sql)?;
    }
    tx.execute_batch(&format!("DROP TABLE {TABLE}"))
        .map_err(sql)?;
    tx.commit().map_err(sql)?;
    Ok(Some(binding))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn binding() -> RetirementBinding {
        RetirementBinding {
            operation_id: "a".repeat(64),
            source_workspace_id: "b".repeat(64),
            operator_actor: "agent:synthetic-adoption".into(),
            source_stopped_confirmed: true,
        }
    }
    #[test]
    fn retirement_fences_existing_handles_and_private_strip_preserves_business() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("legacy.sqlite");
        let mut source = Connection::open(&path).unwrap();
        source.execute_batch("CREATE TABLE business(id INTEGER PRIMARY KEY,value TEXT);INSERT INTO business VALUES(1,'historical')").unwrap();
        let old = Connection::open(&path).unwrap();
        let b = binding();
        assert!(retire_owned(&mut source, &b).unwrap());
        assert!(!retire_owned(&mut source, &b).unwrap());
        for statement in [
            "INSERT INTO business VALUES(2,'new')",
            "UPDATE business SET value='changed'",
            "DELETE FROM business",
            "DELETE FROM rm_legacy_retirement_v1",
        ] {
            assert!(
                old.execute_batch(statement)
                    .unwrap_err()
                    .to_string()
                    .contains("legacy source retired")
            );
        }
        assert_eq!(
            old.query_row("SELECT value FROM business", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "historical"
        );
        let bytes = std::fs::read(&path).unwrap();
        let private = dir.path().join("private.sqlite");
        std::fs::write(&private, bytes).unwrap();
        let mut copy = Connection::open(private).unwrap();
        assert_eq!(strip_verified_overlay(&mut copy).unwrap(), Some(b.clone()));
        copy.execute("INSERT INTO business VALUES(2,'native')", [])
            .unwrap();
        assert_eq!(verify_overlay(&source).unwrap(), Some(b));
        assert_eq!(
            source
                .query_row("SELECT count(*) FROM business", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
    #[test]
    fn missing_confirmation_other_owner_and_partial_overlay_fail_closed() {
        let mut db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE business(value TEXT)")
            .unwrap();
        let mut b = binding();
        b.source_stopped_confirmed = false;
        assert!(retire_owned(&mut db, &b).is_err());
        assert_eq!(verify_overlay(&db).unwrap(), None);
        b.source_stopped_confirmed = true;
        retire_owned(&mut db, &b).unwrap();
        let mut other = b.clone();
        other.operation_id = "c".repeat(64);
        assert!(retire_owned(&mut db, &other).is_err());
        let name = triggers(&tables(&db).unwrap())
            .keys()
            .next()
            .unwrap()
            .clone();
        db.execute_batch(&format!("DROP TRIGGER {}", crate::migration::quote(&name)))
            .unwrap();
        assert!(verify_overlay(&db).is_err());
        assert!(strip_verified_overlay(&mut db).is_err());
        assert!(retire_owned(&mut db, &b).is_err());
    }
    #[test]
    fn checked_retirement_refuses_a_commit_that_arrived_while_draining() {
        use std::sync::mpsc;
        use std::time::Duration;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("legacy.sqlite");
        let mut source = Connection::open(&path).unwrap();
        source
            .execute_batch(
                "CREATE TABLE business(value TEXT);INSERT INTO business VALUES('reviewed')",
            )
            .unwrap();
        source.busy_timeout(Duration::from_secs(2)).unwrap();
        let reviewed = source_digest(&source).unwrap();
        let mut old = Connection::open(&path).unwrap();
        let tx = old
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        tx.execute("UPDATE business SET value='last legacy commit'", [])
            .unwrap();
        let (started_tx, started_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            started_tx.send(()).unwrap();
            let result = retire_owned_checked(&mut source, &binding(), &reviewed);
            done_tx.send(result).unwrap();
        });
        started_rx.recv().unwrap();
        assert!(matches!(
            done_rx.recv_timeout(Duration::from_millis(80)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        tx.commit().unwrap();
        let error = done_rx
            .recv_timeout(Duration::from_secs(3))
            .unwrap()
            .unwrap_err();
        assert!(error.message.contains("source changed"));
        thread.join().unwrap();
        assert_eq!(verify_overlay(&old).unwrap(), None);
        assert_eq!(
            old.query_row("SELECT value FROM business", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "last legacy commit"
        );
        let current = source_digest(&old).unwrap();
        assert!(retire_owned_checked(&mut old, &binding(), &current).unwrap());
        assert_eq!(source_digest(&old).unwrap(), current);
    }
    #[test]
    fn failed_ddl_rolls_back_whole_database_retirement() {
        let mut db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE VIRTUAL TABLE search USING fts5(text);INSERT INTO search VALUES('old')",
        )
        .unwrap();
        assert!(retire_owned(&mut db, &binding()).is_err());
        assert!(overlay_objects(&db).unwrap().is_empty());
        db.execute("INSERT INTO search VALUES('still active')", [])
            .unwrap();
    }
}
