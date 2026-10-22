//! Explicit credential-free business codec and full current native ledger checks.
//! No path opens, archive parsing, signing, grants or operator authority.
mod consolidation;
mod corporate;
pub use consolidation::{
    initialize as initialize_consolidation, validate_companies as validate_consolidation_companies,
};
mod intercompany;
mod ownership;
pub use intercompany::{
    initialize as initialize_intercompany, validate_companies as validate_intercompany_companies,
};
pub use ownership::{
    initialize as initialize_ownership, validate_companies as validate_ownership_companies,
};
mod inbox;
pub mod mail_intake;
pub use corporate::{
    initialize as initialize_corporate, validate_companies as validate_corporate_companies,
};
mod parties;
pub use inbox::{
    initialize as initialize_inbox, validate_companies as validate_inbox_companies,
    validate_company as validate_inbox_company,
};
pub use parties::{
    initialize as initialize_parties, validate_companies as validate_party_companies,
};
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
const UNSUPPORTED: [(&str, &[&str]); 2] = [
    // All six intercompany tables are authenticated by the versioned codec.
    // Retain the coverage bucket with zero unencoded rows for older metadata.
    ("intercompany", &[]),
    // Preserve the legacy coverage bucket with exactly zero unencoded rows:
    // both native consolidation tables are now authenticated by their codec.
    ("consolidation", &[]),
];
const KNOWLEDGE_FORMAT: &str = "rentemester-native-company-knowledge/1";
const KNOWLEDGE_TABLES: [&str; 2] = [
    "rm_company_knowledge_assertions",
    "rm_company_knowledge_events",
];
const KNOWLEDGE_COLUMNS: [&[&str]; 2] = [
    &[
        "id",
        "assertion_id",
        "company_slug",
        "predicate",
        "value_json",
        "value_hash",
        "source_kind",
        "source_ref",
        "valid_from",
        "valid_to_exclusive",
        "certainty",
        "actor",
        "principal_kind",
        "principal_id",
        "created_at",
    ],
    &[
        "id",
        "assertion_id",
        "event_type",
        "reason",
        "supersedes_assertion_id",
        "actor",
        "principal_kind",
        "principal_id",
        "created_at",
    ],
];
fn knowledge_shape(db: &Connection) -> Result<(), Error> {
    for (name, expected) in KNOWLEDGE_TABLES.iter().zip(KNOWLEDGE_COLUMNS) {
        if columns(db, name)? != expected {
            return Err(refused("company knowledge schema differs from codec v1"));
        }
    }
    Ok(())
}
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
    pub knowledge_bytes: Vec<u8>,
    pub parties_bytes: Vec<u8>,
    pub inbox_bytes: Vec<u8>,
    pub corporate_bytes: Vec<u8>,
    pub ownership_bytes: Vec<u8>,
    pub intercompany_bytes: Vec<u8>,
    pub consolidation_bytes: Vec<u8>,
    pub unsupported: Vec<(&'static str, u64)>,
}
pub fn export_control(tx: &Transaction<'_>) -> Result<Control, Error> {
    crate::backup_registration::validate_history(tx)?;
    crate::workspace_transfer::current(tx)?;
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
    knowledge_shape(tx)?;
    Ok(Control {
        bytes: export_tables(tx, &AUDIT_TABLES, "rentemester-native-business-control/1")?,
        knowledge_bytes: export_tables(tx, &KNOWLEDGE_TABLES, KNOWLEDGE_FORMAT)?,
        parties_bytes: parties::export(tx)?,
        inbox_bytes: inbox::export(tx)?,
        corporate_bytes: corporate::export(tx)?,
        ownership_bytes: ownership::export(tx)?,
        intercompany_bytes: intercompany::export(tx)?,
        consolidation_bytes: consolidation::export(tx)?,
        unsupported,
    })
}
fn export_tables(db: &Connection, names: &[&str], format: &str) -> Result<Vec<u8>, Error> {
    let mut tables = BTreeMap::new();
    let mut cell_bytes = 0usize;
    for table in names {
        let cols = columns(db, table)?;
        let order = match *table {
            "rm_party_identifiers" => "jurisdiction,identifier_kind,identifier,party_id",
            "rm_party_company_roles" => "party_id,company_slug,role",
            "rm_party_legacy_links" => "company_slug,legacy_kind,legacy_id",
            _ => "id",
        };
        let mut q = db
            .prepare(&format!(
                "SELECT * FROM {table} ORDER BY {order} LIMIT 100001"
            ))
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
            (*table).into(),
            Table {
                columns: cols,
                rows: out,
            },
        );
    }
    serde_json::to_vec(&Business {
        format: format.into(),
        tables,
    })
    .map_err(sql)
}

