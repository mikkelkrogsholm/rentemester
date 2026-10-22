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
pub(super) fn switch(
    company: &mut Option<Connection>,
    guard: &mut NamespaceGuard,
    root: &Path,
    slug: &str,
    current: &mut String,
    for_snapshot: bool,
) -> Result<(), rentemester_engine::Error> {
    if current == slug && company.is_some() {
        if !for_snapshot {
            guard.select_company(slug).map_err(error)?;
        }
        return Ok(());
    }
    if let Some(db) = company.take()
        && let Err((db, e)) = db.close()
    {
        *company = Some(db);
        return Err(error(e));
    }
    if for_snapshot {
        guard.select_company_for_snapshot(slug).map_err(error)?;
    } else {
        guard.select_company(slug).map_err(error)?;
    }
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
        let original_selected = self.slug.clone();
        let selected = &mut self.slug;
        let tx = slots
            .control
            .as_mut()
            .unwrap()
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| OwnerError::Database)?;
        let mut retention_attempted = false;
        let mut outcome = (|| {
            rentemester_application::portable_backup::authorize(
                &tx, cookies, clock, proof, &catalog,
            )?;
            let business = store::export_control(&tx)?;
            store::validate_party_companies(
                &tx,
                &catalog
                    .companies
                    .iter()
                    .map(|c| c.slug.clone())
                    .collect::<Vec<_>>(),
            )?;
            store::validate_inbox_companies(
                &tx,
                &catalog
                    .companies
                    .iter()
                    .map(|c| c.slug.clone())
                    .collect::<Vec<_>>(),
            )?;
            store::validate_corporate_companies(
                &tx,
                &catalog
                    .companies
                    .iter()
                    .map(|c| c.slug.clone())
                    .collect::<Vec<_>>(),
            )?;
            store::validate_ownership_companies(
                &tx,
                &catalog
                    .companies
                    .iter()
                    .map(|c| c.slug.clone())
                    .collect::<Vec<_>>(),
            )?;
            store::validate_intercompany_companies(
                &tx,
                &catalog
                    .companies
                    .iter()
                    .map(|c| c.slug.clone())
                    .collect::<Vec<_>>(),
            )?;
            store::validate_consolidation_companies(
                &tx,
                &catalog
                    .companies
                    .iter()
                    .map(|c| c.slug.clone())
                    .collect::<Vec<_>>(),
            )?;
            let mut entries = vec![];
            let mut companies = vec![];
            let mut activity_ids = std::collections::BTreeMap::new();
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
            add(
                &mut entries,
                artifact::Snapshot {
                    kind: artifact::EntryKind::ControlBusiness {
                        section: artifact::BusinessSection::CompanyKnowledge,
                    },
                    path: "control/company-knowledge.json".into(),
                    bytes: business.knowledge_bytes,
                },
            )?;
            add(
                &mut entries,
                artifact::Snapshot {
                    kind: artifact::EntryKind::ControlBusiness {
                        section: artifact::BusinessSection::Parties,
                    },
                    path: "control/parties.json".into(),
                    bytes: business.parties_bytes,
                },
            )?;
            add(
                &mut entries,
                artifact::Snapshot {
                    kind: artifact::EntryKind::ControlBusiness {
                        section: artifact::BusinessSection::WorkspaceInbox,
                    },
                    path: "control/workspace-inbox.json".into(),
                    bytes: business.inbox_bytes,
                },
            )?;
            add(
                &mut entries,
                artifact::Snapshot {
                    kind: artifact::EntryKind::ControlBusiness {
                        section: artifact::BusinessSection::CorporateRecords,
                    },
                    path: "control/corporate-records.json".into(),
                    bytes: business.corporate_bytes,
                },
            )?;
            add(
                &mut entries,
                artifact::Snapshot {
                    kind: artifact::EntryKind::ControlBusiness {
                        section: artifact::BusinessSection::Ownership,
                    },
                    path: "control/ownership.json".into(),
                    bytes: business.ownership_bytes,
                },
            )?;
            for c in &catalog.companies {
                switch(
                    &mut slots.company,
                    &mut slots.guard,
                    root,
                    &c.slug,
                    selected,
                    true,
                )?;
                slots
                    .guard
                    .ensure_connections(&tx, slots.company.as_ref().unwrap())
                    .map_err(error)?;
                let db = slots.company.as_ref().unwrap();
                let facts = store::company(db)?;
                super::invoice_email::validate_live_sources(db, &mut slots.guard)?;
                super::archive_import::validate_live_sources(db, &mut slots.guard)?;
                super::dinero_import::validate_live_sources(db, &mut slots.guard)?;
                super::invoice_extraction::validate_live_sources(db, &mut slots.guard)?;
                store::validate_inbox_company(&tx, db, &c.slug)?;
                super::intercompany_dispositions::validate_historical_company(
                    &tx,
                    db,
                    &c.slug,
                    &facts.uuid,
                )?;
                let ids: [i64; 3] = [
                    db.query_row("SELECT COALESCE(MAX(id),0) FROM journal_entries", [], |r| {
                        r.get(0)
                    })
                    .map_err(error)?,
                    db.query_row("SELECT COALESCE(MAX(id),0) FROM documents", [], |r| {
                        r.get(0)
                    })
                    .map_err(error)?,
                    db.query_row(
                        "SELECT COALESCE(MAX(id),0) FROM bank_transactions",
                        [],
                        |r| r.get(0),
                    )
                    .map_err(error)?,
                ];
                activity_ids.insert(c.slug.clone(), ids);
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
            add(
                &mut entries,
                artifact::Snapshot {
                    kind: artifact::EntryKind::ControlBusiness {
                        section: artifact::BusinessSection::Intercompany,
                    },
                    path: "control/intercompany.json".into(),
                    bytes: business.intercompany_bytes,
                },
            )?;
            add(
                &mut entries,
                artifact::Snapshot {
                    kind: artifact::EntryKind::ControlBusiness {
                        section: artifact::BusinessSection::Consolidation,
                    },
                    path: "control/consolidation.json".into(),
                    bytes: business.consolidation_bytes,
                },
            )?;
            let unsupported = business
                .unsupported
                .into_iter()
                // Dispositions are checked empty; all known group, mapping,
                // profile and elimination rows are encoded in signed sections.
                .filter(|(name, _)| !["intercompany", "consolidation"].contains(name))
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
                    included: vec![
                        artifact::BusinessSection::WorkspaceAudit,
                        artifact::BusinessSection::Parties,
                        artifact::BusinessSection::CompanyKnowledge,
                        artifact::BusinessSection::WorkspaceInbox,
                        artifact::BusinessSection::CorporateRecords,
                        artifact::BusinessSection::Ownership,
                        artifact::BusinessSection::Intercompany,
                        artifact::BusinessSection::Consolidation,
                    ],
                    unsupported,
                    excluded: artifact::Exclusion::ALL.to_vec(),
                },
            };
            let wire = artifact::create(context, entries, key).map_err(error)?;
            let trusted =
                artifact::TrustedPublicKey::from_bytes(&key.public_key_bytes()).map_err(error)?;
            let verified = artifact::verify(&wire, &trusted).map_err(error)?;
            // Retention is independent of a successful browser/download reply.
            // Any failure from this point can have left a complete immutable file.
            retention_attempted = true;
            slots
                .guard
                .retain_backup(&wire, &trusted, &self.workspace)
                .map_err(error)?;
            let manifest = verified.manifest();
            let companies = manifest
                .context
                .companies
                .iter()
                .map(|company| {
                    let entry = manifest
                        .inventory
                        .iter()
                        .find(|entry| {
                            matches!(&entry.kind,
                    artifact::EntryKind::CompanyLedger { company: slug } if slug == &company.slug)
                        })
                        .ok_or_else(|| error("retained snapshot ledger inventory missing"))?;
                    Ok(rentemester_storage::backup_inventory::Company {
                        slug: company.slug.clone(),
                        ledger_uuid: company.ledger_uuid.clone(),
                        snapshot_sha256: entry.sha256.clone(),
                        activity_max_ids: activity_ids.get(&company.slug).copied(),
                    })
                })
                .collect::<Result<Vec<_>, rentemester_engine::Error>>()?;
            let current = rentemester_storage::cookie_session::resolve(
                &tx,
                proof.token(),
                now,
                rentemester_storage::cookie_session::SessionRequirement::Current,
            )?;
            rentemester_storage::backup_inventory::append(
                &tx,
                &rentemester_storage::backup_inventory::Snapshot {
                    version: 1,
                    workspace_id: self.workspace.clone(),
                    archive_sha256: rentemester_runtime::backup_keys::digest(&wire),
                    archive_size_bytes: wire.len() as u64,
                    signing_public_key_sha256: manifest.signing_public_key_sha256.clone(),
                    signing_public_key: Some(key.public_key_bytes().to_vec()),
                    created_at: manifest.context.created_at.clone(),
                    created_at_ms: now,
                    companies,
                },
                &format!("user:{}", current.subject),
            )?;
            Ok(wire)
        })();
        let rollback = if outcome.is_ok() {
            match tx.commit() {
                Ok(()) => true,
                Err(_) => {
                    outcome = Err(error("retained backup inventory commit outcome unknown"));
                    false
                }
            }
        } else {
            tx.rollback().is_ok()
        };
        // Maintenance may pin archived companies. Restore the caller's ordinary
        // active selection on success AND failure before returning a reusable owner.
        let restored = switch(
            &mut slots.company,
            &mut slots.guard,
            root,
            &original_selected,
            selected,
            false,
        )
        .is_ok();
        let complete = slots.company.is_some();
        let retired = if complete {
            self.finish(rollback && restored)
        } else {
            true
        };
        let outcome = if restored {
            outcome
        } else {
            Err(error("ordinary company selection could not be restored"))
        };
        Ok(Attempt {
            outcome: outcome.map_err(|error| Failure {
                stage: if retention_attempted {
                    FailureStage::OutcomeUnknown
                } else {
                    FailureStage::NotCommitted
                },
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
        Self::restore_transfer_impl(
            target, uid, workspace, primary, profile, artifact, operator, None,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub fn stage_transfer_new(
        target: &Path,
        uid: u32,
        workspace: &str,
        primary: &str,
        profile: NamespaceProfile,
        artifact: &artifact::VerifiedArtifact,
        operator: &Operator,
        wire_archive_sha256: &str,
        destination: &str,
    ) -> Result<Self, OwnerError> {
        Self::restore_transfer_impl(
            target,
            uid,
            workspace,
            primary,
            profile,
            artifact,
            operator,
            Some((wire_archive_sha256, destination)),
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn restore_transfer_impl(
        target: &Path,
        uid: u32,
        workspace: &str,
        primary: &str,
        profile: NamespaceProfile,
        artifact: &artifact::VerifiedArtifact,
        operator: &Operator,
        transfer: Option<(&str, &str)>,
    ) -> Result<Self, OwnerError> {
        if operator.workspace() != workspace
            || workspace != artifact.manifest().context.workspace_id
            || target.symlink_metadata().is_ok()
        {
            return Err(OwnerError::Context);
        }
        validate(artifact).map_err(|_| OwnerError::Database)?;
        let source_transfer = archive_transfer_state(artifact).map_err(|_| OwnerError::Database)?;
        if (transfer.is_some() && source_transfer.mode != "frozen")
            || (transfer.is_none() && source_transfer.mode != "active")
        {
            return Err(OwnerError::Context);
        }
        if source_transfer.mode != "active"
            && source_transfer.companies
                != artifact
                    .manifest()
                    .context
                    .companies
                    .iter()
                    .map(|c| rentemester_storage::workspace_transfer::Company {
                        slug: c.slug.clone(),
                        ledger_uuid: c.ledger_uuid.clone(),
                    })
                    .collect::<Vec<_>>()
        {
            return Err(OwnerError::Context);
        }
        let staged = if let Some((hash, destination)) = transfer {
            let destination_instance_id = rentemester_storage::workspace_transfer::digest(
                rentemester_runtime::auth::new_token()
                    .map_err(|_| OwnerError::Context)?
                    .as_str()
                    .as_bytes(),
            )[..32]
                .to_owned();
            let c = rentemester_storage::workspace_transfer::Challenge {
                version: 1,
                transfer_id: source_transfer
                    .transfer_id
                    .clone()
                    .ok_or(OwnerError::Context)?,
                source_instance_id: source_transfer
                    .source_instance_id
                    .clone()
                    .ok_or(OwnerError::Context)?,
                workspace_id: workspace.into(),
                archive_sha256: hash.into(),
                destination_instance_id,
                destination: destination.into(),
                companies: artifact
                    .manifest()
                    .context
                    .companies
                    .iter()
                    .map(|c| rentemester_storage::workspace_transfer::Company {
                        slug: c.slug.clone(),
                        ledger_uuid: c.ledger_uuid.clone(),
                    })
                    .collect(),
            };
            c.validate().map_err(|_| OwnerError::Context)?;
            let wire: rentemester_runtime::workspace_transfer::Challenge =
                serde_json::from_value(serde_json::to_value(&c).map_err(|_| OwnerError::Context)?)
                    .map_err(|_| OwnerError::Context)?;
            wire.validate().map_err(|_| OwnerError::Context)?;
            Some(c)
        } else {
            None
        };
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
        let slots = owner.held.0.as_mut().unwrap();
        restore_business(slots.control.as_mut().unwrap(), artifact, &archive_hash)
            .map_err(|_| OwnerError::Database)?;
        if let Some(challenge) = staged {
            let tx = slots
                .control
                .as_mut()
                .unwrap()
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(|_| OwnerError::Database)?;
            rentemester_storage::workspace_transfer::append(
                &tx,
                rentemester_storage::workspace_transfer::Fact::Staged { challenge },
                "system:portable-transfer",
                &rentemester_runtime::backup_keys::timestamp(
                    rentemester_runtime::auth::SystemClock
                        .now_unix_ms()
                        .map_err(|_| OwnerError::Context)?,
                )
                .map_err(|_| OwnerError::Context)?,
            )
            .map_err(|_| OwnerError::Database)?;
            tx.commit().map_err(|_| OwnerError::Database)?;
        }
        for c in &artifact.manifest().context.companies {
            switch(
                &mut slots.company,
                &mut slots.guard,
                &stage,
                &c.slug,
                &mut owner.slug,
                true,
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
            false,
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
        || !ctx
            .coverage
            .included
            .contains(&artifact::BusinessSection::WorkspaceAudit)
        || ctx.coverage.included.iter().any(|s| {
            !matches!(
                s,
                artifact::BusinessSection::WorkspaceAudit
                    | artifact::BusinessSection::CompanyKnowledge
                    | artifact::BusinessSection::Parties
                    | artifact::BusinessSection::WorkspaceInbox
                    | artifact::BusinessSection::CorporateRecords
                    | artifact::BusinessSection::Ownership
                    | artifact::BusinessSection::Intercompany
                    | artifact::BusinessSection::Consolidation
            )
        })
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
                || c["archived"].as_bool().is_none()
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
    let mut control = Connection::open_in_memory().map_err(error)?;
    restore_business(
        &mut control,
        a,
        &rentemester_runtime::backup_keys::digest(a.manifest_bytes()),
    )?;
    let control_tx = control
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .map_err(error)?;
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
        // Includes the complete scheduler chain/generation/boundary validation
        // before restore_new creates any target stage or publishes a namespace.
        let facts = store::company(&db)?;
        store::validate_inbox_company(&control_tx, &db, &c.slug)?;
        super::intercompany_dispositions::validate_historical_company(
            &control_tx,
            &db,
            &c.slug,
            &facts.uuid,
        )?;
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
        super::invoice_email::validate_archive_sources(&db, a, &c.slug)?;
        super::archive_import::validate_archive_sources(&db, a, &c.slug)?;
        super::dinero_import::validate_archive_sources(&db, a, &c.slug)?;
        super::invoice_extraction::validate_archive_sources(&db, a, &c.slug)?;
        for hash in facts.bank_hashes {
            if !paths.contains_key(format!("{}/bank/processed/{hash}.txt", c.slug).as_str()) {
                return Err(error("bank archive missing"));
            }
        }
        db.close().map_err(|(_, e)| error(e))?;
    }
    control_tx.rollback().map_err(error)?;
    control.close().map_err(|(_, e)| error(e))?;
    Ok(())
}

fn restore_business(
    control: &mut Connection,
    a: &artifact::VerifiedArtifact,
    archive_hash: &str,
) -> Result<(), rentemester_engine::Error> {
    let entry = |section| {
        a.entries().iter().find(|e|matches!(e.kind(),artifact::EntryKind::ControlBusiness{section:s}if *s==section)).ok_or_else(||error("control business section missing"))
    };
    store::initialize_control(
        control,
        entry(artifact::BusinessSection::WorkspaceAudit)?.bytes(),
        archive_hash,
    )?;
    if a.manifest()
        .context
        .coverage
        .included
        .contains(&artifact::BusinessSection::Parties)
    {
        store::initialize_parties(
            control,
            entry(artifact::BusinessSection::Parties)?.bytes(),
            &a.manifest()
                .context
                .companies
                .iter()
                .map(|c| c.slug.clone())
                .collect::<Vec<_>>(),
        )?;
    }
    if a.manifest()
        .context
        .coverage
        .included
        .contains(&artifact::BusinessSection::CompanyKnowledge)
    {
        store::initialize_knowledge(
            control,
            entry(artifact::BusinessSection::CompanyKnowledge)?.bytes(),
        )?;
    }
    if a.manifest()
        .context
        .coverage
        .included
        .contains(&artifact::BusinessSection::WorkspaceInbox)
    {
        store::initialize_inbox(
            control,
            entry(artifact::BusinessSection::WorkspaceInbox)?.bytes(),
            &a.manifest()
                .context
                .companies
                .iter()
                .map(|c| c.slug.clone())
                .collect::<Vec<_>>(),
        )?;
    }
    if a.manifest()
        .context
        .coverage
        .included
        .contains(&artifact::BusinessSection::CorporateRecords)
    {
        store::initialize_corporate(
            control,
            entry(artifact::BusinessSection::CorporateRecords)?.bytes(),
            &a.manifest()
                .context
                .companies
                .iter()
                .map(|c| c.slug.clone())
                .collect::<Vec<_>>(),
        )?;
    }
    if a.manifest()
        .context
        .coverage
        .included
        .contains(&artifact::BusinessSection::Ownership)
    {
        store::initialize_ownership(
            control,
            entry(artifact::BusinessSection::Ownership)?.bytes(),
            &a.manifest()
                .context
                .companies
                .iter()
                .map(|c| c.slug.clone())
                .collect::<Vec<_>>(),
        )?;
    }
    if a.manifest()
        .context
        .coverage
        .included
        .contains(&artifact::BusinessSection::Intercompany)
    {
        store::initialize_intercompany(
            control,
            entry(artifact::BusinessSection::Intercompany)?.bytes(),
            &a.manifest()
                .context
                .companies
                .iter()
                .map(|c| c.slug.clone())
                .collect::<Vec<_>>(),
        )?;
    }
    if a.manifest()
        .context
        .coverage
        .included
        .contains(&artifact::BusinessSection::Consolidation)
    {
        store::initialize_consolidation(
            control,
            entry(artifact::BusinessSection::Consolidation)?.bytes(),
            &a.manifest()
                .context
                .companies
                .iter()
                .map(|c| c.slug.clone())
                .collect::<Vec<_>>(),
        )?;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    pub(super) fn signed(
        corporate: bool,
        knowledge: bool,
        parties: bool,
        invalid: bool,
        wrong_uuid: bool,
        inbox_case: Option<u8>,
    ) -> artifact::VerifiedArtifact {
        signed_case(
            corporate, knowledge, parties, invalid, wrong_uuid, inbox_case, false,
        )
    }
    fn signed_case(
        corporate: bool,
        knowledge: bool,
        parties: bool,
        invalid: bool,
        wrong_uuid: bool,
        inbox_case: Option<u8>,
        ownership: bool,
    ) -> artifact::VerifiedArtifact {
        let mut control = Connection::open_in_memory().unwrap();
        let mut company = Connection::open_in_memory().unwrap();
        rentemester_storage::fresh::initialize_empty_pair(
            &mut control,
            &mut company,
            "Synthetic backup coverage",
        )
        .unwrap();
        if parties {
            let tx = control
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            let i=serde_json::from_value::<rentemester_storage::parties::Create>(serde_json::json!({"partyId":"party-signed-synthetic","kind":"organization","name":"Synthetic signed party","aliases":["Synthetic signed alias"],"identifiers":[],"source":"synthetic-signed-source","observedAt":"2026-01-01","reviewAssertion":"Synthetic signed review","role":"supplier"})).unwrap();
            rentemester_storage::parties::create(
                &tx,
                &i,
                "unused",
                "user:historical-owner",
                "2026-01-02T00:00:00.000Z",
            )
            .unwrap();
            rentemester_storage::parties::role(
                &tx,
                "party-signed-synthetic",
                "synthetic",
                &rentemester_storage::parties::Role {
                    role: "supplier".into(),
                    defaults: None,
                },
                "user:historical-owner",
                "2026-01-02T00:00:00.000Z",
            )
            .unwrap();
            tx.commit().unwrap();
        }
        if corporate {
            let tx = control
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            use rentemester_storage::corporate_records as corp;
            for (id, raw, kind) in [
                (
                    "record-signed-original",
                    &b"\x00\xffsynthetic original"[..],
                    "annual_report",
                ),
                (
                    "record-signed-replacement",
                    &b"synthetic replacement"[..],
                    "articles",
                ),
            ] {
                let input=serde_json::from_value(json!({"recordId":id,"type":kind,"bytesBase64":"unused-in-storage","filename":"synthetic.txt","source":"Synthetic signed source","receivedAt":"2026-01-01","uploader":"Historical source uploader","sensitivity":"restricted"})).unwrap();
                let mut links = vec![corp::Link {
                    r#type: "company".into(),
                    id: "synthetic".into(),
                }];
                if parties {
                    links.push(corp::Link {
                        r#type: "party".into(),
                        id: "party-signed-synthetic".into(),
                    });
                }
                corp::ingest(&tx, &input, raw, "unused", &links, "user:historical-owner").unwrap();
            }
            corp::enrich(
                &tx,
                "record-signed-original",
                &corp::Enrich {
                    record_id: None,
                    assertion: "Synthetic signed enrichment".into(),
                },
                "user:historical-reviewer",
                "2026-01-02T00:00:00.000Z",
            )
            .unwrap();
            corp::supersede(
                &tx,
                "record-signed-original",
                &corp::Supersede {
                    record_id: None,
                    replacement_record_id: "record-signed-replacement".into(),
                    reason: "Synthetic reviewed replacement".into(),
                },
                "user:historical-reviewer",
                "2026-01-03T00:00:00.000Z",
            )
            .unwrap();
            tx.commit().unwrap();
        }
        let mut inbox_original = None;
        if let Some(case) = inbox_case {
            use rentemester_storage::workspace_inbox as inbox;
            let raw = b"Synthetic signed inbox original\n";
            let id = "inbox-signed-source";
            let now = "2026-01-01T00:00:00.000Z";
            let input=serde_json::from_value(json!({"sourceId":id,"idempotencyKey":"signed-inbox-key","bytesBase64":"typed-original","filename":"synthetic-inbox.txt","mimeType":"text/plain","transport":"upload","receivedAt":now,"metadata":{"documentType":"cash_register_receipt"},"candidates":[]})).unwrap();
            let tx = control
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            inbox::ingest(
                &tx,
                "synthetic",
                &input,
                raw,
                "unused",
                &["synthetic".into()].into_iter().collect(),
                inbox::Provenance {
                    actor: "user:historical-source",
                    now,
                },
            )
            .unwrap();
            inbox::assign(
                &tx,
                "synthetic",
                id,
                "synthetic",
                &["synthetic".into()].into_iter().collect(),
                "user:historical-source",
                now,
            )
            .unwrap();
            inbox::claim(
                &tx,
                id,
                "synthetic",
                "original-claim",
                now,
                "2026-01-01T00:02:00.000Z",
            )
            .unwrap();
            tx.commit().unwrap();
            if case >= 1 {
                let file = rentemester_runtime::document::Prepared::new(
                    "synthetic-inbox.txt".into(),
                    raw.to_vec(),
                )
                .unwrap();
                let metadata=serde_json::from_value(json!({"source":format!("workspace-inbox:{id}"),"documentType":"cash_register_receipt"})).unwrap();
                let tx = company
                    .transaction_with_behavior(TransactionBehavior::Immediate)
                    .unwrap();
                let doc = rentemester_storage::documents::register(
                    &tx,
                    rentemester_storage::documents::Original {
                        filename: file.filename(),
                        hash: file.hash(),
                        path: &file.relative_path(),
                        mime: file.mime(),
                    },
                    &metadata,
                    "historical-source",
                    "2026-01-01",
                )
                .unwrap();
                tx.commit().unwrap();
                if case >= 2 {
                    let tx = control
                        .transaction_with_behavior(TransactionBehavior::Immediate)
                        .unwrap();
                    inbox::complete(
                        &tx,
                        id,
                        "synthetic",
                        "original-claim",
                        doc["document"]["id"].as_i64().unwrap(),
                        doc["document"]["documentNo"].as_str().unwrap(),
                        inbox::Provenance {
                            actor: "user:historical-source",
                            now,
                        },
                    )
                    .unwrap();
                    tx.commit().unwrap();
                }
                inbox_original = Some((file.relative_path(), raw.to_vec()));
            }
        }
        if ownership {
            use rentemester_storage::ownership as own;
            let tx = control
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            for (id, from, bp) in [
                ("signed-owner-initial", "2000-01-01", 2000),
                ("signed-owner-later", "2001-01-01", 3000),
            ] {
                let input:own::Propose=serde_json::from_value(json!({"snapshotId":id,"source":"Synthetic signed ownership source","observedAt":"2026-01-01","facts":[{"owner":{"kind":"party","partyId":"party-signed-synthetic"},"ownedCompanySlug":"synthetic","validFrom":from,"economicBasisPoints":bp,"controlType":"equity","jurisdiction":"DK","evidenceRefs":["opaque-synthetic-source"]}]})).unwrap();
                let s = own::propose(
                    &tx,
                    &input,
                    &own::normalized(&input.facts).unwrap(),
                    "unused",
                    "user:old-maker",
                    "old-maker",
                    "2026-01-01T00:00:00.000Z",
                )
                .unwrap();
                own::review(
                    &tx,
                    &own::Review {
                        snapshot_id: id.into(),
                        decision: "approved".into(),
                    },
                    "user:old-reviewer",
                    "old-reviewer",
                    "2026-01-02T00:00:00.000Z",
                )
                .unwrap();
                own::apply(
                    &tx,
                    &own::Apply {
                        snapshot_id: id.into(),
                        snapshot_hash: s["snapshotHash"].as_str().unwrap().into(),
                        diff_hash: s["diffHash"].as_str().unwrap().into(),
                    },
                    "user:old-applier",
                    "old-applier",
                    "2026-01-03T00:00:00.000Z",
                )
                .unwrap();
            }
            tx.commit().unwrap();
        }
        let tx = control
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let business = store::export_control(&tx).unwrap();
        tx.rollback().unwrap();
        let mut entries=vec![
            artifact::Snapshot{kind:artifact::EntryKind::WorkspaceManifest,path:"workspace.json".into(),bytes:serde_json::to_vec(&serde_json::json!({"version":1,"companies":[{"slug":"synthetic","name":"Synthetic backup coverage","archived":false,"createdAt":"2026-01-01T00:00:00.000Z"}]})).unwrap()},
            artifact::Snapshot{kind:artifact::EntryKind::ControlBusiness{section:artifact::BusinessSection::WorkspaceAudit},path:"control/workspace-audit.json".into(),bytes:business.bytes},
            artifact::Snapshot{kind:artifact::EntryKind::CompanyLedger{company:"synthetic".into()},path:"synthetic/data/ledger.sqlite".into(),bytes:snapshot(&company).unwrap()},
        ];
        let mut included = vec![artifact::BusinessSection::WorkspaceAudit];
        let mut unsupported = business
            .unsupported
            .into_iter()
            .map(|(s, record_count)| artifact::UnsupportedSection {
                section: serde_json::from_value(serde_json::json!(s)).unwrap(),
                record_count,
            })
            .collect::<Vec<_>>();
        if ownership {
            let mut bytes = business.ownership_bytes;
            if invalid {
                let mut v: Value = serde_json::from_slice(&bytes).unwrap();
                v["format"] = json!("invalid-ownership-codec");
                bytes = serde_json::to_vec(&v).unwrap();
            }
            included.push(artifact::BusinessSection::Ownership);
            entries.push(artifact::Snapshot {
                kind: artifact::EntryKind::ControlBusiness {
                    section: artifact::BusinessSection::Ownership,
                },
                path: "control/ownership.json".into(),
                bytes,
            });
        } else {
            unsupported.push(artifact::UnsupportedSection {
                section: artifact::BusinessSection::Ownership,
                record_count: 0,
            });
        }
        if corporate {
            let mut bytes = business.corporate_bytes;
            if invalid {
                let mut value: Value = serde_json::from_slice(&bytes).unwrap();
                value["format"] = json!("unsupported-corporate-codec");
                bytes = serde_json::to_vec(&value).unwrap();
            }
            included.push(artifact::BusinessSection::CorporateRecords);
            entries.push(artifact::Snapshot {
                kind: artifact::EntryKind::ControlBusiness {
                    section: artifact::BusinessSection::CorporateRecords,
                },
                path: "control/corporate-records.json".into(),
                bytes,
            });
        } else {
            unsupported.push(artifact::UnsupportedSection {
                section: artifact::BusinessSection::CorporateRecords,
                record_count: 0,
            });
        }
        if inbox_case.is_some() {
            let mut bytes = business.inbox_bytes;
            if invalid {
                let mut v: Value = serde_json::from_slice(&bytes).unwrap();
                v["format"] = json!("unsupported-inbox-codec");
                bytes = serde_json::to_vec(&v).unwrap();
            }
            included.push(artifact::BusinessSection::WorkspaceInbox);
            entries.push(artifact::Snapshot {
                kind: artifact::EntryKind::ControlBusiness {
                    section: artifact::BusinessSection::WorkspaceInbox,
                },
                path: "control/workspace-inbox.json".into(),
                bytes,
            });
            if let Some((path, bytes)) = inbox_original.filter(|_| inbox_case != Some(3)) {
                entries.push(artifact::Snapshot {
                    kind: artifact::EntryKind::DocumentOriginal {
                        company: "synthetic".into(),
                    },
                    path: format!("synthetic/{path}"),
                    bytes,
                });
            }
        } else {
            unsupported.push(artifact::UnsupportedSection {
                section: artifact::BusinessSection::WorkspaceInbox,
                record_count: 0,
            });
        }
        if parties {
            let mut bytes = business.parties_bytes;
            if invalid && !ownership {
                let mut v: Value = serde_json::from_slice(&bytes).unwrap();
                v["format"] = serde_json::json!("unknown-parties-codec");
                bytes = serde_json::to_vec(&v).unwrap();
            }
            entries.push(artifact::Snapshot {
                kind: artifact::EntryKind::ControlBusiness {
                    section: artifact::BusinessSection::Parties,
                },
                path: "control/parties.json".into(),
                bytes,
            });
            included.push(artifact::BusinessSection::Parties);
        } else {
            unsupported.push(artifact::UnsupportedSection {
                section: artifact::BusinessSection::Parties,
                record_count: 0,
            });
        }
        if knowledge {
            let mut bytes = business.knowledge_bytes;
            if invalid {
                let mut value: Value = serde_json::from_slice(&bytes).unwrap();
                value["format"] = serde_json::json!("unknown-knowledge-codec");
                bytes = serde_json::to_vec(&value).unwrap();
            }
            entries.push(artifact::Snapshot {
                kind: artifact::EntryKind::ControlBusiness {
                    section: artifact::BusinessSection::CompanyKnowledge,
                },
                path: "control/company-knowledge.json".into(),
                bytes,
            });
            included.push(artifact::BusinessSection::CompanyKnowledge);
        } else {
            unsupported.push(artifact::UnsupportedSection {
                section: artifact::BusinessSection::CompanyKnowledge,
                record_count: 0,
            });
        }
        let context = artifact::Context {
            workspace_id: "synthetic-backup".into(),
            created_at: "2026-01-01T00:00:00.000Z".into(),
            control_schema_version: 30,
            companies: vec![artifact::Company {
                slug: "synthetic".into(),
                ledger_uuid: if wrong_uuid {
                    "f".repeat(32)
                } else {
                    store::company(&company).unwrap().uuid
                },
                schema_version: 36,
            }],
            coverage: artifact::Coverage {
                included,
                unsupported,
                excluded: artifact::Exclusion::ALL.to_vec(),
            },
        };
        verified(context, entries)
    }
    fn verified(
        context: artifact::Context,
        entries: Vec<artifact::Snapshot>,
    ) -> artifact::VerifiedArtifact {
        // Public RFC8032 test-vector material, never an operator default/file.
        let der = concat!(
            "3051020101300506032b657004220420",
            "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
            "812100",
            "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a"
        );
        let der = (0..der.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&der[i..i + 2], 16).unwrap())
            .collect::<Vec<_>>();
        let key = artifact::SigningKey::from_pkcs8(&der).unwrap();
        let trust = artifact::TrustedPublicKey::from_bytes(&key.public_key_bytes()).unwrap();
        artifact::verify(&artifact::create(context, entries, &key).unwrap(), &trust).unwrap()
    }
    fn mixed_company_archive(
        archived: Option<Value>,
        wrong_uuid: bool,
    ) -> artifact::VerifiedArtifact {
        let base = signed(false, false, false, false, false, None);
        let mut context = base.manifest().context.clone();
        let mut control = Connection::open_in_memory().unwrap();
        let mut archived_db = Connection::open_in_memory().unwrap();
        rentemester_storage::fresh::initialize_empty_pair(
            &mut control,
            &mut archived_db,
            "Synthetic archived company",
        )
        .unwrap();
        archived_db.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('2990','Synthetic preserved archived account','equity','credit')").unwrap();
        let uuid = store::company(&archived_db).unwrap().uuid;
        context.companies.push(artifact::Company {
            slug: "archived".into(),
            ledger_uuid: if wrong_uuid { "f".repeat(32) } else { uuid },
            schema_version: 36,
        });
        let mut manifest: Value = serde_json::from_slice(
            base.entries()
                .iter()
                .find(|e| matches!(e.kind(), artifact::EntryKind::WorkspaceManifest))
                .unwrap()
                .bytes(),
        )
        .unwrap();
        let mut row = json!({"slug":"archived","name":"Synthetic archived company","createdAt":"2026-01-01T00:00:00.000Z"});
        if let Some(flag) = archived {
            row["archived"] = flag;
        }
        manifest["companies"].as_array_mut().unwrap().push(row);
        let mut entries = base
            .entries()
            .iter()
            .map(|e| artifact::Snapshot {
                kind: e.kind().clone(),
                path: e.path().into(),
                bytes: if matches!(e.kind(), artifact::EntryKind::WorkspaceManifest) {
                    serde_json::to_vec(&manifest).unwrap()
                } else {
                    e.bytes().to_vec()
                },
            })
            .collect::<Vec<_>>();
        entries.push(artifact::Snapshot {
            kind: artifact::EntryKind::CompanyLedger {
                company: "archived".into(),
            },
            path: "archived/data/ledger.sqlite".into(),
            bytes: snapshot(&archived_db).unwrap(),
        });
        verified(context, entries)
    }
    #[test]
    fn signed_mixed_active_and_archived_companies_preserve_flag_inventory_and_native_ledger() {
        let artifact = mixed_company_archive(Some(json!(true)), false);
        validate(&artifact).unwrap();
        let manifest: Value = serde_json::from_slice(
            artifact
                .entries()
                .iter()
                .find(|e| matches!(e.kind(), artifact::EntryKind::WorkspaceManifest))
                .unwrap()
                .bytes(),
        )
        .unwrap();
        assert_eq!(manifest["companies"][0]["archived"], false);
        assert_eq!(manifest["companies"][1]["archived"], true);
        assert_eq!(artifact.manifest().context.companies.len(), 2);
        let ledger = artifact
            .entries()
            .iter()
            .find(|e| e.path() == "archived/data/ledger.sqlite")
            .unwrap();
        let mut db = Connection::open_in_memory().unwrap();
        db.deserialize_read_exact(MAIN_DB, ledger.bytes(), ledger.bytes().len(), true)
            .unwrap();
        assert_eq!(
            db.query_row::<String, _, _>(
                "SELECT name FROM accounts WHERE account_no='2990'",
                [],
                |r| r.get(0)
            )
            .unwrap(),
            "Synthetic preserved archived account"
        );
        assert_eq!(
            store::company(&db).unwrap().uuid,
            artifact
                .manifest()
                .context
                .companies
                .iter()
                .find(|company| company.slug == "archived")
                .unwrap()
                .ledger_uuid
        );
        assert!(validate(&mixed_company_archive(Some(json!(false)), false)).is_ok());
        // The newly accepted flag does not relax per-company domain identity.
        assert!(validate(&mixed_company_archive(Some(json!(true)), true)).is_err());
    }
    #[test]
    fn signed_archived_flag_requires_explicit_boolean_without_defaulting_or_coercion() {
        for flag in [
            None,
            Some(Value::Null),
            Some(json!("true")),
            Some(json!(0)),
            Some(json!({})),
        ] {
            let artifact = mixed_company_archive(flag, false);
            assert!(validate(&artifact).is_err());
        }
    }
    #[test]
    fn signed_knowledge_coverage_is_semantically_validated_and_old_audit_only_remains_supported() {
        for knowledge in [false, true] {
            let a = signed(false, knowledge, false, false, false, None);
            validate(&a).unwrap();
            let mut restored = Connection::open_in_memory().unwrap();
            restore_business(&mut restored, &a, &"a".repeat(64)).unwrap();
            for table in [
                "user",
                "rm_company_membership_events",
                "rm_company_knowledge_assertions",
                "rm_company_knowledge_events",
            ] {
                assert_eq!(
                    restored
                        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r
                            .get::<_, i64>(0))
                        .unwrap(),
                    0
                );
            }
        }
        // Valid cryptographic signature is insufficient: unsupported section
        // codec must fail the pre-publication semantic validator.
        assert!(validate(&signed(false, true, false, true, false, None)).is_err());
    }
    #[test]
    fn signed_party_codec_is_required_validated_and_preserves_facts_without_authority() {
        for knowledge in [false, true] {
            let a = signed(false, knowledge, true, false, false, None);
            validate(&a).unwrap();
            assert!(
                a.manifest()
                    .context
                    .coverage
                    .included
                    .contains(&artifact::BusinessSection::Parties)
            );
            let mut restored = Connection::open_in_memory().unwrap();
            restore_business(&mut restored, &a, &"a".repeat(64)).unwrap();
            let tx = restored
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            let p = rentemester_storage::parties::inspect(
                &tx,
                "party-signed-synthetic",
                &["synthetic".into()].into_iter().collect(),
            )
            .unwrap();
            assert_eq!(p["name"], "Synthetic signed party");
            assert_eq!(p["aliases"][0]["alias"], "synthetic signed alias");
            for name in [
                "user",
                "account",
                "session",
                "twoFactor",
                "apikey",
                "rm_company_membership_events",
                "rm_workspace_user_access_events",
            ] {
                assert_eq!(
                    tx.query_row(&format!("SELECT COUNT(*) FROM {name}"), [], |r| r
                        .get::<_, i64>(0))
                        .unwrap(),
                    0
                );
            }
            tx.rollback().unwrap();
        }
        // Correct artifact signatures cannot legitimize an unknown typed codec or a substituted ledger identity.
        assert!(validate(&signed(false, false, true, true, false, None)).is_err());
        assert!(validate(&signed(false, false, true, false, true, None)).is_err());
    }
    #[test]
    fn signed_inbox_pending_split_completed_and_physical_original_semantics_validate_before_publication()
     {
        for case in 0..3 {
            let a = signed(false, false, false, false, false, Some(case));
            validate(&a).unwrap();
            let mut restored = Connection::open_in_memory().unwrap();
            restore_business(&mut restored, &a, &"a".repeat(64)).unwrap();
            let (claim, state, expiry): (String, String, String) = restored
                .query_row(
                    "SELECT claim_id,state,lease_expires_at FROM rm_workspace_inbox_handoff_claims",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .unwrap();
            assert_eq!(claim, "original-claim");
            assert_eq!(state, if case >= 2 { "completed" } else { "claimed" });
            assert_eq!(expiry, "2026-01-01T00:02:00.000Z");
            for table in [
                "user",
                "session",
                "twoFactor",
                "rm_workspace_user_access_events",
                "rm_company_membership_events",
            ] {
                assert_eq!(
                    restored
                        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r
                            .get::<_, i64>(0))
                        .unwrap(),
                    0
                );
            }
        }
        assert!(validate(&signed(false, false, false, true, false, Some(2))).is_err());
        assert!(
            validate(&signed(false, false, false, false, false, Some(3))).is_err(),
            "signature and codec cannot replace the actual completed document original"
        );
    }
    #[test]
    fn signed_corporate_originals_history_scope_and_sensitive_gates_restore_without_authority() {
        for parties in [false, true] {
            let archive = signed(true, true, parties, false, false, None);
            validate(&archive).unwrap();
            let mut restored = Connection::open_in_memory().unwrap();
            restore_business(&mut restored, &archive, &"a".repeat(64)).unwrap();
            let tx = restored
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            let read = ["synthetic".to_owned()].into_iter().collect();
            let none = BTreeSet::new();
            use rentemester_storage::corporate_records as corp;
            let record = corp::inspect(&tx, "record-signed-original").unwrap();
            assert_eq!(record["type"], "annual_report");
            assert_eq!(record["sensitivity"], "restricted");
            assert_eq!(record["uploader"], "Historical source uploader");
            assert_eq!(
                corp::download(&tx, "record-signed-original").unwrap(),
                b"\x00\xffsynthetic original"
            );
            assert_eq!(
                record["history"].as_array().unwrap().len(),
                if parties { 5 } else { 4 }
            );
            assert!(!corp::detail_visible(&tx, "record-signed-original", &read, &none).unwrap());
            assert!(corp::detail_visible(&tx, "record-signed-original", &read, &read).unwrap());
            assert!(!corp::visible(&tx, "record-signed-original", &none).unwrap());
            for table in [
                "user",
                "account",
                "session",
                "twoFactor",
                "apikey",
                "rm_company_membership_events",
                "rm_workspace_user_access_events",
            ] {
                assert_eq!(
                    tx.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                        .get::<_, i64>(0))
                        .unwrap(),
                    0
                );
            }
            tx.rollback().unwrap();
        }
        assert!(validate(&signed(true, false, false, true, false, None)).is_err());
        assert!(validate(&signed(true, false, false, false, true, None)).is_err());
    }
    #[test]
    fn signed_ownership_preserves_history_endings_and_original_principals_without_authority() {
        let archive = signed_case(false, false, true, false, false, None, true);
        validate(&archive).unwrap();
        let mut db = Connection::open_in_memory().unwrap();
        restore_business(&mut db, &archive, &"a".repeat(64)).unwrap();
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        use rentemester_storage::ownership as own;
        assert_eq!(
            own::ids(&tx).unwrap(),
            vec!["signed-owner-initial", "signed-owner-later"]
        );
        let first = own::load(&tx, "signed-owner-initial").unwrap().public;
        assert_eq!(first["state"], "applied");
        assert_eq!(first["principal"]["id"], "old-maker");
        assert_eq!(first["history"][1]["principal_id"], "old-reviewer");
        assert_eq!(first["history"][2]["principal_id"], "old-applier");
        let facts = own::approved(&tx).unwrap();
        assert_eq!(facts.len(), 2);
        let old = facts
            .iter()
            .find(|(_, f)| f.valid_from == "2000-01-01")
            .unwrap();
        assert_eq!(old.1.valid_to_exclusive.as_deref(), Some("2001-01-01"));
        for table in [
            "user",
            "account",
            "session",
            "twoFactor",
            "apikey",
            "rm_company_membership_events",
            "rm_workspace_user_access_events",
        ] {
            assert_eq!(
                tx.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                0
            );
        }
        tx.rollback().unwrap();
        assert!(
            validate(&signed_case(false, false, true, true, false, None, true)).is_err(),
            "valid signature cannot admit unknown ownership codec"
        );
        assert!(
            validate(&signed_case(false, false, true, false, true, None, true)).is_err(),
            "signed company identity still needs actual ledger UUID"
        );
    }
}
#[cfg(test)]
#[path = "portable_backup/consolidation_tests.rs"]
mod consolidation_tests;
#[cfg(test)]
#[path = "portable_backup/intercompany_tests.rs"]
mod intercompany_tests;

#[cfg(test)]
#[path = "portable_backup/scheduled_tests.rs"]
mod scheduled_tests;

fn archive_transfer_state(
    a: &artifact::VerifiedArtifact,
) -> Result<rentemester_storage::workspace_transfer::State, rentemester_engine::Error> {
    let entry = a
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
        .ok_or_else(|| error("missing control"))?;
    let mut db = Connection::open_in_memory().map_err(error)?;
    store::initialize_control(
        &mut db,
        entry.bytes(),
        &rentemester_runtime::backup_keys::digest(a.manifest_bytes()),
    )?;
    rentemester_storage::workspace_transfer::current(&db)
}

#[cfg(test)]
mod recurring_workspace_tests;
