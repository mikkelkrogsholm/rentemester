//! Exact additive native FX role confirmations; the legacy role CHECK remains unchanged.
use crate::{
    invalid,
    migration::{SchemaObject, schema},
};
use rentemester_engine::{Error, company_knowledge};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
const SAFE: i64 = 9_007_199_254_740_991;
const PREFIX: &str = "native_fx_role";
const DDL: &str = "CREATE TABLE native_fx_role_schema(id INTEGER PRIMARY KEY CHECK(id=1),version INTEGER NOT NULL CHECK(version=1),ddl_sha256 TEXT NOT NULL);\nCREATE TABLE native_fx_role_confirmations(id INTEGER PRIMARY KEY,role TEXT NOT NULL CHECK(role IN ('fx_gain','fx_loss')),account_id INTEGER NOT NULL REFERENCES accounts(id),account_no TEXT NOT NULL REFERENCES accounts(account_no),version INTEGER NOT NULL CHECK(version>0 AND version<=9007199254740991),confirmed_by TEXT NOT NULL,confirmation_source TEXT NOT NULL CHECK(confirmation_source='explicit'),created_at TEXT NOT NULL DEFAULT(strftime('%Y-%m-%dT%H:%M:%fZ','now')),UNIQUE(role,version));\nCREATE TRIGGER native_fx_role_confirmations_no_update BEFORE UPDATE ON native_fx_role_confirmations BEGIN SELECT RAISE(ABORT,'FX role confirmations are append-only'); END;\nCREATE TRIGGER native_fx_role_confirmations_no_delete BEFORE DELETE ON native_fx_role_confirmations BEGIN SELECT RAISE(ABORT,'FX role confirmations are append-only'); END;\nCREATE TRIGGER native_fx_role_schema_no_update BEFORE UPDATE ON native_fx_role_schema BEGIN SELECT RAISE(ABORT,'native FX role schema identity is immutable'); END;\nCREATE TRIGGER native_fx_role_schema_no_delete BEFORE DELETE ON native_fx_role_schema BEGIN SELECT RAISE(ABORT,'native FX role schema identity is immutable'); END;";
fn refused() -> Error {
    Error::new(
        "unsupported_schema",
        "native FX role extension1 physical schema or confirmation history differs",
    )
}
fn role_kind(role: &str) -> Result<(&'static str, &'static str), Error> {
    match role {
        "fx_gain" => Ok(("income", "credit")),
        "fx_loss" => Ok(("expense", "debit")),
        _ => Err(Error::new(
            "account_setup_refused",
            "unsupported native FX account role",
        )),
    }
}
fn identity() -> String {
    format!("{:x}", Sha256::digest(DDL.as_bytes()))
}
fn text(s: &str, max: usize) -> bool {
    !s.is_empty() && s.len() <= max && s.trim() == s && !s.chars().any(char::is_control)
}
/// Recognize only the exact complete extension; leave all unrelated objects for the base validator.
/// Never invokes the current-schema guard, so it is safe inside that guard.
pub(crate) fn strip_schema(
    db: &Connection,
    actual: &[SchemaObject],
) -> Result<Vec<SchemaObject>, Error> {
    let reference = Connection::open_in_memory().map_err(invalid)?;
    reference.execute_batch(DDL).map_err(invalid)?;
    let expected = schema(&reference)?;
    let is_fx = |o: &SchemaObject| o.name.starts_with(PREFIX) || o.table.starts_with(PREFIX);
    let extension = actual
        .iter()
        .filter(|o| is_fx(o))
        .cloned()
        .collect::<Vec<_>>();
    if extension.is_empty() {
        return Ok(actual.to_vec());
    }
    if extension != expected {
        return Err(refused());
    }
    validate_contents(db)?;
    Ok(actual.iter().filter(|o| !is_fx(o)).cloned().collect())
}
/// Physical DDL, immutable identity and historical references remain part of portable evidence.
/// An account's later inactive/type state cannot erase a once-confirmed mapping's history.
pub(crate) fn validate_history(db: &Connection) -> Result<(), Error> {
    strip_schema(db, &schema(db)?)?;
    Ok(())
}
fn validate_contents(db: &Connection) -> Result<(), Error> {
    let identities = db
        .prepare("SELECT id,version,ddl_sha256 FROM native_fx_role_schema ORDER BY id")
        .map_err(invalid)?
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    if identities != vec![(1, 1, identity())] {
        return Err(refused());
    }
    let mut versions = BTreeMap::<String, i64>::new();
    let mut last_id = 0;
    let mut query=db.prepare("SELECT e.id,e.role,e.account_id,e.account_no,e.version,e.confirmed_by,e.confirmation_source,e.created_at,EXISTS(SELECT 1 FROM accounts a WHERE a.id=e.account_id AND a.account_no=e.account_no) FROM native_fx_role_confirmations e ORDER BY e.id").map_err(invalid)?;
    let mut rows = query.query([]).map_err(invalid)?;
    while let Some(r) = rows.next().map_err(invalid)? {
        let id: i64 = r.get(0).map_err(invalid)?;
        let role: String = r.get(1).map_err(invalid)?;
        let account_id: i64 = r.get(2).map_err(invalid)?;
        let account: String = r.get(3).map_err(invalid)?;
        let version: i64 = r.get(4).map_err(invalid)?;
        let actor: String = r.get(5).map_err(invalid)?;
        let source: String = r.get(6).map_err(invalid)?;
        let at: String = r.get(7).map_err(invalid)?;
        if id != last_id + 1
            || id > SAFE
            || account_id <= 0
            || account_id > SAFE
            || role_kind(&role).is_err()
            || !text(&account, 256)
            || !text(&actor, 512)
            || !actor.starts_with("user:")
            || actor.len() == 5
            || source != "explicit"
            || !r.get::<_, bool>(8).map_err(invalid)?
            || company_knowledge::iso(&at).ok().as_deref() != Some(at.as_str())
        {
            return Err(refused());
        }
        let previous = versions.entry(role).or_default();
        if version != *previous + 1 || version > SAFE {
            return Err(refused());
        }
        *previous = version;
        last_id = id;
    }
    Ok(())
}
pub(crate) fn resolve(
    tx: &Transaction<'_>,
    role: &str,
) -> Result<Option<(String, i64, String, String)>, Error> {
    role_kind(role)?;
    let objects = schema(tx)?;
    let stripped = strip_schema(tx, &objects)?;
    if stripped.len() == objects.len() {
        return Ok(None);
    }
    let row:Option<(String,i64,String,String)>=tx.query_row("SELECT account_no,version,confirmed_by,confirmation_source FROM native_fx_role_confirmations WHERE role=?1 ORDER BY version DESC LIMIT 1",[role],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(invalid)?;
    if let Some((account, _, _, _)) = &row {
        compatible(tx, role, account)?;
    }
    Ok(row)
}
fn compatible(tx: &Transaction<'_>, role: &str, account: &str) -> Result<i64, Error> {
    let (kind, normal) = role_kind(role)?;
    tx.query_row("SELECT id FROM accounts WHERE account_no=?1 AND type=?2 AND normal_balance=?3 AND active=1",params![account,kind,normal],|r|r.get(0)).optional().map_err(invalid)?.ok_or_else(||Error::new("account_setup_refused","FX role requires an active compatible account and normal balance"))
}
/// Only the caller's admitted write transaction can install/append. All DDL rolls back with it.
pub(crate) fn confirm(
    tx: &Transaction<'_>,
    role: &str,
    account: &str,
    actor: &str,
) -> Result<(), Error> {
    crate::journal_post::require_current(tx)?;
    role_kind(role)?;
    if !text(actor, 256) || !text(account, 256) {
        return Err(Error::new(
            "account_setup_refused",
            "bounded account and actor required for FX confirmation",
        ));
    }
    let account_id = compatible(tx, role, account)?;
    let objects = schema(tx)?;
    let stripped = strip_schema(tx, &objects)?;
    if stripped.len() == objects.len() {
        tx.execute_batch(DDL).map_err(invalid)?;
        tx.execute(
            "INSERT INTO native_fx_role_schema(id,version,ddl_sha256) VALUES(1,1,?1)",
            [identity()],
        )
        .map_err(invalid)?;
    }
    validate_history(tx)?;
    let current:Option<(String,i64)>=tx.query_row("SELECT account_no,version FROM native_fx_role_confirmations WHERE role=?1 ORDER BY version DESC LIMIT 1",[role],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(invalid)?;
    if current
        .as_ref()
        .is_some_and(|(previous, _)| previous == account)
    {
        crate::journal_post::require_current(tx)?;
        return Ok(());
    }
    let version = current
        .map(|(_, version)| version)
        .unwrap_or(0)
        .checked_add(1)
        .filter(|v| *v <= SAFE)
        .ok_or_else(refused)?;
    tx.execute("INSERT INTO native_fx_role_confirmations(role,account_id,account_no,version,confirmed_by,confirmation_source) VALUES(?1,?2,?3,?4,?5,'explicit')",params![role,account_id,account,version,format!("user:{actor}")]).map_err(invalid)?;
    validate_history(tx)?;
    crate::journal_post::require_current(tx)?;
    Ok(())
}
#[cfg(test)]
mod tests;
