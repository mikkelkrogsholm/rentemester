//! Source-defined schema operations on an exclusively owned connection. No SQL input.
use crate::{Migration, company_baseline, invalid, migration};
use rentemester_engine::Error;
use rusqlite::{Connection, TransactionBehavior, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
const RAW: [&str; 35] = [
    include_str!("../../../../src/core/migrations/0002-peppol-submission-events.json"),
    include_str!("../../../../src/core/migrations/0003-recurring-automation.json"),
    include_str!("../../../../src/core/migrations/0004-dinero-import-provenance.json"),
    include_str!("../../../../src/core/migrations/0005-migration-open-items.json"),
    include_str!("../../../../src/core/migrations/0006-bank-journal-reconciliation-links.json"),
    include_str!("../../../../src/core/migrations/0007-document-scan-evidence.json"),
    include_str!("../../../../src/core/migrations/0008-issued-invoice-pdf-immutability.json"),
    include_str!("../../../../src/core/migrations/0009-accounting-draft-workflow.json"),
    include_str!("../../../../src/core/migrations/0010-internal-voucher-evidence.json"),
    include_str!("../../../../src/core/migrations/0011-purchase-vat-preflight.json"),
    include_str!("../../../../src/core/migrations/0012-posting-rules.json"),
    include_str!("../../../../src/core/migrations/0013-bookkeeping-batches.json"),
    include_str!("../../../../src/core/migrations/0014-invoice-extraction-evidence.json"),
    include_str!("../../../../src/core/migrations/0015-bookkeeping-batch-retries.json"),
    include_str!("../../../../src/core/migrations/0016-invoice-extraction-actors.json"),
    include_str!("../../../../src/core/migrations/0017-document-pdf-parses.json"),
    include_str!("../../../../src/core/migrations/0018-document-metadata-enrichments.json"),
    include_str!("../../../../src/core/migrations/0019-document-company-contexts.json"),
    include_str!("../../../../src/core/migrations/0020-mutation-idempotency-receipts.json"),
    include_str!("../../../../src/core/migrations/0021-bookkeeping-batch-revisions.json"),
    include_str!("../../../../src/core/migrations/0022-period-close-readiness.json"),
    include_str!("../../../../src/core/migrations/0023-local-idempotency-tombstones.json"),
    include_str!("../../../../src/core/migrations/0024-bookkeeping-batch-principals.json"),
    include_str!("../../../../src/core/migrations/0025-period-close-reviews.json"),
    include_str!("../../../../src/core/migrations/0026-document-party-links.json"),
    include_str!("../../../../src/core/migrations/0027-supplier-commitments.json"),
    include_str!("../../../../src/core/migrations/0028-accounting-dimensions.json"),
    include_str!("../../../../src/core/migrations/0029-document-party-resolution.json"),
    include_str!("../../../../src/core/migrations/0030-accounting-dimension-lifecycle.json"),
    include_str!(
        "../../../../src/core/migrations/0031-supplier-commitment-occurrence-matches.json"
    ),
    include_str!("../../../../src/core/migrations/0032-dimension-budget-and-provenance.json"),
    include_str!("../../../../src/core/migrations/0033-bank-reconciliation-corrections.json"),
    include_str!(
        "../../../../src/core/migrations/0034-direct-bank-purchase-payable-corrections.json"
    ),
    include_str!(
        "../../../../src/core/migrations/0035-bank-reconciliation-account-role-fallback.json"
    ),
    include_str!("../../../../src/core/migrations/0036-imported-receivables.json"),
];

fn catalogue() -> Result<Vec<Migration>, Error> {
    serde_json::from_str(include_str!("../legacy-schema.json")).map_err(invalid)
}
fn refused() -> Error {
    Error::new(
        "unsupported_schema",
        "schema is not a supported exact company prefix",
    )
}
fn inspection(db: &Connection) -> Result<Value, Error> {
    let expected = catalogue()?;
    migration::integrity(db)?;
    let history = migration::strict_history(db, "schema_migrations", &expected)?;
    let version = history.len();
    if version == expected.len() {
        crate::journal_preview::require_current(db)?;
    } else if version == 0 || !company_baseline::recognizes(db, version)? {
        return Err(refused());
    }
    Ok(
        json!({"status":if version == expected.len() {"current"} else {"pending"},
        "currentVersion":version,"requiredVersion":expected.len(),
        "pending":expected[version..].iter().map(|m|json!({"id":m.id,"name":m.name,"checksum":m.checksum})).collect::<Vec<_>>()}),
    )
}
fn audit(
    db: &Connection,
    actor: &str,
    event: &str,
    entity: &str,
    message: &str,
) -> Result<(), Error> {
    db.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES(?1,'schema',?2,?3,?4)",params![event,entity,message,actor]).map_err(invalid)?;
    Ok(())
}
/// Inspect is strictly read-only. Apply is one immediate transaction including all history and audit.
/// The host supplies policy-derived actor and holds the namespace lease throughout.
pub fn migrate(db: &mut Connection, apply: bool, actor: &str) -> Result<Value, Error> {
    if !apply {
        let schema = inspection(db)?;
        let pending = schema["status"] == "pending";
        return Ok(
            json!({"ok":true,"errors":[],"action":if pending {"migration_required"} else {"none"},"wouldMigrate":pending,"schema_outdated":pending,"schema":schema}),
        );
    }
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(invalid)?;
    let before = inspection(&tx)?;
    if before["status"] == "current" {
        tx.commit().map_err(invalid)?;
        return Ok(json!({"ok":true,"migrated":false,"schema":before}));
    }
    let from = before["currentVersion"].as_u64().ok_or_else(refused)? as usize;
    let expected = catalogue()?;
    // Bind every SQL artifact to the immutable registry before executing any SQL.
    let artifacts: Vec<Value> = RAW
        .iter()
        .zip(&expected[1..])
        .map(|(raw, m)| {
            if format!("{:x}", Sha256::digest(raw.as_bytes())) != m.checksum {
                return Err(refused());
            }
            serde_json::from_str(raw).map_err(invalid)
        })
        .collect::<Result<_, _>>()?;
    crate::company_normalization::normalize_in_transaction(&tx)?;
    let old_history = migration::strict_history(&tx, "schema_migrations", &expected)?;
    let tables: Vec<_> = migration::schema(&tx)?
        .into_iter()
        .filter(|o| o.kind == "table" && o.name != "schema_migrations")
        .map(|o| o.name)
        .collect();
    let projection = migration::projection(&tx, &tables)?;
    let before_digest = migration::project_digest(&tx, &projection)?;
    for (artifact, identity) in artifacts.iter().zip(&expected[1..]).skip(from - 1) {
        tx.execute_batch(artifact["sql"].as_str().ok_or_else(refused)?)
            .map_err(invalid)?;
        company_baseline::stamp(&tx, identity)?;
    }
    // The native normalized checkpoint includes these compatibility columns and guards.
    for column in ["bic", "account_owner", "customer_no"] {
        let present: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('bank_accounts') WHERE name=?1)",
                [column],
                |r| r.get(0),
            )
            .map_err(invalid)?;
        if !present {
            tx.execute_batch(&format!(
                "ALTER TABLE bank_accounts ADD COLUMN {column} TEXT"
            ))
            .map_err(invalid)?;
        }
    }
    for object in company_baseline::load_images()?
        .current
        .iter()
        .filter(|o| o.kind == "trigger")
    {
        tx.execute_batch(&format!(
            "DROP TRIGGER IF EXISTS {};",
            migration::quote(&object.name)
        ))
        .map_err(invalid)?;
        tx.execute_batch(&object.sql).map_err(invalid)?;
    }
    let after = inspection(&tx)?;
    let new_history = migration::strict_history(&tx, "schema_migrations", &expected)?;
    if new_history[..from] != old_history
        || migration::project_digest(&tx, &projection)? != before_digest
    {
        return Err(Error::new(
            "migration_failed",
            "migration changed existing projections or provenance",
        ));
    }
    audit(
        &tx,
        actor,
        "schema_migrated",
        &from.to_string(),
        &format!("Schema migrated from {from} to {}", expected.len()),
    )?;
    tx.commit().map_err(invalid)?;
    Ok(json!({"ok":true,"migrated":true,"from":from,"to":expected.len(),"schema":after}))
}
fn views(db: &Connection) -> Result<Value, Error> {
    let expected = company_baseline::load_images()?.current;
    let expected: Vec<_> = expected.iter().filter(|o| o.kind == "view").collect();
    let actual = migration::schema(db)?;
    let mut errors = vec![];
    let mut affected = vec![];
    let mut observed = vec![];
    for view in &expected {
        let found = actual
            .iter()
            .find(|o| o.kind == "view" && o.name == view.name);
        let sql = found.map(|o| normalize(&o.sql));
        if sql.as_deref() != Some(normalize(&view.sql).as_str()) {
            affected.push(view.name.clone());
            errors.push(format!(
                "SCHEMA_VIEW_DRIFT:{}:{}",
                view.name,
                if found.is_some() {
                    "definition_mismatch"
                } else {
                    "missing"
                }
            ));
        }
        observed.push(json!([view.name, sql]));
    }
    let canonical: Vec<_> = expected
        .iter()
        .map(|o| json!([o.name, normalize(&o.sql)]))
        .collect();
    let hash = |v: &Vec<Value>| {
        format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(v).expect("view digest"))
        )
    };
    Ok(
        json!({"ok":errors.is_empty(),"errors":errors,"affectedNames":affected,"catalogueDigest":hash(&canonical),"actualDigest":hash(&observed)}),
    )
}
// Token-aware case/whitespace normalization. Quoted literals/identifiers are preserved.
fn normalize(sql: &str) -> String {
    let mut out = String::new();
    let mut quote = None;
    let mut space = false;
    let mut chars = sql.trim().trim_end_matches(';').chars().peekable();
    while let Some(c) = chars.next() {
        if let Some(q) = quote {
            out.push(c);
            if c == q {
                if chars.peek() == Some(&q) {
                    out.push(chars.next().unwrap());
                } else {
                    quote = None;
                }
            }
        } else if c.is_whitespace() {
            space = true;
        } else {
            if space && !out.is_empty() {
                out.push(' ');
            }
            space = false;
            if matches!(c, '\'' | '"' | '`' | '[') {
                quote = Some(if c == '[' { ']' } else { c });
                out.push(c);
            } else {
                out.extend(c.to_lowercase());
            }
        }
    }
    out
}
/// Repair only the embedded source catalogue. Unknown tables/triggers/extensions refuse.
/// The exact native-extension checkpoint establishes that only supported views differ.
pub fn repair_views(
    db: &mut Connection,
    apply: bool,
    reason: Option<&str>,
    actor: &str,
) -> Result<Value, Error> {
    if apply && reason.is_none_or(|v| v.trim().is_empty() || v.trim().chars().count() > 1000) {
        return Err(Error::new(
            "invalid_reason",
            "--reason is required and must contain 1 through 1000 characters",
        ));
    }
    let tx = db
        .transaction_with_behavior(if apply {
            TransactionBehavior::Immediate
        } else {
            TransactionBehavior::Deferred
        })
        .map_err(invalid)?;
    let before = views(&tx)?;
    let expected = company_baseline::load_images()?.current;
    // Substitute only views in the expected physical checkpoint, keeping all other objects exact.
    let mut checkpoint = expected.clone();
    let actual = migration::schema(&tx)?;
    checkpoint.retain(|o| o.kind != "view");
    for view in expected.iter().filter(|o| o.kind == "view") {
        if let Some(found) = actual.iter().find(|o| o.name == view.name) {
            if found.kind != "view" {
                return Err(refused());
            }
            checkpoint.push(found.clone());
        }
    }
    checkpoint.sort_by(|a, b| a.name.cmp(&b.name));
    crate::scheduled_recurring_schema::validate(&tx, &checkpoint)?;
    let history = migration::strict_history(&tx, "schema_migrations", &catalogue()?)?;
    if history.len() != catalogue()?.len() {
        return Err(refused());
    }
    migration::integrity(&tx)?;
    let drift = before["ok"] == false;
    if !apply {
        let result = if drift {
            json!({"ok":true,"errors":[],"action":"repair_schema_views","wouldRepair":true,"views":before})
        } else {
            json!({"ok":true,"errors":[],"action":"repair_schema_views","wouldRepair":false,"schema":inspection(&tx)?,"views":before})
        };
        tx.rollback().map_err(invalid)?;
        return Ok(result);
    }
    if !drift {
        tx.commit().map_err(invalid)?;
        return Ok(json!({"ok":true,"repaired":false,"action":"none","views":before}));
    }
    let names = before["affectedNames"].as_array().ok_or_else(refused)?;
    for view in expected
        .iter()
        .filter(|o| names.iter().any(|n| n.as_str() == Some(&o.name)))
    {
        tx.execute_batch(&format!(
            "DROP VIEW IF EXISTS {};",
            migration::quote(&view.name)
        ))
        .map_err(invalid)?;
        tx.execute_batch(&view.sql).map_err(invalid)?;
    }
    let after = views(&tx)?;
    inspection(&tx)?;
    if after["ok"] != true {
        return Err(refused());
    }
    audit(&tx,actor,"schema_views_repaired","schema_views",&json!({"reason":reason.unwrap().trim(),"affectedNames":names,"before":before,"after":after}).to_string())?;
    tx.commit().map_err(invalid)?;
    Ok(json!({"ok":true,"repaired":true,"affectedNames":names,"before":before,"after":after}))
}