pub fn initialize_control(
    db: &mut Connection,
    bytes: &[u8],
    archive_hash: &str,
) -> Result<(), Error> {
    crate::control_migration::initialize_empty(db)?;
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(sql)?;
    initialize_audit_business(&tx, bytes)?;
    tx.execute("INSERT INTO workspace_audit(event_type,entity_type,entity_id,actor) VALUES('native_portable_restore','backup',?1,'system:portable-restore')",[archive_hash]).map_err(sql)?;
    tx.commit().map_err(sql)?;
    crate::control_migration::validate_current_control(db)
}
/// Credential-free audit codec only, under the caller's current control writer.
/// Provenance of adoption versus native restore belongs to the distinct caller.
pub(crate) fn initialize_audit_business(tx: &Transaction<'_>, bytes: &[u8]) -> Result<(), Error> {
    let b: Business = serde_json::from_slice(bytes).map_err(sql)?;
    if b.format != "rentemester-native-business-control/1" || b.tables.len() != AUDIT_TABLES.len() {
        return Err(refused("unsupported business codec"));
    }
    insert_tables(tx, &b, &AUDIT_TABLES)?;
    crate::backup_registration::validate_history(tx)?;
    crate::workspace_transfer::current(tx)?;
    Ok(())
}
/// Restores only historical business assertions/events. Historical principal IDs
/// are provenance labels; neither table references live authentication authority.
pub fn initialize_knowledge(db: &mut Connection, bytes: &[u8]) -> Result<(), Error> {
    crate::control_migration::validate_current_control(db)?;
    knowledge_shape(db)?;
    let business: Business = serde_json::from_slice(bytes).map_err(sql)?;
    if business.format != KNOWLEDGE_FORMAT || business.tables.len() != KNOWLEDGE_TABLES.len() {
        return Err(refused("unsupported company knowledge codec"));
    }
    for (name, expected) in KNOWLEDGE_TABLES.iter().zip(KNOWLEDGE_COLUMNS) {
        let table = business
            .tables
            .get(*name)
            .ok_or_else(|| refused("company knowledge table missing"))?;
        if table.columns != expected {
            return Err(refused("company knowledge codec columns differ"));
        }
        for row in &table.rows {
            if row.len() != expected.len() {
                return Err(refused("company knowledge codec row differs"));
            }
            for (column, cell) in expected.iter().zip(row) {
                let valid = match (*column, cell) {
                    ("id", Cell::Integer(_)) => true,
                    ("id", _) => false,
                    ("valid_to_exclusive" | "reason" | "supersedes_assertion_id", Cell::Null) => {
                        true
                    }
                    (_, Cell::Text(_)) => true,
                    _ => false,
                };
                if !valid {
                    return Err(refused("company knowledge codec cell type differs"));
                }
            }
        }
    }
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(sql)?;
    for name in KNOWLEDGE_TABLES {
        let count: i64 = tx
            .query_row(&format!("SELECT COUNT(*) FROM {name}"), [], |r| r.get(0))
            .map_err(sql)?;
        if count != 0 {
            return Err(refused("fresh company knowledge target required"));
        }
    }
    insert_tables(&tx, &business, &KNOWLEDGE_TABLES)?;
    // FK checks are independent of the connection's enforcement PRAGMA. Never
    // commit dangling events/supersession links, even on a private validation DB.
    crate::control_migration::validate_current_control(&tx)?;
    tx.commit().map_err(sql)
}
fn insert_tables(tx: &Transaction<'_>, b: &Business, names: &[&str]) -> Result<(), Error> {
    for table in names {
        let data = b
            .tables
            .get(*table)
            .ok_or_else(|| refused("missing business audit table"))?;
        if data.columns != columns(tx, table)? || data.rows.len() > 100000 {
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
    Ok(())
}

pub struct Company {
    pub uuid: String,
    pub originals: Vec<(String, String)>,
    pub bank_hashes: Vec<String>,
}
pub fn company(db: &Connection) -> Result<Company, Error> {
    crate::journal_preview::require_current(db)
        .map_err(|_| refused("complete current native company schema required"))?;
    // The signed SQLite image already contains the complete scheduler extension.
    // Admit its source history only after validating its physical schema, chain,
    // generation references and all earlier restore boundaries.
    crate::document_pdf::validate_history(db)?;
    crate::invoice_extraction::validate_history(db)?;
    mail_intake::validate_mail_intake(db)?;
    crate::document_scan::validate_backup(db)?;
    crate::scheduled_recurring::validate_backup(db)?;
    crate::native_fx_roles::validate_history(db)?;
    crate::invoice_email::validate_backup(db)?;
    crate::public_einvoice::validate_backup(db)?;
    crate::recurring_workspace::validate_history(db)?;
    crate::document_party_links::validate_backup(db)?;
    crate::purchase_rules::validate_backup(db)?;
    crate::bookkeeping_batch::validate_backup(db)?;
    crate::direct_bank_payable_correction::validate_backup(db)?;
    crate::archive_import::validate_history(db)?;
    crate::opening_balance::validate_history(db)?;
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
    let mut originals = crate::archive_import::originals(db)?
        .into_iter()
        .map(|source| (source.path, source.sha256))
        .collect::<Vec<_>>();
    originals.extend(
        crate::dinero_import::originals(db)?
            .into_iter()
            .map(|s| (s.path, s.sha256)),
    );
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
    crate::scheduled_recurring::append_restore_boundary(&tx, archive_hash)?;
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
    fn empty_known_native_schedule_schema_is_valid_but_corruption_is_not() {
        let (_, mut db) = pair();
        assert!(company(&db).is_ok());
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        crate::scheduled_recurring_schema::install(&tx).unwrap();
        tx.commit().unwrap();
        assert!(company(&db).is_ok());
        db.execute_batch("DROP TRIGGER native_scheduled_events_no_update;")
            .unwrap();
        assert!(company(&db).is_err());
    }

    #[test]
    fn scheduler_boundary_failure_leaves_receipt_boundary_and_audit_unchanged() {
        let (_, mut db) = pair();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        crate::scheduled_recurring_schema::install(&tx).unwrap();
        tx.commit().unwrap();
        db.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES('native_restore_scheduled_boundary','native_scheduled_recurring_events','0','{}','system:portable-restore')", []).unwrap();
        let before: i64 = db
            .query_row("SELECT count(*) FROM audit_log", [], |r| r.get(0))
            .unwrap();
        assert!(restore_boundary(&mut db, &"a".repeat(64)).is_err());
        assert_eq!(
            db.query_row("SELECT count(*) FROM audit_log", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            before
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM audit_log WHERE event_type='native_restore_receipt_boundary'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
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
        assert_eq!(exported.unsupported.len(), 2);
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
    fn malformed_manifest_is_refused_and_unknown_codec_never_imports_authority() {
        let (mut c, _) = pair();
        c.execute("INSERT INTO rm_group_manifest_events(manifest_hash,canonical_manifest,actor,created_at)VALUES(?1,'{}','system:synthetic','2026-01-01T00:00:00Z')",["a".repeat(64)]).unwrap();
        let tx = c
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let e = export_control(&tx).err().unwrap();
        assert!(e.message.contains("intercompany"));
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
    fn knowledge_fixture(db: &Connection) {
        for (id, key, company, predicate, value, source, reference, from, to) in [
            (
                11,
                "knowledge-old",
                "synthetic",
                "business_description",
                r#"{"text":"Synthetic previous activity"}"#,
                "user",
                "Synthetic owner interview",
                "2026-01-01T00:00:00.000Z",
                Some("2026-05-01T00:00:00.000Z"),
            ),
            (
                23,
                "knowledge-new",
                "synthetic",
                "business_description",
                r#"{"text":"Synthetic reviewed replacement"}"#,
                "canonical_setting",
                "company/synthetic/settings",
                "2026-02-01T00:00:00.000Z",
                None,
            ),
            (
                37,
                "knowledge-rejected",
                "synthetic",
                "markets",
                r#"["Synthetic unverified market"]"#,
                "external_snapshot",
                "https://example.invalid/synthetic-evidence",
                "2026-01-01T00:00:00.000Z",
                None,
            ),
            (
                48,
                "knowledge-proposed",
                "synthetic",
                "operating_status",
                r#"{"status":"Synthetic proposed"}"#,
                "registry",
                "Synthetic explicit registry reference",
                "2026-01-01T00:00:00.000Z",
                None,
            ),
            (
                66,
                "knowledge-other",
                "other",
                "business_description",
                r#"{"text":"Synthetic separate company"}"#,
                "user",
                "Synthetic other owner interview",
                "2026-01-01T00:00:00.000Z",
                None,
            ),
        ] {
            use sha2::{Digest, Sha256};
            let hash = format!("{:x}", Sha256::digest(value.as_bytes()));
            db.execute("INSERT INTO rm_company_knowledge_assertions(id,assertion_id,company_slug,predicate,value_json,value_hash,source_kind,source_ref,valid_from,valid_to_exclusive,certainty,actor,principal_kind,principal_id,created_at)VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'confirmed','user:historical-owner','user','historical-owner','2026-01-01T00:00:00.000Z')",params![id,key,company,predicate,value,hash,source,reference,from,to]).unwrap();
        }
        for (id, key, event, reason, replacement) in [
            (101, "knowledge-old", "proposed", None, None),
            (
                113,
                "knowledge-old",
                "approved",
                Some("Synthetic original checked"),
                None,
            ),
            (127, "knowledge-new", "proposed", None, None),
            (
                139,
                "knowledge-old",
                "superseded",
                Some("Synthetic activity changed"),
                Some("knowledge-new"),
            ),
            (
                151,
                "knowledge-new",
                "approved",
                Some("Synthetic replacement reviewed"),
                None,
            ),
            (163, "knowledge-rejected", "proposed", None, None),
            (
                175,
                "knowledge-rejected",
                "rejected",
                Some("Synthetic evidence rejected"),
                None,
            ),
            (187, "knowledge-proposed", "proposed", None, None),
            (199, "knowledge-other", "proposed", None, None),
            (
                211,
                "knowledge-other",
                "approved",
                Some("Synthetic separate review"),
                None,
            ),
        ] {
            db.execute("INSERT INTO rm_company_knowledge_events(id,assertion_id,event_type,reason,supersedes_assertion_id,actor,principal_kind,principal_id,created_at)VALUES(?1,?2,?3,?4,?5,'user:historical-reviewer','user','historical-reviewer','2026-02-02T00:00:00.000Z')",params![id,key,event,reason,replacement]).unwrap();
        }
        db.execute("INSERT INTO workspace_audit(event_type,entity_type,entity_id,actor)VALUES('synthetic_knowledge_review','company_knowledge','knowledge-new','user:historical-reviewer')",[]).unwrap();
    }
    // The source's effective context uses each assertion's latest event and
    // half-open validity interval. Whole table bytes additionally prove every
    // original value, hash, provenance field and historical event survived.
    fn effective(
        db: &Connection,
        slug: &str,
        at: &str,
        proposed: bool,
    ) -> Vec<(String, String, String)> {
        db.prepare("SELECT a.assertion_id,a.value_json,(SELECT e.event_type FROM rm_company_knowledge_events e WHERE e.assertion_id=a.assertion_id ORDER BY e.id DESC LIMIT 1) AS state FROM rm_company_knowledge_assertions a WHERE a.company_slug=?1 AND a.valid_from<=?2 AND (a.valid_to_exclusive IS NULL OR a.valid_to_exclusive>?2) AND (state='approved' OR (?3 AND state='proposed')) ORDER BY a.predicate,a.assertion_id").unwrap().query_map(params![slug,at,proposed],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap().collect::<Result<Vec<_>,_>>().unwrap()
    }
    #[test]
    fn knowledge_full_history_provenance_effective_context_and_new_authority_survive() {
        let (mut source, _) = pair();
        knowledge_fixture(&source);
        source.execute_batch("INSERT INTO user(id,name,email,emailVerified,createdAt,updatedAt)VALUES('historical-owner','Synthetic historical','historical@example.invalid',1,'2026-01-01','2026-01-01');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('historical-owner','activate','workspace_owner','system:synthetic');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('historical-owner','synthetic','grant','owner','system:synthetic');").unwrap();
        let tx = source
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let exported = export_control(&tx).unwrap();
        tx.rollback().unwrap();
        assert_eq!(exported.unsupported.len(), 2);
        assert!(
            !exported
                .unsupported
                .iter()
                .any(|s| s.0 == "company-knowledge")
        );
        let mut restored = Connection::open_in_memory().unwrap();
        initialize_control(&mut restored, &exported.bytes, &"c".repeat(64)).unwrap();
        initialize_knowledge(&mut restored, &exported.knowledge_bytes).unwrap();
        assert_eq!(
            export_tables(&restored, &KNOWLEDGE_TABLES, KNOWLEDGE_FORMAT).unwrap(),
            exported.knowledge_bytes
        );
        for at in [
            "2025-12-31T23:59:59.999Z",
            "2026-01-31T23:59:59.999Z",
            "2026-02-01T00:00:00.000Z",
            "2026-05-01T00:00:00.000Z",
        ] {
            for slug in ["synthetic", "other"] {
                for proposed in [false, true] {
                    assert_eq!(
                        effective(&source, slug, at, proposed),
                        effective(&restored, slug, at, proposed)
                    );
                }
            }
        }
        let current = effective(&restored, "synthetic", "2026-03-01T00:00:00.000Z", false);
        assert_eq!(current.len(), 1);
        assert_eq!(current[0].0, "knowledge-new");
        assert_eq!(restored.query_row("SELECT actor FROM workspace_audit WHERE event_type='synthetic_knowledge_review'",[],|r|r.get::<_,String>(0)).unwrap(),"user:historical-reviewer");
        for table in [
            "user",
            "session",
            "account",
            "twoFactor",
            "rm_workspace_user_access_events",
            "rm_company_membership_events",
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
        // Normal operator/bootstrap creates this new live identity separately;
        // imported historical labels never confer membership or credentials.
        restored.execute_batch("INSERT INTO user(id,name,email,emailVerified,createdAt,updatedAt)VALUES('new-owner','Synthetic new','new@example.invalid',0,'2026-03-01','2026-03-01');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('new-owner','activate','workspace_owner','system:synthetic-restore');").unwrap();
        let tx = restored
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        grant_restored_owner(
            &tx,
            "new-owner",
            &["synthetic".into(), "other".into()],
            "system:synthetic-restore",
        )
        .unwrap();
        assert_eq!(
            crate::workspace_members::membership(&tx, "new-owner", "synthetic")
                .unwrap()
                .as_deref(),
            Some("owner")
        );
        assert_eq!(
            crate::workspace_members::membership(&tx, "historical-owner", "synthetic").unwrap(),
            None
        );
        tx.commit().unwrap();
        assert_eq!(
            export_tables(&restored, &KNOWLEDGE_TABLES, KNOWLEDGE_FORMAT).unwrap(),
            exported.knowledge_bytes
        );
        assert!(
            initialize_knowledge(&mut restored, &exported.knowledge_bytes).is_err(),
            "no replay into populated target"
        );
    }
    #[test]
    fn knowledge_unknown_codec_columns_foreign_keys_and_authority_tables_refuse_atomically() {
        let (mut source, _) = pair();
        knowledge_fixture(&source);
        let tx = source
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let exported = export_control(&tx).unwrap();
        tx.rollback().unwrap();
        let original: serde_json::Value =
            serde_json::from_slice(&exported.knowledge_bytes).unwrap();
        for case in 0..9 {
            let mut bad = original.clone();
            match case {
                0 => bad["format"] = serde_json::json!("rentemester-native-company-knowledge/2"),
                1 => {
                    bad["tables"][KNOWLEDGE_TABLES[0]]["columns"][0] =
                        serde_json::json!("unknown_future_id")
                }
                2 => {
                    bad["tables"][KNOWLEDGE_TABLES[1]]["rows"][0][1]["value"] =
                        serde_json::json!("missing-assertion")
                }
                3 => {
                    bad["tables"][KNOWLEDGE_TABLES[1]]["rows"][3][4] =
                        serde_json::json!({"type":"Text","value":"missing-replacement"})
                }
                4 => bad["tables"]["user"] = serde_json::json!({"columns":[],"rows":[]}),
                5 => {
                    bad["tables"]
                        .as_object_mut()
                        .unwrap()
                        .remove(KNOWLEDGE_TABLES[1]);
                }
                6 => {
                    bad["tables"][KNOWLEDGE_TABLES[0]]["rows"][0][0] =
                        serde_json::json!({"type":"Null"})
                }
                7 => {
                    bad["tables"][KNOWLEDGE_TABLES[0]]["rows"][0][0] =
                        serde_json::json!({"type":"Text","value":"11"})
                }
                _ => {
                    bad["tables"][KNOWLEDGE_TABLES[0]]["rows"][0][11] =
                        serde_json::json!({"type":"Integer","value":123})
                }
            }
            let mut target = Connection::open_in_memory().unwrap();
            initialize_control(&mut target, &exported.bytes, &"d".repeat(64)).unwrap();
            assert!(
                initialize_knowledge(&mut target, &serde_json::to_vec(&bad).unwrap()).is_err(),
                "case{case}"
            );
            for name in [
                KNOWLEDGE_TABLES[0],
                KNOWLEDGE_TABLES[1],
                "user",
                "rm_company_membership_events",
            ] {
                assert_eq!(
                    target
                        .query_row(&format!("SELECT COUNT(*) FROM {name}"), [], |r| r
                            .get::<_, i64>(0))
                        .unwrap(),
                    0,
                    "case{case}"
                );
            }
        }
        source
            .execute_batch(
                "ALTER TABLE rm_company_knowledge_assertions ADD COLUMN unknown_future_field TEXT",
            )
            .unwrap();
        let tx = source
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        assert!(export_control(&tx).is_err());
        tx.rollback().unwrap();
    }
}
