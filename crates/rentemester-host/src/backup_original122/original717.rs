//! Company snapshot bytes never alias a workspace export or leave the admitted Owner boundary.
use super::*;
use rentemester_application::backup_original122::original717::{self as original, Operation};
use rentemester_runtime::backup_artifact::original717 as codec;
use rusqlite::{MAIN_DB, Transaction};
fn invalid() -> rentemester_engine::Error {
    rentemester_engine::Error::new(
        "backup_artifact_unavailable",
        "Original backup unavailable; reconcile retained artifacts before another deliberate attempt",
    )
}
fn refused(message: &str) -> Value {
    serde_json::json!({"ok":false,"appliedRules":[codec::RULE],"errors":[message]})
}
fn apply(
    l: &Transaction<'_>,
    guard: &mut NamespaceGuard,
    clock: &dyn Clock,
    actor: &str,
    operation: Operation,
    operator: Option<&rentemester_runtime::bootstrap::Operator>,
    effect: &mut bool,
) -> Result<Value, rentemester_engine::Error> {
    let operator = operator.filter(|o| o.actor() == format!("user:{actor}"));
    match operation {
        Operation::Archive(input) => original::pack(guard, input, operator, effect),
        Operation::Create(input) => {
            if !input.confirm {
                return Ok(refused("confirm must be true"));
            }
            let at = match input.at.as_deref() {
                Some(s) => match rentemester_storage::backup_state::original122_moment(s) {
                    Ok(v) => v,
                    Err(_) => {
                        return Ok(refused(
                            "createdAt must be a valid ISO-8601 datetime when provided",
                        ));
                    }
                },
                None => clock.now_unix_ms().map_err(|_| invalid())?,
            };
            let at = rentemester_runtime::backup_keys::timestamp(at).map_err(|_| invalid())?;
            let id = format!(
                "backup-{}",
                at.replace(['-', ':'], "")
                    .replace(".000Z", "Z")
                    .split('.')
                    .next()
                    .unwrap_or("")
                    .to_owned()
            );
            // The source id has second precision even when the requested instant has milliseconds.
            let id = if id.ends_with('Z') {
                id
            } else {
                format!("{id}Z")
            };
            if guard.load_original717(Some(&id), None).is_ok() {
                return Ok(refused(&format!("backup already exists: {id}")));
            }
            rentemester_storage::portable_backup::company(l)?;
            let count: i64 = l
                .pragma_query_value(None, "page_count", |r| r.get(0))
                .map_err(|_| invalid())?;
            let size: i64 = l
                .pragma_query_value(None, "page_size", |r| r.get(0))
                .map_err(|_| invalid())?;
            if count.checked_mul(size).is_none_or(|n| {
                n <= 0 || n as usize > rentemester_runtime::backup_artifact::MAX_FILE_BYTES
            }) {
                return Err(invalid());
            }
            let mut files = guard
                .snapshot_original717_inventory()
                .map_err(|_| invalid())?;
            files.insert(
                "ledger.sqlite".into(),
                l.serialize(MAIN_DB).map_err(|_| invalid())?.to_vec(),
            );
            let key = guard.original717_manifest_key().map_err(|_| invalid())?;
            let item = |name: &str| codec::File {
                path: name.into(),
                sha256: codec::sha256(&files[name]),
                size_bytes: files[name].len() as u64,
            };
            let category = |prefix: &str| {
                files
                    .keys()
                    .filter(|p| p.starts_with(prefix))
                    .map(|p| item(p))
                    .collect()
            };
            let count = |table: &str| {
                l.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| {
                    r.get::<_, i64>(0)
                })
                .map_err(|_| invalid())
            };
            let manifest = codec::Manifest {
                manifest_version: Some(2),
                provenance: Some(rentemester_storage::backup_state::original717::provenance(
                    l,
                )?),
                backup_id: id.clone(),
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
                    journal_entries: u64::try_from(count("journal_entries")?).map_err(|_| invalid())?,
                    documents: u64::try_from(count("documents")?).map_err(|_| invalid())?,
                    bank_transactions: u64::try_from(count("bank_transactions")?).map_err(|_| invalid())?,
                },
            };
            let mut text = serde_json::to_vec_pretty(&manifest).map_err(|_| invalid())?;
            text.push(b'\n');
            files.insert(
                "manifest.json.hmac".into(),
                format!("{}\n", codec::sign(&text, &key).map_err(|_| invalid())?).into_bytes(),
            );
            files.insert("manifest.json".into(), text);
            let proof = codec::verify(&files, &key, None).map_err(|_| invalid())?;
            *effect = true;
            let path = guard
                .retain_original717_directory(&proof)
                .map_err(|_| invalid())?;
            l.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor,created_at)VALUES('system_backup','company','1',?,?,?)",rusqlite::params![format!("Created full backup {id}"),format!("user:{actor}"),at]).map_err(|_|invalid())?;
            let mut result = serde_json::json!({"ok":true,"backupId":id,"backupDir":path,"manifestPath":path.join("manifest.json"),"dbSnapshotPath":path.join("ledger.sqlite"),"appliedRules":[codec::RULE],"errors":[]});
            if input.archive.unwrap_or(false) {
                result["archive"] = original::archive(guard, &proof, None, operator, effect)?;
            }
            Ok(result)
        }
    }
}
impl Owner {
    pub fn backup_original717(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        operation: Operation,
        operator: Option<&rentemester_runtime::bootstrap::Operator>,
    ) -> Result<
        Attempt<Result<Committed<Value>, rentemester_application::control_fence::Failure>>,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, guard, ctx| {
            let mut effect = false;
            let mut result = original::human(c, l, ctx, |_, l, actor| {
                apply(l, guard, clock, actor, operation, operator, &mut effect)
            });
            if effect {
                if let Err(f) = &mut result {
                    f.stage = rentemester_application::control_fence::FailureStage::OutcomeUnknown;
                }
            }
            result
        })
    }
    pub fn backup_original717_service(
        &mut self,
        key: &ServiceKey,
        clock: &dyn Clock,
        operation: Operation,
    ) -> Result<
        Attempt<Result<service_company::ServiceResult<Value>, service_company::ServiceFailure>>,
        OwnerError,
    > {
        self.service_document_action(key, clock, |c, l, guard, ctx| {
            let mut effect = false;
            let mut result = original::service(c, l, ctx, |_, l, actor| {
                apply(l, guard, clock, actor, operation, None, &mut effect)
            });
            if effect {
                if let Err(f) = &mut result {
                    f.business = service_company::BusinessStage::OutcomeUnknown;
                }
            }
            result
        })
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests;

impl Owner {
    /// Verified Unix transport is a server prerequisite. Neither paths nor UID grant authority.
    pub fn restore_original717(
        &mut self,
        cookies: &rentemester_runtime::auth::CookieVerifier,
        clock: &dyn Clock,
        cookie: &VerifiedCookie,
        operator: &rentemester_runtime::bootstrap::Operator,
        uid: u32,
        input: original::Restore,
    ) -> AuthAttempt<Value> {
        let catalog = self.catalog()?;
        let root = self.root.clone();
        self.control_auth(cookies.workspace(),|control|{
            use rentemester_application::password_login::{Failure,FailureStage};
            let fail=|error|Failure{stage:FailureStage::NotCommitted,error,connection_reusable:true};
            let tx=control.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate).map_err(|_|fail(invalid()))?;
            original::restore_authorize(&tx,cookies,clock,cookie,&catalog,operator).map_err(fail)?;
            let refusal=|message:&str|serde_json::json!({"ok":false,"appliedRules":["DK-BOOKKEEPING-RESTORE-001"],"errors":[message]});
            if !input.confirm{return Ok(refusal("confirm must be true"));}
            if input.confirm_text.as_deref()!=Some(format!("RESTORE {}",input.target_company).as_str()){return Ok(serde_json::json!({"ok":false,"code":"CONFIRMTEXT_MISMATCH","appliedRules":["DK-BOOKKEEPING-RESTORE-001"],"errors":["confirmText must exactly match RESTORE <targetCompany>"]}));}
            let target=std::path::Path::new(&input.target_company);
            if !target.is_absolute()||target.starts_with(&root)||root.starts_with(target)||target.components().any(|p|matches!(p,std::path::Component::ParentDir|std::path::Component::CurDir)){return Ok(refusal("targetCompanyRoot must be a separate fresh local company directory"));}
            match std::fs::symlink_metadata(target){
                Ok(meta) if meta.file_type().is_symlink()||!meta.is_dir()=>return Ok(refusal("targetCompanyRoot must be an empty/new directory")),
                Ok(_)=>match std::fs::read_dir(target){Ok(mut entries)=>{if entries.next().is_some(){return Ok(refusal("targetCompanyRoot exists and is not empty; refusing to overwrite"));}},Err(_)=>return Ok(refusal("targetCompanyRoot unavailable"))},
                Err(e) if e.kind()==std::io::ErrorKind::NotFound=>{},
                Err(_)=>return Ok(refusal("targetCompanyRoot unavailable"))
            }
            let proof=match rentemester_runtime::backup_destination::read_original717_restore(std::path::Path::new(&input.backup_dir),input.verify_key.as_deref().map(std::path::Path::new),input.public_key.as_deref().map(std::path::Path::new),&root,uid,operator){Ok(p)=>p,Err(message)=>return Ok(refusal(message))};
            let token=rentemester_runtime::auth::new_token().map_err(|_|fail(invalid()))?;
            let stage=target.parent().ok_or_else(||fail(invalid()))?.join(format!(".restore-{}.tmp",token.as_str()));
            #[cfg(target_os="macos")]let profile=NamespaceProfile::LocalUserMacos;
            #[cfg(not(target_os="macos"))]let profile=NamespaceProfile::DedicatedLinux;
            let mut staged=match rentemester_runtime::namespace::Original717RestoreStage::create(&stage,uid,profile,&proof){Ok(s)=>s,Err(_)=>return Ok(refusal("failed to stage backup restore"))};
            let db_path=stage.join("data/ledger.sqlite");
            let db=Connection::open_with_flags(&db_path,OpenFlags::SQLITE_OPEN_READ_WRITE|OpenFlags::SQLITE_OPEN_NOFOLLOW).map_err(|_|fail(invalid()))?;
            db.execute_batch("PRAGMA foreign_keys=ON;PRAGMA trusted_schema=OFF;PRAGMA synchronous=FULL;").map_err(|_|fail(invalid()))?;
            rentemester_storage::portable_backup::company(&db).map_err(fail)?;
            let restored_at=rentemester_runtime::backup_keys::timestamp(clock.now_unix_ms().map_err(|_|fail(invalid()))?).map_err(|_|fail(invalid()))?;
            db.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor,created_at)VALUES('system_restore_backup','company','1',?,?,?)",rusqlite::params![format!("Restored backup {}",proof.manifest().backup_id),operator.actor(),restored_at]).map_err(|_|fail(invalid()))?;
            db.close().map_err(|_|fail(invalid()))?;
            // NOREPLACE protects existing files, and the typed empty-target path refuses contents.
            let publication=if target.exists(){staged.publish_into_empty(target)}else{staged.publish(target)};
            if publication.is_err(){return Err(Failure{stage:FailureStage::OutcomeUnknown,error:invalid(),connection_reusable:true});}
            Ok(serde_json::json!({"ok":true,"backupId":proof.manifest().backup_id,"restoredAt":restored_at,"targetCompanyRoot":input.target_company,"restoredDbPath":target.join("data/ledger.sqlite"),"restoredFiles":{"documentsOriginals":proof.manifest().copied_files.documents_originals.len(),"invoicesIssued":proof.manifest().copied_files.invoices_issued.len(),"config":proof.manifest().copied_files.config.len()},"appliedRules":["DK-BOOKKEEPING-RESTORE-001"],"errors":[]}))
        })
    }
}

