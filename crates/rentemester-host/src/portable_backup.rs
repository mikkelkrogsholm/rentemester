//! Typed live snapshot and verified fresh restore; no raw connection escape.
use super::*;
use rentemester_application::password_login::{Failure, FailureStage};
use rentemester_runtime::{auth::CookieVerifier, backup_artifact as artifact, bootstrap::Operator};
use rentemester_storage::portable_backup as store;
use rusqlite::{
    MAIN_DB, TransactionBehavior,
    backup::{Backup, StepResult},
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
fn error(_: impl std::fmt::Display) -> rentemester_engine::Error {
    store::refused("native snapshot or restore refused; no automatic retry or adoption")
}
fn snapshot(db: &Connection) -> Result<Vec<u8>, rentemester_engine::Error> {
    let count: i64 = db
        .pragma_query_value(None, "page_count", |r| r.get(0))
        .map_err(error)?;
    let size: i64 = db
        .pragma_query_value(None, "page_size", |r| r.get(0))
        .map_err(error)?;
    if count
        .checked_mul(size)
        .is_none_or(|n| n <= 0 || n as usize > artifact::MAX_FILE_BYTES)
    {
        return Err(error("database size bound"));
    }
    let mut copy = Connection::open_in_memory().map_err(error)?;
    let backup = Backup::new(db, &mut copy).map_err(error)?;
    if backup.step(-1).map_err(error)? != StepResult::Done {
        return Err(error("backup busy; never retry writes"));
    }
    drop(backup);
    store::company(&copy)?;
    let bytes = copy.serialize(MAIN_DB).map_err(error)?.to_vec();
    copy.close().map_err(|(_, e)| error(e))?;
    Ok(bytes)
}
fn switch(
    company: &mut Option<Connection>,
    guard: &mut NamespaceGuard,
    root: &Path,
    slug: &str,
    current: &mut String,
) -> Result<(), rentemester_engine::Error> {
    if current == slug {
        return Ok(());
    }
    if let Some(db) = company.take()
        && let Err((db, e)) = db.close()
    {
        *company = Some(db);
        return Err(error(e));
    }
    guard.select_company(slug).map_err(error)?;
    *company = Some(
        Connection::open_with_flags(
            root.join(slug).join("data/ledger.sqlite"),
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )
        .map_err(error)?,
    );
    company
        .as_ref()
        .unwrap()
        .execute_batch("PRAGMA foreign_keys=ON;PRAGMA synchronous=FULL;PRAGMA trusted_schema=OFF;")
        .map_err(error)?;
    *current = slug.into();
    Ok(())
}
fn add(
    entries: &mut Vec<artifact::Snapshot>,
    entry: artifact::Snapshot,
) -> Result<(), rentemester_engine::Error> {
    let total = entries
        .iter()
        .try_fold(entry.bytes.len(), |n, e| n.checked_add(e.bytes.len()))
        .ok_or_else(|| error("size"))?;
    if entries.len() >= artifact::MAX_ENTRIES
        || entry.bytes.len() > artifact::MAX_FILE_BYTES
        || total > artifact::MAX_TOTAL_BYTES
    {
        return Err(error("archive bounds"));
    }
    entries.push(entry);
    Ok(())
}
impl Owner {
    pub fn backup_workspace(
        &mut self,
        cookies: &CookieVerifier,
        clock: &dyn Clock,
        proof: &VerifiedCookie,
        key: &artifact::SigningKey,
    ) -> AuthAttempt<Vec<u8>> {
        self.admit_auth(cookies.workspace())?;
        let slots = self.held.0.as_mut().unwrap();
        let catalog = slots.guard.catalog().map_err(OwnerError::Namespace)?;
        let catalog = rentemester_application::workspace_context::Catalog {
            workspace: self.workspace.clone(),
            companies: catalog
                .into_iter()
                .map(|(slug, name, archived)| {
                    rentemester_application::workspace_context::RegisteredCompany {
                        slug,
                        name,
                        archived,
                    }
                })
                .collect(),
        };
        let root = &self.root;
        let selected = &mut self.slug;
        let tx = slots
            .control
            .as_mut()
            .unwrap()
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| OwnerError::Database)?;
        let outcome = (|| {
            rentemester_application::portable_backup::authorize(
                &tx, cookies, clock, proof, &catalog,
            )?;
            let business = store::export_control(&tx)?;
            let mut entries = vec![];
            let mut companies = vec![];
            add(
                &mut entries,
                artifact::Snapshot {
                    kind: artifact::EntryKind::WorkspaceManifest,
                    path: "workspace.json".into(),
                    bytes: slots.guard.workspace_manifest().map_err(error)?,
                },
            )?;
            add(
                &mut entries,
                artifact::Snapshot {
                    kind: artifact::EntryKind::ControlBusiness {
                        section: artifact::BusinessSection::WorkspaceAudit,
                    },
                    path: "control/workspace-audit.json".into(),
                    bytes: business.bytes,
                },
            )?;
            for c in &catalog.companies {
                switch(
                    &mut slots.company,
                    &mut slots.guard,
                    root,
                    &c.slug,
                    selected,
                )?;
                slots
                    .guard
                    .ensure_connections(&tx, slots.company.as_ref().unwrap())
                    .map_err(error)?;
                let db = slots.company.as_ref().unwrap();
                let facts = store::company(db)?;
                companies.push(artifact::Company {
                    slug: c.slug.clone(),
                    ledger_uuid: facts.uuid,
                    schema_version: 36,
                });
                add(
                    &mut entries,
                    artifact::Snapshot {
                        kind: artifact::EntryKind::CompanyLedger {
                            company: c.slug.clone(),
                        },
                        path: format!("{}/data/ledger.sqlite", c.slug),
                        bytes: snapshot(db)?,
                    },
                )?;
                let mut inventory = BTreeSet::new();
                for bank in [false, true] {
                    for (path, bytes) in slots.guard.snapshot_originals(bank).map_err(error)? {
                        inventory.insert(path.clone());
                        add(
                            &mut entries,
                            artifact::Snapshot {
                                kind: if bank {
                                    artifact::EntryKind::BankStatement {
                                        company: c.slug.clone(),
                                    }
                                } else {
                                    artifact::EntryKind::DocumentOriginal {
                                        company: c.slug.clone(),
                                    }
                                },
                                path: format!("{}/{path}", c.slug),
                                bytes,
                            },
                        )?;
                    }
                }
                for (path, hash) in facts.originals {
                    if !inventory.contains(&path) || !path.contains(&hash) {
                        return Err(store::refused("document original inventory incomplete"));
                    }
                }
                for hash in facts.bank_hashes {
                    if !inventory.contains(&format!("bank/processed/{hash}.txt")) {
                        return Err(store::refused("bank source inventory incomplete"));
                    }
                }
            }
            let unsupported = business
                .unsupported
                .into_iter()
                .map(|(name, record_count)| {
                    Ok(artifact::UnsupportedSection {
                        section: serde_json::from_value(serde_json::json!(name)).map_err(error)?,
                        record_count,
                    })
                })
                .collect::<Result<Vec<_>, rentemester_engine::Error>>()?;
            let now = clock.now_unix_ms().map_err(error)?;
            let context = artifact::Context {
                workspace_id: self.workspace.clone(),
                created_at: rentemester_runtime::backup_keys::timestamp(now).map_err(error)?,
                control_schema_version: 30,
                companies,
                coverage: artifact::Coverage {
                    included: vec![artifact::BusinessSection::WorkspaceAudit],
                    unsupported,
                    excluded: artifact::Exclusion::ALL.to_vec(),
                },
            };
            artifact::create(context, entries, key).map_err(error)
        })();
        let rollback = tx.rollback().is_ok();
        let complete = slots.company.is_some();
        let retired = if complete {
            self.finish(rollback)
        } else {
            true
        };
        Ok(Attempt {
            outcome: outcome.map_err(|error| Failure {
                stage: FailureStage::NotCommitted,
                error,
                connection_reusable: !retired,
            }),
            retired,
        })
    }
    /// Whole signed artifact semantics are checked in private memory BEFORE any
    /// target stage exists. Signature verification requires separate trusted key.
    pub fn validate_backup(artifact: &artifact::VerifiedArtifact) -> Result<(), OwnerError> {
        validate(artifact).map_err(|_| OwnerError::Database)
    }
    pub fn restore_new(
        target: &Path,
        uid: u32,
        workspace: &str,
        primary: &str,
        profile: NamespaceProfile,
        artifact: &artifact::VerifiedArtifact,
        operator: &Operator,
    ) -> Result<Self, OwnerError> {
        if operator.workspace() != workspace
            || workspace != artifact.manifest().context.workspace_id
            || target.symlink_metadata().is_ok()
        {
            return Err(OwnerError::Context);
        }
        validate(artifact).map_err(|_| OwnerError::Database)?;
        let parent = target.parent().ok_or(OwnerError::Context)?;
        let token = rentemester_runtime::auth::new_token().map_err(|_| OwnerError::Context)?;
        let stage = parent.join(format!(".rentemester-restore-{}", token.as_str()));
        let guard = NamespaceGuard::create_verified_restore_stage(
            &stage, uid, workspace, primary, profile, artifact,
        )
        .map_err(OwnerError::Namespace)?;
        let mut owner = Self::open_guarded(&stage, workspace, primary, guard)?;
        owner.reusable = false;
        let archive_hash = rentemester_runtime::backup_keys::digest(artifact.manifest_bytes());
        let business = artifact
            .entries()
            .iter()
            .find(|e| {
                matches!(
                    e.kind(),
                    artifact::EntryKind::ControlBusiness {
                        section: artifact::BusinessSection::WorkspaceAudit
                    }
                )
            })
            .ok_or(OwnerError::Context)?;
        let slots = owner.held.0.as_mut().unwrap();
        store::initialize_control(
            slots.control.as_mut().unwrap(),
            business.bytes(),
            &archive_hash,
        )
        .map_err(|_| OwnerError::Database)?;
        for c in &artifact.manifest().context.companies {
            switch(
                &mut slots.company,
                &mut slots.guard,
                &stage,
                &c.slug,
                &mut owner.slug,
            )
            .map_err(|_| OwnerError::Database)?;
            store::restore_boundary(slots.company.as_mut().unwrap(), &archive_hash)
                .map_err(|_| OwnerError::Database)?;
        }
        switch(
            &mut slots.company,
            &mut slots.guard,
            &stage,
            primary,
            &mut owner.slug,
        )
        .map_err(|_| OwnerError::Database)?;
        // Close every SQLite descriptor while retaining guard + lease. A BUSY result
        // stays owned/quarantined and can never publish a completed target.
        if !slots.close() {
            return Err(OwnerError::Database);
        }
        slots
            .guard
            .publish_restored_root(target)
            .map_err(OwnerError::Namespace)?;
        let slots = owner.held.0.take().unwrap();
        let guard = slots.guard;
        // Same guard/lease transfers; no release/reacquire gap across publication.
        Self::open_guarded(target, workspace, primary, guard)
    }
    pub fn grant_restored_first_owner(
        &mut self,
        operator: &Operator,
        reservation: &rentemester_application::bootstrap::Reservation,
    ) -> Result<(), OwnerError> {
        self.admit_auth(operator.workspace())?;
        if reservation.phase != rentemester_application::bootstrap::Phase::AccessCompleted {
            return Err(OwnerError::Context);
        }
        let catalog = self
            .held
            .0
            .as_mut()
            .unwrap()
            .guard
            .catalog()
            .map_err(OwnerError::Namespace)?;
        let tx = self
            .held
            .0
            .as_mut()
            .unwrap()
            .control
            .as_mut()
            .unwrap()
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| OwnerError::Database)?;
        let has_restore:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM workspace_audit WHERE event_type='native_portable_restore' AND entity_type='backup')",[],|r|r.get(0)).map_err(|_|OwnerError::Database)?;
        let boot:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM rm_workspace_bootstrap_events WHERE reservation_hash=?1 AND event_type='access_completed' AND user_id=?2 AND company_slug=?3)",rusqlite::params![reservation.reservation_hash,reservation.user_id,reservation.company_slug],|r|r.get(0)).map_err(|_|OwnerError::Database)?;
        if !has_restore || !boot {
            return Err(OwnerError::Context);
        }
        store::grant_restored_owner(
            &tx,
            reservation.user_id.as_deref().ok_or(OwnerError::Context)?,
            &catalog.into_iter().map(|c| c.0).collect::<Vec<_>>(),
            operator.actor(),
        )
        .map_err(|_| OwnerError::Database)?;
        tx.commit().map_err(|_| OwnerError::Database)?;
        self.observe()?;
        self.reusable = true;
        Ok(())
    }
}
fn validate(a: &artifact::VerifiedArtifact) -> Result<(), rentemester_engine::Error> {
    let ctx = &a.manifest().context;
    if ctx.control_schema_version != 30
        || ctx.coverage.included != [artifact::BusinessSection::WorkspaceAudit]
    {
        return Err(error("unsupported control coverage"));
    }
    let manifest = a
        .entries()
        .iter()
        .find(|e| matches!(e.kind(), artifact::EntryKind::WorkspaceManifest))
        .ok_or_else(|| error("manifest"))?;
    let value: Value = serde_json::from_slice(manifest.bytes()).map_err(error)?;
    let registered = value["companies"]
        .as_array()
        .ok_or_else(|| error("companies"))?;
    let expected: BTreeSet<_> = ctx.companies.iter().map(|c| c.slug.as_str()).collect();
    let actual: BTreeSet<_> = registered
        .iter()
        .filter_map(|c| c["slug"].as_str())
        .collect();
    if value.as_object().is_none_or(|v| v.len() != 2)
        || value["version"] != 1
        || actual != expected
        || registered.len() != actual.len()
        || registered.iter().any(|c| {
            c.as_object().is_none_or(|v| v.len() != 4)
                || c["archived"] != false
                || c["name"].as_str().is_none_or(|s| {
                    s.trim().is_empty() || s.len() > 1024 || s.chars().any(char::is_control)
                })
                || c["createdAt"].as_str().is_none_or(|s| {
                    s.trim().is_empty() || s.len() > 128 || s.chars().any(char::is_control)
                })
        })
    {
        return Err(error("manifest context differs"));
    }
    for c in &ctx.companies {
        if c.schema_version != 36 {
            return Err(error("company schema"));
        }
        let ledger=a.entries().iter().find(|e|matches!(e.kind(),artifact::EntryKind::CompanyLedger{company}if company==&c.slug)).ok_or_else(||error("ledger"))?;
        let mut db = Connection::open_in_memory().map_err(error)?;
        db.deserialize_read_exact(MAIN_DB, ledger.bytes(), ledger.bytes().len(), true)
            .map_err(error)?;
        db.execute_batch("PRAGMA trusted_schema=OFF;PRAGMA foreign_keys=ON;PRAGMA query_only=ON")
            .map_err(error)?;
        let facts = store::company(&db)?;
        if facts.uuid != c.ledger_uuid {
            return Err(error("ledger identity differs"));
        }
        let paths: BTreeMap<_, _> = a.entries().iter().map(|e| (e.path(), e)).collect();
        for (path, hash) in facts.originals {
            let full = format!("{}/{path}", c.slug);
            let entry = paths
                .get(full.as_str())
                .ok_or_else(|| error("original missing"))?;
            if !matches!(entry.kind(),artifact::EntryKind::DocumentOriginal{company}if company==&c.slug)
                || rentemester_runtime::backup_keys::digest(entry.bytes()) != hash
            {
                return Err(error("original differs"));
            }
        }
        for hash in facts.bank_hashes {
            if !paths.contains_key(format!("{}/bank/processed/{hash}.txt", c.slug).as_str()) {
                return Err(error("bank archive missing"));
            }
        }
        db.close().map_err(|(_, e)| error(e))?;
    }
    // Validate the explicit business codec against a fresh canonical credential-free
    // control before publication, not by attaching or restoring the source auth DB.
    let mut control = Connection::open_in_memory().map_err(error)?;
    let business = a
        .entries()
        .iter()
        .find(|e| {
            matches!(
                e.kind(),
                artifact::EntryKind::ControlBusiness {
                    section: artifact::BusinessSection::WorkspaceAudit
                }
            )
        })
        .ok_or_else(|| error("control business missing"))?;
    store::initialize_control(
        &mut control,
        business.bytes(),
        &rentemester_runtime::backup_keys::digest(a.manifest_bytes()),
    )?;
    control.close().map_err(|(_, e)| error(e))?;
    Ok(())
}
