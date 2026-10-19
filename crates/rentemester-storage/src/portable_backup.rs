//! Explicit credential-free business codec and full current native ledger checks.
//! No path opens, archive parsing, signing, grants or operator authority.
use rentemester_engine::Error;
use rusqlite::{Connection, Transaction, params, types::ValueRef};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
pub fn refused(message: impl Into<String>) -> Error {
    Error::new("portable_backup_refused", message)
}
fn sql(_: impl std::fmt::Display) -> Error {
    refused("portable business storage refused")
}
const AUDIT_TABLES: [&str; 3] = [
    "workspace_audit",
    "rm_workspace_document_access_events",
    "rm_workspace_authorization_events",
];
const UNSUPPORTED: [(&str, &[&str]); 7] = [
    (
        "parties",
        &[
            "rm_party_events",
            "rm_party_identifiers",
            "rm_party_company_roles",
            "rm_party_alias_assertions",
            "rm_party_field_assertions",
            "rm_party_legacy_links",
        ],
    ),
    (
        "corporate-records",
        &[
            "rm_corporate_record_events",
            "rm_corporate_record_bytes",
            "rm_corporate_record_links",
            "rm_corporate_record_scope_assertions",
        ],
    ),
    (
        "company-knowledge",
        &[
            "rm_company_knowledge_assertions",
            "rm_company_knowledge_events",
        ],
    ),
    (
        "ownership",
        &[
            "rm_group_manifest_events",
            "rm_ownership_source_snapshots",
            "rm_ownership_snapshot_events",
            "rm_ownership_facts",
            "rm_ownership_fact_events",
        ],
    ),
    (
        "workspace-inbox",
        &[
            "rm_workspace_inbox_sources",
            "rm_workspace_inbox_events",
            "rm_workspace_inbox_assignments",
            "rm_workspace_inbox_exceptions",
            "rm_workspace_inbox_handoff_claims",
        ],
    ),
    (
        "intercompany",
        &[
            "rm_intercompany_mapping_events",
            "rm_intercompany_dispositions",
            "rm_intercompany_disposition_events",
            "rm_intercompany_disposition_journal_links",
            "rm_intercompany_disposition_lifecycle_events",
        ],
    ),
    (
        "consolidation",
        &[
            "rm_consolidation_profile_events",
            "rm_consolidation_elimination_events",
        ],
    ),
];
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", content = "value", deny_unknown_fields)]
enum Cell {
    Null,
    Integer(i64),
    Text(String),
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Table {
    columns: Vec<String>,
    rows: Vec<Vec<Cell>>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Business {
    format: String,
    tables: BTreeMap<String, Table>,
}
fn columns(db: &Connection, table: &str) -> Result<Vec<String>, Error> {
    db.prepare(&format!("PRAGMA table_info({table})"))
        .map_err(sql)?
        .query_map([], |r| r.get::<_, String>(1))
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)
}
pub struct Control {
    pub bytes: Vec<u8>,
    pub unsupported: Vec<(&'static str, u64)>,
}
pub fn export_control(tx: &Transaction<'_>) -> Result<Control, Error> {
    crate::workspace_members::writer(tx)?;
    let mut unsupported = vec![];
    for (section, tables) in UNSUPPORTED {
        let mut count = 0u64;
        for table in tables {
            let n: i64 = tx
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
                .map_err(sql)?;
            count = count
                .checked_add(u64::try_from(n).map_err(sql)?)
                .ok_or_else(|| refused("business coverage bound"))?;
        }
        if count != 0 {
            return Err(refused(format!(
                "nonempty unsupported business section: {section} ({count} records)"
            )));
        }
        unsupported.push((section, count));
    }
    let mut tables = BTreeMap::new();
    let mut cell_bytes = 0usize;
    for table in AUDIT_TABLES {
        let cols = columns(tx, table)?;
        let mut q = tx
            .prepare(&format!("SELECT * FROM {table} ORDER BY id LIMIT 100001"))
            .map_err(sql)?;
        let mut rows = q.query([]).map_err(sql)?;
        let mut out = vec![];
        while let Some(r) = rows.next().map_err(sql)? {
            if out.len() >= 100000 {
                return Err(refused("business audit row bound"));
            }
            let mut row = vec![];
            for i in 0..cols.len() {
                row.push(match r.get_ref(i).map_err(sql)? {
                    ValueRef::Null => Cell::Null,
                    ValueRef::Integer(n) => Cell::Integer(n),
                    ValueRef::Text(s) => {
                        cell_bytes = cell_bytes
                            .checked_add(s.len())
                            .ok_or_else(|| refused("business audit byte bound"))?;
                        if cell_bytes > 16 * 1024 * 1024 {
                            return Err(refused("business audit byte bound"));
                        }
                        Cell::Text(std::str::from_utf8(s).map_err(sql)?.into())
                    }
                    _ => return Err(refused("unsupported audit cell")),
                });
            }
            out.push(row);
        }
        tables.insert(
            table.into(),
            Table {
                columns: cols,
                rows: out,
            },
        );
    }
    Ok(Control {
        bytes: serde_json::to_vec(&Business {
            format: "rentemester-native-business-control/1".into(),
            tables,
        })
        .map_err(sql)?,
        unsupported,
    })
}
pub fn initialize_control(
    db: &mut Connection,
    bytes: &[u8],
    archive_hash: &str,
) -> Result<(), Error> {
    crate::control_migration::initialize_empty(db)?;
    let b: Business = serde_json::from_slice(bytes).map_err(sql)?;
    if b.format != "rentemester-native-business-control/1" || b.tables.len() != AUDIT_TABLES.len() {
        return Err(refused("unsupported business codec"));
    }
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(sql)?;
    for table in AUDIT_TABLES {
        let data = b
            .tables
            .get(table)
            .ok_or_else(|| refused("missing business audit table"))?;
        if data.columns != columns(&tx, table)? || data.rows.len() > 100000 {
            return Err(refused("business audit shape differs"));
        }
        let placeholders = (0..data.columns.len())
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        let mut q = tx
            .prepare(&format!("INSERT INTO {table} VALUES({placeholders})"))
            .map_err(sql)?;
        for row in &data.rows {
            if row.len() != data.columns.len() {
                return Err(refused("business audit row differs"));
            }
            let values = row
                .iter()
                .map(|c| match c {
                    Cell::Null => rusqlite::types::Value::Null,
                    Cell::Integer(n) => rusqlite::types::Value::Integer(*n),
                    Cell::Text(s) => rusqlite::types::Value::Text(s.clone()),
                })
                .collect::<Vec<_>>();
            q.execute(rusqlite::params_from_iter(values)).map_err(sql)?;
        }
    }
    tx.execute("INSERT INTO workspace_audit(event_type,entity_type,entity_id,actor) VALUES('native_portable_restore','backup',?1,'system:portable-restore')",[archive_hash]).map_err(sql)?;
    tx.commit().map_err(sql)?;
    crate::control_migration::validate_current_control(db)
}
pub struct Company {
    pub uuid: String,
    pub originals: Vec<(String, String)>,
    pub bank_hashes: Vec<String>,
}
pub fn company(db: &Connection) -> Result<Company, Error> {
    crate::validate(db)?;
    if !crate::company_baseline::recognizes(db, 36)? {
        return Err(refused("complete current native company schema required"));
    }
    crate::migration::integrity(db)?;
    let report = crate::journal_hash::verify(db)?;
    if !report.ok {
        return Err(refused("journal hash chain differs"));
    }
    let uuid: String = db
        .query_row(
            "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
            [],
            |r| r.get(0),
        )
        .map_err(sql)?;
    let mut originals = vec![];
    let mut q = db
        .prepare("SELECT stored_path,sha256_hash FROM documents ORDER BY id")
        .map_err(sql)?;
    let mut rows = q.query([]).map_err(sql)?;
    while let Some(r) = rows.next().map_err(sql)? {
        let p: Option<String> = r.get(0).map_err(sql)?;
        let h: Option<String> = r.get(1).map_err(sql)?;
        let (Some(p), Some(h)) = (p, h) else {
            return Err(refused("registered document original unavailable"));
        };
        originals.push((p, h));
    }
    let mut q = db
        .prepare(
            "SELECT DISTINCT source_file_hash FROM bank_transactions ORDER BY source_file_hash",
        )
        .map_err(sql)?;
    let hashes = q
        .query_map([], |r| r.get::<_, Option<String>>(0))
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    let bank_hashes = hashes
        .into_iter()
        .map(|h| h.ok_or_else(|| refused("bank statement source unavailable")))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Company {
        uuid,
        originals,
        bank_hashes,
    })
}
/// Immutable restore boundary. Receipts, their IDs, payloads and ledger UUID stay unchanged.
pub fn restore_boundary(db: &mut Connection, archive_hash: &str) -> Result<(), Error> {
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(sql)?;
    crate::validate(&tx)?;
    let high: i64 = tx
        .query_row(
            "SELECT COALESCE(MAX(id),0) FROM mutation_idempotency_tombstones",
            [],
            |r| r.get(0),
        )
        .map_err(sql)?;
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES('native_restore_receipt_boundary','mutation_idempotency_tombstones',?1,?2,'system:portable-restore')",params![high.to_string(),format!("Imported receipt high-water mark; archive {archive_hash}; original keys require manual reconciliation under fresh identities")]).map_err(sql)?;
    tx.commit().map_err(sql)
}
/// Only an explicitly configured local restore operator, after the normal new
/// first-owner saga, provisions access to these restored ledgers. No old grants imported.
pub fn grant_restored_owner(
    tx: &Transaction<'_>,
    user: &str,
    companies: &[String],
    operator: &str,
) -> Result<(), Error> {
    crate::workspace_members::writer(tx)?;
    let sole: bool = tx
        .query_row("SELECT COUNT(*)=1 AND MIN(id)=?1 FROM user", [user], |r| {
            r.get(0)
        })
        .map_err(sql)?;
    if !sole || crate::workspace_members::access(tx, user)?.as_deref() != Some("workspace_owner") {
        return Err(refused("new restore owner bootstrap required"));
    }
    for slug in companies {
        let current = crate::workspace_members::membership(tx, user, slug)?;
        if current.is_none() {
            tx.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES(?1,?2,'grant','owner',?3)",params![user,slug,operator]).map_err(sql)?;
        } else if current.as_deref() != Some("owner") {
            return Err(refused(
                "restored owner access changed; no automatic regrant",
            ));
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn pair() -> (Connection, Connection) {
        let mut c = Connection::open_in_memory().unwrap();
        let mut l = Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic portable").unwrap();
        (c, l)
    }
    #[test]
    fn business_codec_preserves_historical_evidence_without_auth_or_grants() {
        let (mut source, _) = pair();
        source.execute_batch("INSERT INTO user(id,name,email,emailVerified,createdAt,updatedAt)VALUES('old','Historical','old@example.invalid',1,'2026-01-01','2026-01-01');INSERT INTO workspace_audit(event_type,entity_type,entity_id,actor)VALUES('synthetic','company','source','user:old');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('old','activate','workspace_owner','user:old');").unwrap();
        let tx = source
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let exported = export_control(&tx).unwrap();
        tx.rollback().unwrap();
        assert_eq!(exported.unsupported.len(), 7);
        let mut restored = Connection::open_in_memory().unwrap();
        initialize_control(&mut restored, &exported.bytes, &"a".repeat(64)).unwrap();
        for table in [
            "user",
            "session",
            "account",
            "twoFactor",
            "rm_workspace_user_access_events",
            "rm_company_membership_events",
            "rm_workspace_invitation_events",
            "apikey",
        ] {
            assert_eq!(
                restored
                    .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r
                        .get::<_, i64>(0))
                    .unwrap(),
                0
            );
        }
        assert_eq!(
            restored
                .query_row(
                    "SELECT actor FROM workspace_audit WHERE event_type='synthetic'",
                    [],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
            "user:old"
        );
    }
    #[test]
    fn nonempty_unsupported_business_is_refused_and_unknown_codec_never_imports_authority() {
        let (mut c, _) = pair();
        c.execute("INSERT INTO rm_group_manifest_events(manifest_hash,canonical_manifest,actor,created_at)VALUES(?1,'{}','system:synthetic','2026-01-01T00:00:00Z')",["a".repeat(64)]).unwrap();
        let tx = c
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let e = export_control(&tx).err().unwrap();
        assert!(e.message.contains("ownership"));
        tx.rollback().unwrap();
        let mut fresh = Connection::open_in_memory().unwrap();
        assert!(initialize_control(&mut fresh,br#"{"format":"rentemester-native-business-control/1","tables":{"user":{"columns":[],"rows":[]}}}"#,&"b".repeat(64)).is_err());
        assert_eq!(
            fresh
                .query_row("SELECT COUNT(*) FROM user", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}