impl Owner {
    pub fn place_original717(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        input: original::Place,
        configs: &[rentemester_runtime::backup_destination::Config],
        uid: u32,
    ) -> Result<
        Attempt<Result<Committed<Value>, rentemester_application::control_fence::Failure>>,
        OwnerError,
    > {
        let root = self.root.clone();
        let slug = self.slug.clone();
        self.document_action(cookie, clock, |c, l, guard, ctx| {
            let mut effect = false;
            let mut result = original::human(c, l, ctx, |c, l, subject| {
                original::place(
                    c,
                    l,
                    guard,
                    &slug,
                    clock,
                    subject,
                    input,
                    configs,
                    uid,
                    &root,
                    &mut effect,
                )
            });
            if effect {
                if let Err(f) = &mut result {
                    f.stage = rentemester_application::control_fence::FailureStage::OutcomeUnknown;
                }
            }
            result
        })
    }
    pub fn place_original717_service(
        &mut self,
        key: &ServiceKey,
        clock: &dyn Clock,
        input: original::Place,
        configs: &[rentemester_runtime::backup_destination::Config],
        uid: u32,
    ) -> Result<
        Attempt<Result<service_company::ServiceResult<Value>, service_company::ServiceFailure>>,
        OwnerError,
    > {
        let root = self.root.clone();
        let slug = self.slug.clone();
        self.service_document_action(key, clock, |c, l, guard, ctx| {
            let mut effect = false;
            let mut result = original::service(c, l, ctx, |c, l, subject| {
                original::place(
                    c,
                    l,
                    guard,
                    &slug,
                    clock,
                    subject,
                    input,
                    configs,
                    uid,
                    &root,
                    &mut effect,
                )
            });
            if effect {
                if let Err(f) = &mut result {
                    f.business = service_company::BusinessStage::OutcomeUnknown;
                }
            }
            result
        })
    }
}