/// Protected SQLite image only; no source SQL descriptor or sidecar creation.
pub fn inspect_company(root: &std::path::Path, repair: bool) -> Result<Value, Error> {
    crate::Snapshot::capture_unvalidated(root)?.with_writable_copy(|db| {
        if repair {
            repair_views(db, false, None, "")
        } else {
            migrate(db, false, "")
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn current() -> Connection {
        let mut db = Connection::open_in_memory().unwrap();
        company_baseline::initialize_empty(&mut db, "Synthetic schema operator").unwrap();
        db
    }
    fn pending() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        for object in company_baseline::load_images().unwrap().baseline {
            db.execute_batch(&object.sql).unwrap();
        }
        company_baseline::stamp(&db, &catalogue().unwrap()[0]).unwrap();
        db
    }
    #[test]
    fn original716_current_noop_and_full_pending_inspect_apply() {
        let mut db = current();
        let before = migration::schema(&db).unwrap();
        assert_eq!(migrate(&mut db, false, "").unwrap()["action"], "none");
        assert_eq!(
            migrate(&mut db, true, "system:synthetic").unwrap()["migrated"],
            false
        );
        assert_eq!(before, migration::schema(&db).unwrap());
        let mut db = pending();
        let before = migration::schema(&db).unwrap();
        let report = migrate(&mut db, false, "").unwrap();
        assert_eq!(report["schema"]["currentVersion"], 1);
        assert_eq!(report["schema"]["requiredVersion"], 36);
        assert_eq!(report["schema"]["pending"].as_array().unwrap().len(), 35);
        assert_eq!(before, migration::schema(&db).unwrap());
        let result = migrate(&mut db, true, "system:synthetic").unwrap();
        assert_eq!(result["from"], 1);
        assert_eq!(result["to"], 36);
        assert_eq!(result["schema"]["status"], "current");
        assert_eq!(
            db.query_row(
                "SELECT COUNT(*) FROM audit_log WHERE event_type='schema_migrated'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(
            migrate(&mut db, true, "system:synthetic").unwrap()["migrated"],
            false
        );
    }
    #[test]
    fn original716_unknown_schema_and_history_refuse_without_writes() {
        for sql in [
            "CREATE TABLE invented(secret TEXT)",
            "UPDATE schema_migrations SET checksum='unknown' WHERE id=1",
        ] {
            let mut db = pending();
            db.execute_batch(sql).unwrap();
            let before = migration::schema(&db).unwrap();
            assert!(migrate(&mut db, false, "").is_err());
            assert!(migrate(&mut db, true, "system:synthetic").is_err());
            assert_eq!(before, migration::schema(&db).unwrap());
            assert_eq!(
                db.query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                1
            );
        }
    }
    #[test]
    fn original716_exact_view_repair_and_nonview_refusal() {
        let mut db = current();
        assert_eq!(
            repair_views(&mut db, true, Some("Synthetic noop"), "system:synthetic").unwrap()["repaired"],
            false
        );
        db.execute_batch("DROP VIEW current_supplier_commitments; CREATE VIEW current_supplier_commitments AS SELECT 'wrong' AS value;").unwrap();
        let before = migration::schema(&db).unwrap();
        let inspect = repair_views(&mut db, false, None, "").unwrap();
        assert_eq!(
            inspect["views"]["affectedNames"],
            json!(["current_supplier_commitments"])
        );
        assert_eq!(before, migration::schema(&db).unwrap());
        let repair = repair_views(
            &mut db,
            true,
            Some("Synthetic view drift"),
            "system:synthetic",
        )
        .unwrap();
        assert_eq!(repair["repaired"], true);
        assert_eq!(repair["after"]["ok"], true);
        crate::journal_preview::require_current(&db).unwrap();
        db.execute_batch("CREATE TABLE invented(secret TEXT)")
            .unwrap();
        assert!(
            repair_views(&mut db, true, Some("Synthetic refusal"), "system:synthetic").is_err()
        );
    }
    #[test]
    fn original716_history_failure_rolls_back_entire_upgrade() {
        let mut db = pending();
        // An extra trigger itself must be refused before execution: no partial migration.
        db.execute_batch("CREATE TRIGGER refuse_history BEFORE INSERT ON schema_migrations BEGIN SELECT RAISE(ABORT,'synthetic'); END;").unwrap();
        let before = migration::schema(&db).unwrap();
        assert!(migrate(&mut db, true, "system:synthetic").is_err());
        assert_eq!(before, migration::schema(&db).unwrap());
    }
}
