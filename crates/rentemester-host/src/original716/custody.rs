//! Company key operations run only inside the existing Owner/document CompanyAdmin fence.
use crate::*;
use rentemester_application::original716::custody::Rotate;
use serde_json::{Value, json};
fn invalid() -> rentemester_engine::Error {
    rentemester_engine::Error::new(
        "backup_custody_unavailable",
        "Backup custody unavailable; reconcile retained effects before another deliberate attempt",
    )
}
/// Finalwire invokes this BEFORE snapshot_original717_inventory, once per requested signed backup.
/// No backup is created here and no source HMAC manifest or snapshot is regenerated.
pub fn prepare_signed_backup(
    guard: &mut rentemester_runtime::namespace::NamespaceGuard,
    effect: &mut bool,
) -> Result<rentemester_runtime::original716::custody::CompanyKey, rentemester_engine::Error> {
    *effect = true;
    guard.original716_signing_key(true).map_err(|_| invalid())
}
impl Owner {
    pub fn rotate_backup_keypair_original716(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        input: Rotate,
        operator: Option<&rentemester_runtime::bootstrap::Operator>,
    ) -> Result<
        Attempt<Result<Committed<Value>, rentemester_application::control_fence::Failure>>,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c,l,guard,ctx| {
            let mut effect = false;
            let mut result = rentemester_application::backup_original122::original717::human(c,l,ctx, |_,l,actor| {
                if operator.is_none_or(|o| o.actor() != format!("user:{actor}")) { return Err(rentemester_engine::Error::new("access_denied", "Current local operator required")); }
                let reason = input.reason.trim();
                if reason.is_empty() { return Ok(json!({"ok":false,"errors":["reason is required for backup key rotation"]})); }
                let at = match input.at.as_deref() {
                    Some(s) => rentemester_storage::backup_state::original122_moment(s)?,
                    None => clock.now_unix_ms().map_err(|_| invalid())?,
                };
                let at = rentemester_runtime::backup_keys::timestamp(at).map_err(|_| invalid())?;
                if guard.original716_signing_key(false).is_err() { return Ok(json!({"ok":false,"errors":["no existing ed25519 keypair to rotate — run 'system backup --sign-with-ed25519' first to bootstrap one"]})); }
                effect = true;
                let result = guard.original716_rotate_key(&at).map_err(|_| invalid())?;
                rentemester_storage::original716::custody::audit_rotation(l,actor,&at,reason,result["oldPublicKeyHint"].as_str().ok_or_else(invalid)?,result["newPublicKeyHint"].as_str().ok_or_else(invalid)?)?;
                Ok(result)
            });
            if effect { if let Err(f) = &mut result { f.stage = rentemester_application::control_fence::FailureStage::OutcomeUnknown; } }
            result
        })
    }
    /// Only public PEM crosses this authenticated boundary; operator CLI owns --out.
    pub fn export_backup_public_key_original716(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
    ) -> Result<
        Attempt<Result<Committed<Value>, rentemester_application::control_fence::Failure>>,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c,l,guard,ctx| {
            rentemester_application::backup_original122::original717::human(c,l,ctx, |_,_,_| {
                let Some(pem) = guard.original716_public_key().map_err(|_| invalid())? else { return Ok(json!({"ok":false,"error":"no ed25519 public key found; run \"system backup --sign-with-ed25519\" first to generate the keypair"})); };
                let pem = String::from_utf8(pem).map_err(|_| invalid())?;
                Ok(json!({"ok":true,"publicKeyHint":rentemester_runtime::backup_artifact::original717::sha256(pem.trim().as_bytes())[..16],"publicKeyPem":pem}))
            })
        })
    }
}
impl Owner {
    /// Read-only operator filesystem verification; current authority is checked before any I/O.
    pub fn verify_backup_signature_original716(
        &mut self,
        cookies: &rentemester_runtime::auth::CookieVerifier,
        clock: &dyn Clock,
        cookie: &VerifiedCookie,
        operator: &rentemester_runtime::bootstrap::Operator,
        uid: u32,
        input: rentemester_application::original716::custody::Verify,
    ) -> AuthAttempt<Value> {
        let catalog = self.catalog()?;
        let root = self.root.clone();
        self.control_auth(cookies.workspace(), |control| {
            use rentemester_application::password_login::{Failure, FailureStage};
            let fail = |error| Failure { stage:FailureStage::NotCommitted,error,connection_reusable:true };
            let tx = control.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate).map_err(|_| fail(invalid()))?;
            rentemester_application::backup_original122::original717::restore_authorize(&tx,cookies,clock,cookie,&catalog,operator).map_err(fail)?;
            let value = rentemester_runtime::original716::custody::verify_local(std::path::Path::new(&input.backup_dir),input.public_key.as_deref().map(std::path::Path::new),input.verify_key.as_deref().map(std::path::Path::new),&root,uid)
                .unwrap_or_else(|message| json!({"ok":false,"algorithms":[],"trustLevel":"none","errors":[message]}));
            tx.rollback().map_err(|_| fail(invalid()))?;
            Ok(value)
        })
    }
}

fn refused(message: &str) -> Value {
    json!({"ok":false,"appliedRules":[rentemester_runtime::backup_artifact::original717::RULE],"errors":[message]})
}
/// One admitted SQLite snapshot, one inventory and one retention. No native backup is used.
fn create_signed(
    l: &rusqlite::Transaction<'_>,
    guard: &mut rentemester_runtime::namespace::NamespaceGuard,
    clock: &dyn Clock,
    actor: &str,
    input: rentemester_application::original716::custody::SignedBackup,
    operator: Option<&rentemester_runtime::bootstrap::Operator>,
    effect: &mut bool,
) -> Result<Value, rentemester_engine::Error> {
    use rentemester_application::backup_original122::original717 as original;
    use rentemester_runtime::backup_artifact::original717 as codec;
    use rusqlite::MAIN_DB;
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
    if count
        .checked_mul(size)
        .is_none_or(|n| n <= 0 || n as usize > rentemester_runtime::backup_artifact::MAX_FILE_BYTES)
    {
        return Err(invalid());
    }
    let signing = prepare_signed_backup(guard, effect)?;
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
    let proof =
        codec::seal_original716(files, manifest, &key, Some(&signing)).map_err(|_| invalid())?;
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
impl Owner {
    pub fn signed_backup_original716(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        input: rentemester_application::original716::custody::SignedBackup,
        operator: Option<&rentemester_runtime::bootstrap::Operator>,
    ) -> Result<
        Attempt<Result<Committed<Value>, rentemester_application::control_fence::Failure>>,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, guard, ctx| {
            let mut effect = false;
            let mut result = rentemester_application::backup_original122::original717::human(
                c,
                l,
                ctx,
                |_, l, actor| {
                    create_signed(
                        l,
                        guard,
                        clock,
                        actor,
                        input,
                        operator.filter(|o| o.actor() == format!("user:{actor}")),
                        &mut effect,
                    )
                },
            );
            if effect {
                if let Err(f) = &mut result {
                    f.stage = rentemester_application::control_fence::FailureStage::OutcomeUnknown;
                }
            }
            result
        })
    }
}
