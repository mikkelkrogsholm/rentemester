//! Source TAR transfer through the single lease-held Owner, never native provenance.
use crate::*;
use rentemester_application::password_login::{Failure, FailureStage};
use rentemester_runtime::{
    auth::CookieVerifier, backup_artifact::original717 as codec, bootstrap::Operator,
    original716::transfer as source,
};
use rusqlite::TransactionBehavior;
fn error(_: impl std::fmt::Display) -> rentemester_engine::Error {
    rentemester_storage::portable_backup::refused(
        "source workspace transfer refused; reconcile retained effects before another deliberate attempt",
    )
}
impl Owner {
    pub fn snapshot_original716(
        &mut self,
        cookies: &CookieVerifier,
        clock: &dyn Clock,
        proof: &VerifiedCookie,
        operator: &Operator,
        at: Option<&str>,
    ) -> AuthAttempt<Vec<u8>> {
        let catalog = self.catalog()?;
        self.admit_auth(cookies.workspace())?;
        let original = self.slug.clone();
        let root = self.root.clone();
        let slots = self.held.0.as_mut().unwrap();
        let tx = slots
            .control
            .as_mut()
            .unwrap()
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| OwnerError::Database)?;
        let mut effect = false;
        let result = (|| {
            rentemester_application::original716::transfer::authorize(
                &tx, cookies, clock, proof, &catalog, operator,
            )?;
            rentemester_storage::original716::transfer::require_source_export_coverage(&tx)?;
            let access: source::AccessPlan = serde_json::from_value(
                rentemester_storage::original716::transfer::access_plan(&tx)?,
            )
            .map_err(error)?;
            let now = match at {
                Some(at) => rentemester_storage::backup_state::original122_moment(at)?,
                None => clock.now_unix_ms().map_err(error)?,
            };
            let at = rentemester_runtime::backup_keys::timestamp(now).map_err(error)?;
            let workspace = slots.guard.workspace_manifest().map_err(error)?;
            let mut companies = vec![];
            let mut provenance = None;
            for c in &catalog.companies {
                crate::portable_backup::switch(
                    &mut slots.company,
                    &mut slots.guard,
                    &root,
                    &c.slug,
                    &mut self.slug,
                    true,
                )?;
                slots
                    .guard
                    .ensure_connections(&tx, slots.company.as_ref().unwrap())
                    .map_err(error)?;
                let db = slots.company.as_ref().unwrap();
                let facts = rentemester_storage::portable_backup::company(db)?;
                if !facts.bank_hashes.is_empty() {
                    return Err(error("source format cannot encode native bank originals"));
                }
                // Validate native-only original evidence before taking one consistent image.
                crate::invoice_email::validate_live_sources(db, &mut slots.guard)?;
                crate::archive_import::validate_live_sources(db, &mut slots.guard)?;
                crate::dinero_import::validate_live_sources(db, &mut slots.guard)?;
                crate::invoice_extraction::validate_live_sources(db, &mut slots.guard)?;
                effect = true; // key creation may persist even if subsequent output fails
                let signing = slots.guard.original716_signing_key(true).map_err(error)?;
                let key = slots.guard.original717_manifest_key().map_err(error)?;
                let mut files = slots
                    .guard
                    .snapshot_original717_inventory()
                    .map_err(error)?;
                files.retain(|p, _| {
                    !p.starts_with("config/")
                        || matches!(
                            p.as_str(),
                            "config/policy.yaml"
                                | "config/backup-manifest.pub"
                                | "config/backup-lock.json"
                        )
                });
                files.insert(
                    "ledger.sqlite".into(),
                    crate::portable_backup::snapshot(db)?,
                );
                let item = |p: &str| codec::File {
                    path: p.into(),
                    sha256: codec::sha256(&files[p]),
                    size_bytes: files[p].len() as u64,
                };
                let category = |prefix: &str| {
                    files
                        .keys()
                        .filter(|p| p.starts_with(prefix))
                        .map(|p| item(p))
                        .collect()
                };
                let count = |t: &str| {
                    db.query_row(&format!("SELECT count(*) FROM {t}"), [], |r| {
                        r.get::<_, i64>(0)
                    })
                    .map_err(error)
                    .and_then(|n| u64::try_from(n).map_err(error))
                };
                let p = rentemester_storage::backup_state::original717::provenance(db)?;
                provenance = Some(p.clone());
                let manifest = codec::Manifest {
                    manifest_version: Some(2),
                    provenance: Some(p),
                    backup_id: format!("backup-{}", now),
                    created_at: at.clone(),
                    rule_ids: vec![codec::RULE.into()],
                    manifest_signature: codec::ManifestSignature {
                        algorithm: "hmac-sha256".into(),
                        key_hint: codec::sha256(&key)[..16].into(),
                        signature_path: "manifest.json.hmac".into(),
                    },
                    asymmetric_signature: None,
                    db_snapshot: item("ledger.sqlite"),
                    copied_files: codec::CopiedFiles {
                        documents_originals: category("documents-originals/"),
                        invoices_issued: category("invoices-issued/"),
                        config: category("config/"),
                    },
                    ledger_stats: codec::LedgerStats {
                        journal_entries: count("journal_entries")?,
                        documents: count("documents")?,
                        bank_transactions: count("bank_transactions")?,
                    },
                };
                let sealed = codec::seal_original716(files, manifest, &key, Some(&signing))
                    .map_err(error)?;
                let tar = codec::archive(sealed.files()).map_err(error)?;
                companies.push((
                    source::Company {
                        slug: c.slug.clone(),
                        name: c.name.clone(),
                        archived: c.archived,
                        backup: source::File {
                            path: String::new(),
                            sha256: String::new(),
                            size_bytes: 0,
                        },
                    },
                    tar,
                ));
            }
            source::create_archive(
                workspace,
                access,
                companies,
                at,
                provenance.ok_or_else(|| error("empty workspace"))?,
            )
            .map_err(error)
        })();
        let rollback = tx.rollback().is_ok();
        let restored = crate::portable_backup::switch(
            &mut slots.company,
            &mut slots.guard,
            &root,
            &original,
            &mut self.slug,
            false,
        )
        .is_ok();
        let retired = self.finish(rollback && restored);
        Ok(Attempt {
            outcome: result.map_err(|error| Failure {
                stage: if effect {
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

    /// Caller must retain an independent source fence or explicitly establish that
    /// the original source is unavailable. This is not a live migration/cutover API.
    pub fn restore_source_original716(
        target: &Path,
        uid: u32,
        workspace: &str,
        primary: &str,
        profile: NamespaceProfile,
        source: &source::TrustedArchive,
        operator: &Operator,
        source_unavailable: bool,
    ) -> Result<Self, OwnerError> {
        if !source_unavailable
            || operator.workspace() != workspace
            || target.symlink_metadata().is_ok()
        {
            return Err(OwnerError::Context);
        }
        rentemester_application::original716::transfer::validate(source)
            .map_err(|_| OwnerError::Database)?;
        let token = rentemester_runtime::auth::new_token().map_err(|_| OwnerError::Context)?;
        let stage = target
            .parent()
            .ok_or(OwnerError::Context)?
            .join(format!(".restore-workspace-{}", token.as_str()));
        let guard = NamespaceGuard::create_source_restore_stage(
            &stage, uid, workspace, primary, profile, source,
        )
        .map_err(OwnerError::Namespace)?;
        let hash = codec::sha256(
            &serde_json::to_vec(source.archive().manifest()).map_err(|_| OwnerError::Context)?,
        );
        let mut owner = Self::open_guarded(&stage, workspace, primary, guard)?;
        owner.reusable = false;
        let slots = owner.held.0.as_mut().unwrap();
        rentemester_storage::original716::transfer::initialize_source_control(
            slots.control.as_mut().unwrap(),
            &hash,
        )
        .map_err(|_| OwnerError::Database)?;
        for c in &source.archive().manifest().companies {
            crate::portable_backup::switch(
                &mut slots.company,
                &mut slots.guard,
                &stage,
                &c.slug,
                &mut owner.slug,
                true,
            )
            .map_err(|_| OwnerError::Database)?;
            rentemester_storage::portable_backup::company(slots.company.as_ref().unwrap())
                .map_err(|_| OwnerError::Database)?;
            let db = slots.company.as_ref().unwrap();
            let tx = slots
                .control
                .as_mut()
                .unwrap()
                .transaction_with_behavior(TransactionBehavior::Deferred)
                .map_err(|_| OwnerError::Database)?;
            rentemester_storage::portable_backup::validate_inbox_company(&tx, db, &c.slug)
                .map_err(|_| OwnerError::Database)?;
            let facts = rentemester_storage::portable_backup::company(db)
                .map_err(|_| OwnerError::Database)?;
            crate::intercompany_dispositions::validate_historical_company(
                &tx,
                db,
                &c.slug,
                &facts.uuid,
            )
            .map_err(|_| OwnerError::Database)?;
            tx.rollback().map_err(|_| OwnerError::Database)?;
            crate::invoice_email::validate_live_sources(db, &mut slots.guard)
                .map_err(|_| OwnerError::Database)?;
            crate::archive_import::validate_live_sources(db, &mut slots.guard)
                .map_err(|_| OwnerError::Database)?;
            crate::dinero_import::validate_live_sources(db, &mut slots.guard)
                .map_err(|_| OwnerError::Database)?;
            crate::invoice_extraction::validate_live_sources(db, &mut slots.guard)
                .map_err(|_| OwnerError::Database)?;
            rentemester_storage::portable_backup::restore_boundary(
                slots.company.as_mut().unwrap(),
                &hash,
            )
            .map_err(|_| OwnerError::Database)?;
        }
        crate::portable_backup::switch(
            &mut slots.company,
            &mut slots.guard,
            &stage,
            primary,
            &mut owner.slug,
            false,
        )
        .map_err(|_| OwnerError::Database)?;
        if !slots.close() {
            return Err(OwnerError::Database);
        }
        slots
            .guard
            .publish_restored_root(target)
            .map_err(OwnerError::Namespace)?;
        let guard = owner.held.0.take().unwrap().guard;
        Self::open_guarded(target, workspace, primary, guard)
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use rentemester_runtime::auth::{AuthError, CookieProfile};
    struct Fixed;
    impl Clock for Fixed {
        fn now_unix_ms(&self) -> Result<i64, AuthError> {
            Ok(1767225660000)
        }
    }
    #[test]
    #[ignore = "qualified private process spawned by source_transfer_end_to_end"]
    fn qualified_source_transfer_child() {
        rentemester_runtime::original716::bootstrap::private_process();
        let parent = std::path::PathBuf::from(std::env::var("RM_SOURCE_TRANSFER_TEST").unwrap());
        let root = parent.join("source");
        let uid = rentemester_runtime::original716::bootstrap::private_process();
        let profile = NamespaceProfile::LocalUserMacos;
        let mut owner = Owner::create_with_chart_profile(
            &root,
            uid,
            "w",
            "a",
            "Synthetic A",
            "2026-01-01T00:00:00.000Z",
            &[],
            profile,
        )
        .unwrap();
        owner.held.0.as_mut().unwrap().control.as_mut().unwrap().execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('u','Synthetic','u@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('f','synthetic','[]','u',1);INSERT INTO rm_workspace_mfa_events(user_id,event_type,actor,created_at)VALUES('u','mfa_enabled','system:better-auth','2026-01-01T00:00:00.000Z');INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('s','u','synthetic-token','2026-01-01T00:00:01.000Z','2026-01-01T00:00:01.000Z','2026-01-01T12:00:00.000Z');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','workspace_owner','user:u');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','owner','user:u');").unwrap();
        owner.held.0.as_mut().unwrap().company.as_mut().unwrap().execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('2990','Synthetic preserved equity','equity','credit')").unwrap();
        assert!(Owner::open_with_profile(&root, uid, "w", "a", profile).is_err());
        let cookies = CookieVerifier::new(
            "w",
            CookieProfile::Development,
            "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE",
        )
        .unwrap();
        let cookie = cookies.issue_session("synthetic-token").unwrap();
        let proof = cookies
            .verify(Some(cookie.set_cookie().split(';').next().unwrap()), None)
            .unwrap();
        let op = Operator::from_policy("w", "user:u", &["user:u".into()]).unwrap();
        // Retain one synthetic native artifact for the packaged CLI adapter check.
        // Public RFC8032 vector, never an operator key/default.
        if let Some(dir) = std::env::var_os("RENTEMESTER_OPERATOR_ACCEPT_DIR") {
            use rentemester_runtime::backup_artifact as artifact;
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
            let native = owner
                .backup_workspace(&cookies, &Fixed, &proof, &key)
                .unwrap()
                .outcome
                .unwrap();
            let dir = std::path::PathBuf::from(dir);
            std::fs::write(dir.join("native.json"), native).unwrap();
            std::fs::write(dir.join("native.pub"), key.public_key_bytes()).unwrap();
        }
        let wire = owner
            .snapshot_original716(&cookies, &Fixed, &proof, &op, None)
            .unwrap()
            .outcome
            .unwrap();
        let archive = source::check_archive(&wire).unwrap();
        let company = codec::read_archive(&archive.files()["companies/a.tar"]).unwrap();
        let key = company["config/backup-manifest.pub"].clone();
        let keys = codec::Files::from([("a".into(), key)]);
        let trusted = source::trust_archive(&wire, &keys).unwrap();
        assert!(source::trust_archive(&wire, &codec::Files::new()).is_err());
        let wrong = rentemester_runtime::original716::custody::CompanyKey::generate().unwrap();
        assert!(
            source::trust_archive(
                &wire,
                &codec::Files::from([("a".into(), wrong.public_pem().as_bytes().to_vec())])
            )
            .is_err()
        );
        let mut tamper = wire.clone();
        tamper[520] ^= 1;
        assert!(source::trust_archive(&tamper, &keys).is_err());
        let target = parent.join("restored");
        assert!(
            Owner::restore_source_original716(
                &target, uid, "w", "a", profile, &trusted, &op, false
            )
            .is_err()
        );
        let mut restored =
            Owner::restore_source_original716(&target, uid, "w", "a", profile, &trusted, &op, true)
                .unwrap();
        assert!(
            Owner::restore_source_original716(&target, uid, "w", "a", profile, &trusted, &op, true)
                .is_err()
        );
        assert!(Owner::open_with_profile(&target, uid, "w", "a", profile).is_err());
        let slots = restored.held.0.as_mut().unwrap();
        let db = slots.company.as_ref().unwrap();
        assert_eq!(
            db.query_row(
                "SELECT name FROM accounts WHERE account_no='2990'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "Synthetic preserved equity"
        );
        for table in [
            "user",
            "session",
            "account",
            "twoFactor",
            "apikey",
            "rm_company_membership_events",
        ] {
            assert_eq!(
                slots
                    .control
                    .as_ref()
                    .unwrap()
                    .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                        .get::<_, i64>(0))
                    .unwrap(),
                0
            );
        }
        assert_eq!(slots.control.as_ref().unwrap().query_row("SELECT count(*) FROM workspace_audit WHERE event_type='native_portable_restore'",[],|r|r.get::<_,i64>(0)).unwrap(),0);
        assert_eq!(slots.control.as_ref().unwrap().query_row("SELECT count(*) FROM workspace_audit WHERE event_type='source_workspace_snapshot_restore'",[],|r|r.get::<_,i64>(0)).unwrap(),1);
        assert!(
            target
                .join(".rentemester/restored-access-plan.json")
                .is_file()
        );
        restored.try_close().map_err(|_| ()).unwrap();
        let reopened = Owner::open_with_profile(&target, uid, "w", "a", profile).unwrap();
        reopened.try_close().map_err(|_| ()).unwrap();
        owner.try_close().map_err(|_| ()).unwrap();
    }
    #[test]
    fn source_transfer_end_to_end() {
        let parent = tempfile::tempdir_in(std::env::var_os("HOME").unwrap()).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "original716::transfer::tests::qualified_source_transfer_child",
                "--nocapture",
            ])
            .env(
                "RM_SOURCE_TRANSFER_TEST",
                parent.path().canonicalize().unwrap(),
            )
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