impl Owner {
    pub fn prepare_remote_original717(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        input: original::Remote,
        bindings: &[rentemester_application::backup_remote::Binding],
        uid: u32,
    ) -> Result<
        Attempt<
            Result<
                Committed<original::RemotePreparation>,
                rentemester_application::control_fence::Failure,
            >,
        >,
        OwnerError,
    > {
        let root = self.root.clone();
        let workspace = self.workspace.clone();
        let slug = self.slug.clone();
        self.document_action(cookie, clock, |c, l, guard, ctx| {
            rentemester_application::backup_original122::original717::human_prepare(
                c,
                l,
                ctx,
                |c, _, subject| {
                    original::prepare_remote(
                        c, guard, &workspace, &slug, &root, uid, clock, subject, input, bindings,
                    )
                },
            )
        })
    }
    pub fn finish_remote_original717(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        prepared: &original::PreparedRemote,
        observed: Result<&rentemester_runtime::backup_remote::Observation, ()>,
        bindings: &[rentemester_application::backup_remote::Binding],
        uid: u32,
    ) -> Result<
        Attempt<Result<Committed<Value>, rentemester_application::control_fence::Failure>>,
        OwnerError,
    > {
        let root = self.root.clone();
        let workspace = self.workspace.clone();
        let slug = self.slug.clone();
        self.document_action(cookie, clock, |c, l, guard, ctx| {
            original::human(c, l, ctx, |c, l, subject| {
                original::finish_remote(
                    c, l, guard, &workspace, &slug, &root, uid, clock, subject, prepared, observed,
                    bindings,
                )
            })
        })
    }
}

