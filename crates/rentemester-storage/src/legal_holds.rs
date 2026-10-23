//! Exact optional extension; history is retained in the signed SQLite image.
use crate::{
    invalid,
    migration::{SchemaObject, schema},
};
use rentemester_engine::{Error, fiscal_year::Date};
use rusqlite::{Connection, Transaction, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
const DDL: &str = "CREATE TABLE native_gdpr_legal_hold_schema(id INTEGER PRIMARY KEY CHECK(id=1),version INTEGER NOT NULL CHECK(version=1),ddl_sha256 TEXT NOT NULL);\nCREATE TABLE native_gdpr_legal_hold_events(id INTEGER PRIMARY KEY,hold_id TEXT NOT NULL,subject_reference TEXT NOT NULL,subject_aliases TEXT NOT NULL,operation TEXT NOT NULL CHECK(operation IN ('place','release')),reason TEXT NOT NULL,actor TEXT NOT NULL,created_at TEXT NOT NULL);\nCREATE TRIGGER native_gdpr_legal_hold_events_no_update BEFORE UPDATE ON native_gdpr_legal_hold_events BEGIN SELECT RAISE(ABORT,'legal holds are append-only'); END;\nCREATE TRIGGER native_gdpr_legal_hold_events_no_delete BEFORE DELETE ON native_gdpr_legal_hold_events BEGIN SELECT RAISE(ABORT,'legal holds are append-only'); END;\nCREATE TRIGGER native_gdpr_legal_hold_schema_no_update BEFORE UPDATE ON native_gdpr_legal_hold_schema BEGIN SELECT RAISE(ABORT,'legal hold schema is immutable'); END;\nCREATE TRIGGER native_gdpr_legal_hold_schema_no_delete BEFORE DELETE ON native_gdpr_legal_hold_schema BEGIN SELECT RAISE(ABORT,'legal hold schema is immutable'); END;";
fn fail() -> Error {
    Error::new(
        "gdpr_refused",
        "legal hold schema, identity or lifecycle differs",
    )
}
fn identity() -> String {
    format!("{:x}", Sha256::digest(DDL.as_bytes()))
}
fn text(s: &str, max: usize) -> bool {
    !s.is_empty() && s.trim() == s && s.len() <= max && !s.chars().any(char::is_control)
}
fn exists(db: &Connection) -> Result<bool, Error> {
    db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name='native_gdpr_legal_hold_events')",
        [],
        |r| r.get(0),
    )
    .map_err(invalid)
}
pub(crate) fn strip_schema(
    db: &Connection,
    actual: &[SchemaObject],
) -> Result<Vec<SchemaObject>, Error> {
    let is_hold = |o: &SchemaObject| {
        o.name.starts_with("native_gdpr_legal_hold")
            || o.table.starts_with("native_gdpr_legal_hold")
    };
    let found = actual
        .iter()
        .filter(|o| is_hold(o))
        .cloned()
        .collect::<Vec<_>>();
    if found.is_empty() {
        return Ok(actual.to_vec());
    }
    let reference = Connection::open_in_memory().map_err(invalid)?;
    reference.execute_batch(DDL).map_err(invalid)?;
    if found != schema(&reference)? {
        return Err(fail());
    }
    let ids: Vec<(i64, i64, String)> = db
        .prepare("SELECT id,version,ddl_sha256 FROM native_gdpr_legal_hold_schema")
        .map_err(invalid)?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .map_err(invalid)?
        .collect::<Result<_, _>>()
        .map_err(invalid)?;
    if ids != vec![(1, 1, identity())] {
        return Err(fail());
    }
    history(db)?;
    Ok(actual.iter().filter(|o| !is_hold(o)).cloned().collect())
}
pub fn validate_history(db: &Connection) -> Result<(), Error> {
    strip_schema(db, &schema(db)?)?;
    Ok(())
}
fn history(db: &Connection) -> Result<Vec<Value>, Error> {
    if !exists(db)? {
        return Ok(vec![]);
    }
    let mut states = BTreeMap::<String, (String, String)>::new();
    let mut out = vec![];
    let mut q=db.prepare("SELECT id,hold_id,subject_reference,subject_aliases,operation,reason,actor,created_at FROM native_gdpr_legal_hold_events ORDER BY id").map_err(invalid)?;
    let mut rs = q.query([]).map_err(invalid)?;
    while let Some(r) = rs.next().map_err(invalid)? {
        let id: i64 = r.get(0).map_err(invalid)?;
        let hold: String = r.get(1).map_err(invalid)?;
        let subject: String = r.get(2).map_err(invalid)?;
        let aliases: String = r.get(3).map_err(invalid)?;
        let op: String = r.get(4).map_err(invalid)?;
        let reason: String = r.get(5).map_err(invalid)?;
        let actor: String = r.get(6).map_err(invalid)?;
        let at: String = r.get(7).map_err(invalid)?;
        let refs: BTreeSet<String> = serde_json::from_str(&aliases).map_err(invalid)?;
        if id != out.len() as i64 + 1
            || !text(&hold, 128)
            || !hold
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
            || !text(&reason, 2000)
            || !text(&actor, 1024)
            || !actor.starts_with("user:")
            || refs.is_empty()
            || !refs.contains(&subject)
            || refs.iter().any(|s| {
                s.len() != 71
                    || !s.starts_with("sha256:")
                    || !s[7..].bytes().all(|b| b.is_ascii_hexdigit())
            })
        {
            return Err(fail());
        }
        Date::parse(&at)?;
        match (op.as_str(), states.get(&hold)) {
            ("place", None) => {}
            ("release", Some((s, p))) if s == &subject && p == "place" => {}
            _ => return Err(fail()),
        }
        states.insert(hold.clone(), (subject.clone(), op.clone()));
        out.push(json!({"id":id,"holdId":hold,"subjectReference":subject,"subjectAliases":refs,"operation":op,"reason":reason,"actor":actor,"createdAt":at}));
    }
    Ok(out)
}
pub(crate) fn state(db: &Connection, refs: &BTreeSet<String>) -> Result<Value, Error> {
    let all = history(db)?;
    let matching = all
        .into_iter()
        .filter(|r| {
            r["subjectAliases"]
                .as_array()
                .unwrap()
                .iter()
                .any(|a| refs.contains(a.as_str().unwrap()))
        })
        .collect::<Vec<_>>();
    let mut latest = BTreeMap::new();
    for r in &matching {
        latest.insert(r["holdId"].as_str().unwrap(), r);
    }
    let active = latest
        .values()
        .filter(|r| r["operation"] == "place")
        .cloned()
        .cloned()
        .collect::<Vec<_>>();
    Ok(json!({"active":active,"history":matching}))
}
pub(crate) fn change(
    tx: &Transaction<'_>,
    refs: &BTreeSet<String>,
    reference: &str,
    hold: &str,
    op: &str,
    reason: &str,
    today: &str,
    actor: &str,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    if !exists(tx)? {
        tx.execute_batch(DDL).map_err(invalid)?;
        tx.execute(
            "INSERT INTO native_gdpr_legal_hold_schema VALUES(1,1,?1)",
            [identity()],
        )
        .map_err(invalid)?;
    }
    let all = history(tx)?;
    let prior = all.iter().rev().find(|r| r["holdId"] == hold);
    let (subject, aliases) = if op == "release" {
        let p = prior.ok_or_else(fail)?;
        if p["operation"] != "place"
            || !p["subjectAliases"]
                .as_array()
                .unwrap()
                .iter()
                .any(|a| refs.contains(a.as_str().unwrap()))
        {
            return Err(fail());
        }
        (
            p["subjectReference"].as_str().unwrap().to_owned(),
            p["subjectAliases"].to_string(),
        )
    } else {
        if op != "place" || prior.is_some() {
            return Err(fail());
        }
        (reference.to_owned(), json!(refs).to_string())
    };
    tx.execute("INSERT INTO native_gdpr_legal_hold_events(hold_id,subject_reference,subject_aliases,operation,reason,actor,created_at)VALUES(?1,?2,?3,?4,?5,?6,?7)",params![hold,subject,aliases,op,reason,format!("user:{actor}"),today]).map_err(invalid)?;
    // Validate before returning; any failure is rolled back by the owning transaction.
    history(tx)?;
    state(tx, refs)
}