impl Owner {
    pub fn prepare_remote_original717_service(
        &mut self,
        key: &ServiceKey,
        clock: &dyn Clock,
        input: original::Remote,
        bindings: &[rentemester_application::backup_remote::Binding],
        uid: u32,
    ) -> Result<
        Attempt<
            Result<
                service_company::ServiceResult<original::RemotePreparation>,
                service_company::ServiceFailure,
            >,
        >,
        OwnerError,
    > {
        let root = self.root.clone();
        let workspace = self.workspace.clone();
        let slug = self.slug.clone();
        self.service_document_action(key, clock, |c, l, guard, ctx| {
            original::service_prepare(c, l, ctx, |c, _, subject| {
                original::prepare_remote(
                    c, guard, &workspace, &slug, &root, uid, clock, subject, input, bindings,
                )
            })
        })
    }
    pub fn finish_remote_original717_service(
        &mut self,
        key: &ServiceKey,
        clock: &dyn Clock,
        prepared: &original::PreparedRemote,
        observed: Result<&rentemester_runtime::backup_remote::Observation, ()>,
        bindings: &[rentemester_application::backup_remote::Binding],
        uid: u32,
    ) -> Result<
        Attempt<Result<service_company::ServiceResult<Value>, service_company::ServiceFailure>>,
        OwnerError,
    > {
        let root = self.root.clone();
        let workspace = self.workspace.clone();
        let slug = self.slug.clone();
        self.service_document_action(key, clock, |c, l, guard, ctx| {
            original::service(c, l, ctx, |c, l, subject| {
                original::finish_remote(
                    c, l, guard, &workspace, &slug, &root, uid, clock, subject, prepared, observed,
                    bindings,
                )
            })
        })
    }
}
