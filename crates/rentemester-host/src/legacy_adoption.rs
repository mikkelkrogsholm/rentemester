//! Operator-only reviewed legacy capture, monotonic retirement and fresh adoption.
//! No source identity, provider configuration, or native-backup claim is imported.
use super::*;
use rentemester_engine::Error;
use rentemester_runtime::{bootstrap::Operator, legacy_adoption as runtime};
use rentemester_storage::{
    legacy_adoption as company, legacy_control_adoption as control,
    legacy_retirement as retirement, portable_backup as business,
};
use rusqlite::{MAIN_DB, TransactionBehavior};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

fn refused(_: impl std::fmt::Display) -> Error {
    Error::new(
        "legacy_adoption_refused",
        "legacy capture, review, retirement or admission refused",
    )
}
fn digest(bytes: &[u8]) -> String {
    rentemester_runtime::backup_keys::digest(bytes)
}
fn encode(value: &Value) -> Result<Vec<u8>, Error> {
    serde_json::to_vec(value).map_err(refused)
}
fn image_db(bytes: &[u8]) -> Result<Connection, Error> {
    let mut db = Connection::open_in_memory().map_err(refused)?;
    db.deserialize_read_exact(MAIN_DB, bytes, bytes.len(), true)
        .map_err(refused)?;
    db.execute_batch("PRAGMA trusted_schema=OFF;PRAGMA foreign_keys=ON;PRAGMA query_only=ON")
        .map_err(refused)?;
    Ok(db)
}
struct Database {
    relative: String,
    logical_digest: String,
}
/// Non-Clone: converted random IDs/timestamps and all original bytes remain the
/// exact reviewed capture throughout retirement and publication.
pub struct Prepared {
    source: PathBuf,
    target: PathBuf,
    operation_id: String,
    source_originals: BTreeMap<String, String>,
    workspace: String,
    selected: String,
    source_workspace_id: String,
    manifest: Vec<u8>,
    control: control::ConvertedControlBusiness,
    files: Vec<runtime::AdoptionFile>,
    databases: Vec<Database>,
    raw_files: Vec<Value>,
    review: Value,
    review_hash: String,
}
pub struct Retired {
    prepared: Prepared,
    binding: Value,
    raw_files: Vec<Value>,
    review: Value,
    review_hash: String,
}
impl Prepared {
    pub fn review(&self) -> &Value {
        &self.review
    }
    pub fn review_hash(&self) -> &str {
        &self.review_hash
    }
    pub fn source_workspace_id(&self) -> &str {
        &self.source_workspace_id
    }
    pub fn expected_operation_id(&self) -> &str {
        &self.operation_id
    }
    pub fn target(&self) -> &Path {
        &self.target
    }
}
impl Retired {
    pub fn review(&self) -> &Value {
        &self.review
    }
    pub fn review_hash(&self) -> &str {
        &self.review_hash
    }
    pub fn workspace(&self) -> &str {
        &self.prepared.workspace
    }
}
#[derive(Debug)]
pub struct RetirementFailure {
    pub error: Error,
    /// Earlier per-database commits survive a later refusal. Never infer rollback.
    pub progress: Vec<Value>,
}
impl std::fmt::Display for RetirementFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.error.code)
    }
}
impl std::error::Error for RetirementFailure {}

fn source_bytes(root: &Path, relative: &str, limit: usize) -> Result<Vec<u8>, Error> {
    runtime::read_source_file(root, relative, limit).map_err(refused)
}
fn raw_files(root: &Path, databases: &[Database]) -> Result<Vec<Value>, Error> {
    let mut files = Vec::new();
    for db in databases {
        // SHM is lock metadata and can legitimately change on read admission.
        for suffix in ["", "-wal", "-journal"] {
            let path = format!("{}{suffix}", db.relative);
            match root.join(&path).symlink_metadata() {
                Ok(_) => {
                    let bytes = source_bytes(root, &path, 512 * 1024 * 1024)?;
                    files.push(json!({"path":path,"bytes":bytes.len(),"sha256":digest(&bytes)}));
                }
                Err(e) if !suffix.is_empty() && e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(refused(e)),
            }
        }
    }
    Ok(files)
}
fn company_pending(db: &Connection) -> Result<(), Error> {
    // Schema and histories were fully verified by the company converter first.
    let pending:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM peppol_submissions p WHERE COALESCE((SELECT e.event_type FROM peppol_submission_events e WHERE e.submission_id=p.id ORDER BY e.id DESC LIMIT 1),'')='delivery_reserved' OR COALESCE((SELECT e.status FROM peppol_submission_events e WHERE e.submission_id=p.id ORDER BY e.id DESC LIMIT 1),p.status) IN ('uncertain','delivery-uncertain','prepared')) OR EXISTS(SELECT 1 FROM recurring_invoice_delivery_events e WHERE e.id=(SELECT MAX(x.id) FROM recurring_invoice_delivery_events x WHERE x.generation_id=e.generation_id) AND e.event_type IN ('attempted','uncertain')) OR EXISTS(SELECT 1 FROM invoice_extraction_attempts a WHERE COALESCE((SELECT json_extract(r.resolution_json,'$.status') FROM invoice_extraction_resolutions r WHERE r.attempt_id=a.id AND r.field_key='extraction_status' ORDER BY r.id DESC LIMIT 1),'') NOT IN ('completed','needs_resolution'))",[],|r|r.get(0)).map_err(refused)?;
    if pending {
        return Err(Error::new(
            "legacy_external_intent_pending",
            "Resolve ambiguous Peppol, recurring delivery or extraction work before retirement",
        ));
    }
    let email:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM audit_log a WHERE a.event_type IN ('native_invoice_email_reserved','native_invoice_email_unknown') AND a.id=(SELECT MAX(x.id) FROM audit_log x WHERE x.entity_type='native_invoice_email_v1' AND x.entity_id=a.entity_id))",[],|r|r.get(0)).map_err(refused)?;
    if email {
        return Err(Error::new(
            "legacy_external_intent_pending",
            "Resolve reserved native invoice email before retirement",
        ));
    }
    Ok(())
}
fn check_originals(
    db: &Connection,
    slug: &str,
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<(), Error> {
    let original = |p: &str| {
        files
            .get(&format!("{slug}/{p}"))
            .map(Vec::as_slice)
            .ok_or_else(|| refused("missing original"))
    };
    for s in rentemester_storage::invoice_email::sources(db)? {
        rentemester_application::invoice_email::verify_source_bytes(
            db,
            &s,
            original(&s.json_path)?,
            original(&s.pdf_path)?,
        )?;
    }
    for s in rentemester_storage::invoice_extraction::sources(db)? {
        let p: String = db
            .query_row(
                "SELECT stored_path FROM documents WHERE id=?1",
                [s.document_id],
                |r| r.get(0),
            )
            .map_err(refused)?;
        if original(&p)? != s.pdf_bytes {
            return Err(refused("extraction original"));
        }
    }
    for r in rentemester_storage::archive_import::originals(db)? {
        rentemester_storage::archive_import::verify_original(db, &r, original(&r.path)?)?;
    }
    Ok(())
}
/// Source inspection confers no retirement or destination writer authority.
pub fn prepare(
    source: &Path,
    target: &Path,
    workspace: &str,
    selected: &str,
) -> Result<Prepared, Error> {
    if source.to_str().is_none() || source.canonicalize().map_err(refused)? != source {
        return Err(refused("canonical source required"));
    }
    if !target.is_absolute()
        || target.to_str().is_none()
        || target.components().any(|c| {
            !matches!(
                c,
                std::path::Component::RootDir | std::path::Component::Normal(_)
            )
        })
        || target.starts_with(source)
        || target
            .parent()
            .ok_or_else(|| refused("destination parent"))?
            .canonicalize()
            .map_err(refused)?
            != target.parent().unwrap()
    {
        return Err(refused("exact canonical destination required"));
    }
    match target.symlink_metadata() {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        _ => return Err(refused("fresh destination required")),
    }
    let manifest = source_bytes(source, "workspace.json", runtime::MAX_METADATA_BYTES)?;
    use std::os::unix::ffi::OsStrExt;
    let mut source_identity = source.as_os_str().as_bytes().to_vec();
    source_identity.push(0);
    source_identity.extend_from_slice(&manifest);
    let source_workspace_id = digest(&source_identity);
    let mut operation_bytes = b"rentemester-legacy-adoption-destination/1\0".to_vec();
    operation_bytes.extend_from_slice(source_workspace_id.as_bytes());
    operation_bytes.push(0);
    operation_bytes.extend_from_slice(target.as_os_str().as_bytes());
    let operation_id = digest(&operation_bytes);

    let value: Value = serde_json::from_slice(&manifest).map_err(refused)?;
    let companies = value["companies"]
        .as_array()
        .ok_or_else(|| refused("manifest"))?;
    if companies.is_empty() || companies.len() > 64 {
        return Err(refused("company bound"));
    }
    let slugs = companies
        .iter()
        .map(|c| {
            c["slug"]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| refused("slug"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    // Validate before any slug can select a source path.
    let mut seen = BTreeSet::new();
    for slug in &slugs {
        if slug.is_empty()
            || slug.len() > 100
            || !slug.as_bytes()[0].is_ascii_alphanumeric()
            || !slug
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
            || !seen.insert(slug)
        {
            return Err(refused("manifest slug"));
        }
    }
    let mut databases = vec![Database {
        relative: ".rentemester/workspace-control.sqlite".into(),
        logical_digest: String::new(),
    }];
    databases.extend(slugs.iter().map(|slug| Database {
        relative: format!("{slug}/data/ledger.sqlite"),
        logical_digest: String::new(),
    }));
    let before = raw_files(source, &databases)?;
    let control = control::convert_control_for_adoption(source)?;
    databases[0].logical_digest = control.source_logical_digest().into();
    let mut files = BTreeMap::<String, runtime::AdoptionFile>::new();
    let mut reports = Vec::new();
    let mut source_originals = BTreeMap::new();
    let mut uuids = BTreeSet::new();
    let mut original_total = 0usize;
    for (i, slug) in slugs.iter().enumerate() {
        let converted = company::convert_company_for_adoption(&source.join(slug))?;
        let report = serde_json::to_value(converted.report()).map_err(refused)?;
        databases[i + 1].logical_digest = report["source_logical_digest"]
            .as_str()
            .ok_or_else(|| refused("logical source digest"))?
            .into();
        let mappings = converted
            .path_mappings()
            .iter()
            .map(|m| {
                (
                    m.target_relative_path().to_owned(),
                    m.source_relative_path().to_owned(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let mapping_report=converted.path_mappings().iter().map(|m|json!({"documentId":m.document_id(),"originalStoredPath":m.original_stored_path(),"sourceRelativePath":m.source_relative_path(),"targetRelativePath":m.target_relative_path(),"sha256":m.sha256()})).collect::<Vec<_>>();
        let inventory = converted.inventory();
        if !uuids.insert(inventory.ledger_uuid().to_owned()) {
            return Err(refused("duplicate ledger UUID"));
        }
        let db = image_db(converted.bytes())?;
        company_pending(&db)?;
        for (relative, sha) in inventory.originals().iter().cloned().chain(
            inventory
                .bank_hashes()
                .iter()
                .map(|h| (format!("bank/processed/{h}.txt"), h.clone())),
        ) {
            let full = format!("{slug}/{relative}");
            let source_relative = mappings
                .get(&relative)
                .map(String::as_str)
                .unwrap_or(&relative);
            let source_full = format!("{slug}/{source_relative}");
            if files.contains_key(&full) {
                continue;
            }
            let bytes = source_bytes(source, &source_full, runtime::MAX_FILE_BYTES)?;
            source_originals.insert(full.clone(), source_full);
            if digest(&bytes) != sha {
                return Err(refused("original hash"));
            }
            original_total = original_total
                .checked_add(bytes.len())
                .ok_or_else(|| refused("bytes bound"))?;
            if original_total > runtime::MAX_TOTAL_BYTES {
                return Err(refused("bytes bound"));
            }
            let kind = if relative.starts_with("bank/processed/") {
                runtime::AdoptionFileKind::BankStatement {
                    company: slug.clone(),
                }
            } else {
                runtime::AdoptionFileKind::DocumentOriginal {
                    company: slug.clone(),
                }
            };
            files.insert(
                full.clone(),
                runtime::AdoptionFile {
                    kind,
                    path: full,
                    bytes,
                    sha256: sha,
                },
            );
        }
        let originals = files
            .iter()
            .map(|(p, f)| (p.clone(), f.bytes.clone()))
            .collect();
        check_originals(&db, slug, &originals)?;
        db.close().map_err(|(_, e)| refused(e))?;
        reports.push(json!({"slug":slug,"ledgerUuid":inventory.ledger_uuid(),"conversion":report,"sourceFiles":converted.source_files(),"inventory":inventory,"pathMappings":mapping_report}));
        let bytes = converted.into_bytes();
        original_total = original_total
            .checked_add(bytes.len())
            .ok_or_else(|| refused("bytes bound"))?;
        if bytes.len() > runtime::MAX_FILE_BYTES
            || original_total > runtime::MAX_TOTAL_BYTES
            || files.len() >= runtime::MAX_FILES
        {
            return Err(refused("image bound"));
        }
        let path = format!("{slug}/data/ledger.sqlite");
        files.insert(
            path.clone(),
            runtime::AdoptionFile {
                kind: runtime::AdoptionFileKind::CompanyLedger {
                    company: slug.clone(),
                },
                path,
                sha256: digest(&bytes),
                bytes,
            },
        );
    }
    let after = raw_files(source, &databases)?;
    if before != after
        || source_bytes(source, "workspace.json", runtime::MAX_METADATA_BYTES)? != manifest
    {
        return Err(refused("source changed during capture"));
    }
    let files = files.into_values().collect::<Vec<_>>();
    let mut validation = Connection::open_in_memory().map_err(refused)?;
    control.initialize_new_control(
        &mut validation,
        &slugs,
        &"0".repeat(64),
        "system:legacy-adoption-review",
    )?;
    let tx = validation
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .map_err(refused)?;
    for file in &files {
        if let runtime::AdoptionFileKind::CompanyLedger { company: slug } = &file.kind {
            let db = image_db(&file.bytes)?;
            business::validate_inbox_company(&tx, &db, slug)?;
            let facts = business::company(&db)?;
            super::intercompany_dispositions::validate_historical_company(
                &tx,
                &db,
                slug,
                &facts.uuid,
            )?;
            db.close().map_err(|(_, e)| refused(e))?;
        }
    }
    tx.rollback().map_err(refused)?;
    validation.close().map_err(|(_, e)| refused(e))?;
    let review = json!({"format":"rentemester-legacy-adoption-review/1","sourceRoot":source,"destination":target,"expectedOperationId":operation_id,"sourceWorkspaceId":source_workspace_id,"selectedCompany":selected,"sourceManifestSha256":digest(&manifest),"sourceFiles":before,"companies":reports,"control":{"fromVersion":control.from_version(),"toVersion":control.to_version(),"businessSha256":control.payload_sha256(),"sourceLogicalDigest":control.source_logical_digest()},"excludedAuthority":["credentials","sessions","MFA","API keys","memberships","bootstrap grants","provider configuration"],"writerAuthority":false});
    let review_hash = digest(&encode(&review)?);
    // Runtime shape/path/total-byte validation, including archived companies.
    runtime::AdoptionImage::validate(
        workspace,
        selected,
        manifest.clone(),
        files.clone(),
        encode(&review)?,
        &review_hash,
    )
    .map_err(refused)?;
    let prepared = Prepared {
        source: source.into(),
        target: target.into(),
        operation_id,
        source_originals,
        workspace: workspace.into(),
        selected: selected.into(),
        source_workspace_id,
        manifest,
        control,
        files,
        databases,
        raw_files: before,
        review,
        review_hash,
    };
    // Bind uncoded source provider-operation checks to the captured control digest.
    runtime::with_source_database(source, &prepared.databases[0].relative, |db| {
        if retirement::source_digest(db)? != prepared.databases[0].logical_digest {
            return Err(refused("control changed"));
        }
        control_pending(db)
    })?;
    Ok(prepared)
}
fn control_pending(db: &Connection) -> Result<(), Error> {
    let now = rentemester_runtime::auth::SystemClock
        .now_unix_ms()
        .map_err(refused)?;
    rentemester_storage::service_lifecycle::validate_adoption_retirement(db, now)
}
fn recheck(p: &Prepared, binding: Option<&Value>, raw: Option<&[Value]>) -> Result<(), Error> {
    if source_bytes(&p.source, "workspace.json", runtime::MAX_METADATA_BYTES)? != p.manifest {
        return Err(refused("manifest changed"));
    }
    for file in &p.files {
        if !matches!(file.kind, runtime::AdoptionFileKind::CompanyLedger { .. })
            && source_bytes(
                &p.source,
                p.source_originals
                    .get(&file.path)
                    .ok_or_else(|| refused("source original mapping"))?,
                runtime::MAX_FILE_BYTES,
            )? != file.bytes
        {
            return Err(refused("original changed"));
        }
    }
    for source in &p.databases {
        runtime::with_source_database(&p.source, &source.relative, |db| {
            if retirement::source_digest(db)? != source.logical_digest {
                return Err(refused("logical source changed"));
            }
            if let Some(expected) = binding {
                let actual = retirement::verify_overlay(db)?
                    .map(serde_json::to_value)
                    .transpose()
                    .map_err(refused)?;
                if actual.as_ref() != Some(expected) {
                    return Err(refused("retirement binding changed"));
                }
            }
            Ok(())
        })?;
    }
    if let Some(expected) = raw {
        if raw_files(&p.source, &p.databases)? != expected {
            return Err(refused("raw source changed"));
        }
    }
    Ok(())
}
/// No workspace-wide atomicity is claimed: completed per-DB fences are permanent
/// on a later failure. Offline acknowledgement covers unlogged TS mail/PDF work;
/// SQL facts never claim that an external process/network request was drained.
pub fn retire_source(
    p: Prepared,
    operator: &Operator,
    binding: &retirement::RetirementBinding,
    expected_review_hash: &str,
    external_workers_stopped: bool,
) -> Result<Retired, RetirementFailure> {
    let mut progress = Vec::new();
    let result = (|| {
        binding.validate()?;
        if p.review_hash != expected_review_hash
            || !external_workers_stopped
            || !binding.source_stopped_confirmed
            || binding.source_workspace_id != p.source_workspace_id
            || binding.operation_id != p.operation_id
            || binding.operator_actor != operator.actor()
            || operator.workspace() != p.workspace
        {
            return Err(refused("retirement confirmation"));
        }
        recheck(&p, None, Some(&p.raw_files))?;
        for source in &p.databases {
            let installed = runtime::with_source_database(&p.source, &source.relative, |db| {
                retirement::retire_owned_checked(db, binding, &source.logical_digest)
            })?;
            progress.push(json!({"path":source.relative,"installed":installed,"operationId":binding.operation_id}));
        }
        let binding = serde_json::to_value(binding).map_err(refused)?;
        recheck(&p, Some(&binding), None)?;
        let raw_files = raw_files(&p.source, &p.databases)?;
        let review = json!({"format":"rentemester-retired-legacy-adoption-review/1","preparedReviewHash":p.review_hash,"preparedReview":p.review,"retirement":binding,"retiredSourceFiles":raw_files,"externalWorkersStoppedAcknowledged":true,"retirementProgress":progress,"writerAuthority":false});
        let review_hash = digest(&encode(&review)?);
        Ok(Retired {
            prepared: p,
            binding,
            raw_files,
            review,
            review_hash,
        })
    })();
    result.map_err(|error| RetirementFailure { error, progress })
}
/// Consumes the exact reviewed retired capture. Never reconverts after review.
pub fn adopt(
    retired: Retired,
    target: &Path,
    uid: u32,
    profile: NamespaceProfile,
    operator: &Operator,
    expected_review_hash: &str,
) -> Result<Owner, OwnerError> {
    let p = retired.prepared;
    if retired.review_hash != expected_review_hash
        || operator.workspace() != p.workspace
        || retired.binding["operator_actor"] != operator.actor()
        || target.symlink_metadata().is_ok()
        || target != p.target
    {
        return Err(OwnerError::Context);
    }
    recheck(&p, Some(&retired.binding), Some(&retired.raw_files))
        .map_err(|_| OwnerError::Context)?;
    let receipt = json!({"format":"rentemester-legacy-adoption/1","workspace":p.workspace,"target":target,"selectedCompany":p.selected,"reviewHash":retired.review_hash,"review":retired.review,"operatorActor":operator.actor()});
    let receipt = encode(&receipt).map_err(|_| OwnerError::Context)?;
    let receipt_hash = digest(&receipt);
    let image = runtime::AdoptionImage::validate(
        &p.workspace,
        &p.selected,
        p.manifest,
        p.files,
        receipt.clone(),
        &receipt_hash,
    )
    .map_err(|_| OwnerError::Context)?;
    let parent = target.parent().ok_or(OwnerError::Context)?;
    let token = rentemester_runtime::auth::new_token().map_err(|_| OwnerError::Context)?;
    let stage = parent.join(format!(".rentemester-adoption-{}", token.as_str()));
    let guard = NamespaceGuard::create_adoption_stage(
        &stage,
        uid,
        &p.workspace,
        &p.selected,
        profile,
        &image,
    )
    .map_err(OwnerError::Namespace)?;
    let mut owner = Owner::open_guarded(&stage, &p.workspace, &p.selected, guard)?;
    owner.reusable = false;
    let slots = owner.held.0.as_mut().unwrap();
    p.control
        .initialize_new_control(
            slots.control.as_mut().unwrap(),
            image.companies(),
            &receipt_hash,
            operator.actor(),
        )
        .map_err(|_| OwnerError::Database)?;
    let tx = slots
        .control
        .as_mut()
        .unwrap()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| OwnerError::Database)?;
    let receipt_text = std::str::from_utf8(&receipt).map_err(|_| OwnerError::Database)?;
    tx.execute("INSERT INTO workspace_audit(event_type,entity_type,entity_id,actor)VALUES('legacy_adoption_receipt','legacy_adoption',?1,?2)",rusqlite::params![receipt_text,operator.actor()]).map_err(|_|OwnerError::Database)?;
    tx.commit().map_err(|_| OwnerError::Database)?;
    if !slots.close() {
        return Err(OwnerError::Database);
    }
    // Borrow source metadata without recapturing/reconstructing converted bytes.
    // The image owns files now; original source hashes remain in the reviewed evidence.
    recheck_image_source(
        &p.source,
        &p.databases,
        &p.source_originals,
        &retired.binding,
        &retired.raw_files,
        &image,
    )
    .map_err(|_| OwnerError::Context)?;
    slots
        .guard
        .publish_adopted_root(target)
        .map_err(OwnerError::Namespace)?;
    let guard = owner.held.0.take().unwrap().guard;
    Owner::open_guarded(target, &p.workspace, &p.selected, guard)
}
fn recheck_image_source(
    source: &Path,
    databases: &[Database],
    source_originals: &BTreeMap<String, String>,
    binding: &Value,
    raw: &[Value],
    image: &runtime::AdoptionImage,
) -> Result<(), Error> {
    if source_bytes(source, "workspace.json", runtime::MAX_METADATA_BYTES)?
        != image.manifest_bytes()
    {
        return Err(refused("manifest changed"));
    }
    for file in image.files() {
        if !matches!(file.kind, runtime::AdoptionFileKind::CompanyLedger { .. })
            && source_bytes(
                source,
                source_originals
                    .get(&file.path)
                    .ok_or_else(|| refused("source original mapping"))?,
                runtime::MAX_FILE_BYTES,
            )? != file.bytes
        {
            return Err(refused("original changed"));
        }
    }
    for d in databases {
        runtime::with_source_database(source, &d.relative, |db| {
            let actual = retirement::verify_overlay(db)?
                .map(serde_json::to_value)
                .transpose()
                .map_err(refused)?;
            if actual.as_ref() != Some(binding)
                || retirement::source_digest(db)? != d.logical_digest
            {
                return Err(refused("source fence or digest changed"));
            }
            Ok(())
        })?;
    }
    if raw_files(source, databases)? != raw {
        return Err(refused("raw source changed"));
    }
    Ok(())
}

impl Owner {
    /// Detect audited adoption lineage on ordinary startup, including after a
    /// native backup restore. Any receipt event selects strict grant validation;
    /// malformed rows and SQL failures must never be swallowed as "not adopted".
    pub fn has_legacy_adoption(&mut self) -> Result<bool, OwnerError> {
        self.admit()?;
        let result=self.held.0.as_mut().unwrap().control.as_ref().unwrap().query_row(
            "SELECT EXISTS(SELECT 1 FROM workspace_audit WHERE event_type='legacy_adoption_receipt')",
            [],|r|r.get(0)).map_err(|_|OwnerError::Database);
        let retired = self.finish(result.is_ok());
        if retired && result.is_ok() {
            return Err(OwnerError::Retired);
        }
        result
    }
    /// The existing first-owner saga grants its selected company. This narrowly
    /// extends that same fresh identity to the exact adopted catalog; old grants
    /// and a convenient receipt file confer no authority.
    pub fn grant_adopted_first_owner(
        &mut self,
        operator: &Operator,
        reservation: &rentemester_application::bootstrap::Reservation,
    ) -> Result<(), OwnerError> {
        self.admit_auth(operator.workspace())?;
        if reservation.phase != rentemester_application::bootstrap::Phase::AccessCompleted {
            return Err(OwnerError::Context);
        }
        let slots = self.held.0.as_mut().unwrap();
        let catalog = slots.guard.catalog().map_err(OwnerError::Namespace)?;
        let tx = slots
            .control
            .as_mut()
            .unwrap()
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| OwnerError::Database)?;
        let mut q=tx.prepare("SELECT entity_id,actor FROM workspace_audit WHERE event_type='legacy_adoption_receipt' AND entity_type='legacy_adoption' ORDER BY id LIMIT 2").map_err(|_|OwnerError::Database)?;
        let rows = q
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(|_| OwnerError::Database)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| OwnerError::Database)?;
        drop(q);
        if rows.len() != 1 || rows[0].0.len() > runtime::MAX_METADATA_BYTES {
            return Err(OwnerError::Context);
        }
        let (raw, actor) = &rows[0];
        let receipt: Value = serde_json::from_str(raw).map_err(|_| OwnerError::Context)?;
        if encode(&receipt).map_err(|_| OwnerError::Context)? != raw.as_bytes()
            || receipt["format"] != "rentemester-legacy-adoption/1"
            || receipt["workspace"] != self.workspace
            || receipt["operatorActor"] != actor.as_str()
        {
            return Err(OwnerError::Context);
        }
        if receipt["reviewHash"]
            != digest(&encode(&receipt["review"]).map_err(|_| OwnerError::Context)?)
            || receipt["review"]["preparedReviewHash"]
                != digest(
                    &encode(&receipt["review"]["preparedReview"])
                        .map_err(|_| OwnerError::Context)?,
                )
        {
            return Err(OwnerError::Context);
        }
        let expected = digest(raw.as_bytes());
        let hashes: Vec<(String, String)> = {
            let mut q=tx.prepare("SELECT entity_id,actor FROM workspace_audit WHERE event_type='legacy_control_adoption' AND entity_type='legacy_adoption' ORDER BY id LIMIT 2").map_err(|_|OwnerError::Database)?;
            q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .map_err(|_| OwnerError::Database)?
                .collect::<Result<_, _>>()
                .map_err(|_| OwnerError::Database)?
        };
        if hashes != vec![(expected, actor.clone())] {
            return Err(OwnerError::Context);
        }
        let selected = receipt["selectedCompany"]
            .as_str()
            .ok_or(OwnerError::Context)?;
        let boot:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM rm_workspace_bootstrap_events WHERE reservation_hash=?1 AND event_type='access_completed' AND user_id=?2 AND company_slug=?3)",rusqlite::params![reservation.reservation_hash,reservation.user_id,selected],|r|r.get(0)).map_err(|_|OwnerError::Database)?;
        if !boot || reservation.company_slug != selected {
            return Err(OwnerError::Context);
        }
        let company_rows = receipt["review"]["preparedReview"]["companies"]
            .as_array()
            .ok_or(OwnerError::Context)?;
        let registered = catalog.into_iter().map(|c| c.0).collect::<Vec<_>>();
        let receipt_slugs = company_rows
            .iter()
            .map(|c| {
                c["slug"]
                    .as_str()
                    .map(str::to_owned)
                    .ok_or(OwnerError::Context)
            })
            .collect::<Result<BTreeSet<_>, _>>()?;
        if receipt_slugs.len() != company_rows.len()
            || receipt_slugs != registered.iter().cloned().collect()
        {
            return Err(OwnerError::Context);
        }
        business::grant_restored_owner(
            &tx,
            reservation.user_id.as_deref().ok_or(OwnerError::Context)?,
            &registered,
            operator.actor(),
        )
        .map_err(|_| OwnerError::Database)?;
        tx.commit().map_err(|_| OwnerError::Database)?;
        self.observe()?;
        self.reusable = true;
        Ok(())
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use rentemester_engine::{fiscal_year::Date, manual_journal::ManualJournal};
    use std::os::unix::fs::PermissionsExt;
    struct Fixture {
        _dir: tempfile::TempDir,
        source: PathBuf,
        target: PathBuf,
        original: String,
    }
    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let parent = dir.path().canonicalize().unwrap();
        let source = parent.join("source");
        for relative in [
            "",
            ".rentemester",
            "a",
            "a/data",
            "a/documents",
            "a/documents/originals",
            "b",
            "b/data",
        ] {
            std::fs::create_dir(source.join(relative)).unwrap();
            std::fs::set_permissions(
                source.join(relative),
                std::fs::Permissions::from_mode(0o700),
            )
            .unwrap();
        }
        std::fs::write(source.join("workspace.json"),br#"{"version":1,"companies":[{"slug":"a","name":"Synthetic active","createdAt":"2026-01-01","archived":false},{"slug":"b","name":"Synthetic archive","createdAt":"2026-01-01","archived":true}]}"#).unwrap();
        let mut control =
            Connection::open(source.join(".rentemester/workspace-control.sqlite")).unwrap();
        let mut db = Connection::open(source.join("a/data/ledger.sqlite")).unwrap();
        rentemester_storage::fresh::initialize_empty_pair(
            &mut control,
            &mut db,
            "Synthetic active",
        )
        .unwrap();
        control.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('old','Synthetic prior','old@example.invalid',1,0,'2026-01-01','2026-01-01');INSERT INTO session(id,token,userId,createdAt,updatedAt,expiresAt)VALUES('prior','synthetic-secret-session','old','2026-01-01','2026-01-01','2099-01-01')").unwrap();
        db.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('2000','Synthetic bank','asset','debit'),('5000','Synthetic capital','equity','credit')").unwrap();
        let request:ManualJournal=serde_json::from_value(json!({"transactionDate":"2026-01-01","text":"Synthetic preserved capital","currency":"DKK","lines":[{"accountNo":"2000","debitAmount":"175.25"},{"accountNo":"5000","creditAmount":"175.25"}]})).unwrap();
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        rentemester_storage::journal_post::post(
            &tx,
            &request,
            &rentemester_storage::journal_preview::manual_journal_rule_version().unwrap(),
            Date::parse("2026-02-15").unwrap(),
            rentemester_storage::journal_post::Actor {
                subject: "user:synthetic",
                program: "synthetic-adoption-fixture",
            },
        )
        .unwrap();
        tx.commit().unwrap();
        let bytes = b"Synthetic immutable voucher original";
        let hash = digest(bytes);
        let original = format!("a/documents/originals/{hash}.txt");
        std::fs::write(source.join(&original), bytes).unwrap();
        db.execute("INSERT INTO documents(document_no,source,stored_path,mime_type,sha256_hash,document_type,status)VALUES('SYNTHETIC-ORIGINAL','synthetic',?1,'text/plain',?2,'internal_voucher','ingested')",rusqlite::params![format!("documents/originals/{hash}.txt"),hash]).unwrap();
        db.close().unwrap();
        control.close().unwrap();
        let mut extra_control = Connection::open_in_memory().unwrap();
        let mut archive = Connection::open(source.join("b/data/ledger.sqlite")).unwrap();
        rentemester_storage::fresh::initialize_empty_pair(
            &mut extra_control,
            &mut archive,
            "Synthetic archive",
        )
        .unwrap();
        archive.close().unwrap();
        Fixture {
            _dir: dir,
            source,
            target: parent.join("target"),
            original,
        }
    }
    fn operator() -> Operator {
        Operator::from_policy(
            "synthetic-native",
            "system:synthetic-installer",
            &["system:synthetic-installer".into()],
        )
        .unwrap()
    }
    fn binding(p: &Prepared) -> retirement::RetirementBinding {
        retirement::RetirementBinding {
            operation_id: p.expected_operation_id().into(),
            source_workspace_id: p.source_workspace_id().into(),
            operator_actor: operator().actor().into(),
            source_stopped_confirmed: true,
        }
    }
    fn retire(p: Prepared) -> Retired {
        let b = binding(&p);
        let hash = p.review_hash().to_owned();
        retire_source(p, &operator(), &b, &hash, true).unwrap()
    }
    #[test]
    fn reviewed_retirement_blocks_old_handle_and_adoption_preserves_complete_business_without_auth()
    {
        let f = fixture();
        let old = Connection::open(f.source.join("a/data/ledger.sqlite")).unwrap();
        let p = prepare(&f.source, &f.target, "synthetic-native", "a").unwrap();
        assert_eq!(p.review()["companies"].as_array().unwrap().len(), 2);
        let r = retire(p);
        assert!(old.execute("INSERT INTO audit_log(event_type,entity_type,actor)VALUES('late','test','user:old')",[]).is_err());
        old.close().unwrap();
        let hash = r.review_hash().to_owned();
        let uid = String::from_utf8(
            std::process::Command::new("id")
                .arg("-u")
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap()
        .trim()
        .parse()
        .unwrap();
        let owner = adopt(
            r,
            &f.target,
            uid,
            NamespaceProfile::LocalUserMacos,
            &operator(),
            &hash,
        )
        .unwrap();
        owner
            .try_close()
            .unwrap_or_else(|_| panic!("target descriptors must close"));
        assert_eq!(
            std::fs::read(f.target.join(&f.original)).unwrap(),
            b"Synthetic immutable voucher original"
        );
        let db = Connection::open(f.target.join("a/data/ledger.sqlite")).unwrap();
        assert_eq!(
            db.query_row::<i64, _, _>("SELECT count(*) FROM journal_entries", [], |r| r.get(0))
                .unwrap(),
            1
        );
        assert_eq!(
            db.query_row::<String, _, _>("SELECT entry_hash FROM journal_entries", [], |r| r
                .get(0))
                .unwrap()
                .len(),
            64
        );
        business::company(&db).unwrap();
        db.close().unwrap();
        let control =
            Connection::open(f.target.join(".rentemester/workspace-control.sqlite")).unwrap();
        for table in [
            "user",
            "session",
            "account",
            "twoFactor",
            "apikey",
            "rm_company_membership_events",
            "rm_workspace_bootstrap_events",
        ] {
            assert_eq!(
                control
                    .query_row::<i64, _, _>(&format!("SELECT count(*) FROM {table}"), [], |r| r
                        .get(0))
                    .unwrap(),
                0,
                "{table}"
            );
        }
        let raw: String = control
            .query_row(
                "SELECT entity_id FROM workspace_audit WHERE event_type='legacy_adoption_receipt'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let saved_hash: String = control
            .query_row(
                "SELECT entity_id FROM workspace_audit WHERE event_type='legacy_control_adoption'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(digest(raw.as_bytes()), saved_hash);
        assert_eq!(control.query_row::<i64,_,_>("SELECT count(*) FROM workspace_audit WHERE event_type='native_portable_restore'",[],|r|r.get(0)).unwrap(),0);
        control.close().unwrap();
        let owner = Owner::open_with_profile(
            &f.target,
            uid,
            "synthetic-native",
            "a",
            NamespaceProfile::LocalUserMacos,
        )
        .unwrap();
        owner
            .try_close()
            .unwrap_or_else(|_| panic!("reopened target closes"));
    }
    #[test]
    fn changed_capture_wrong_review_or_missing_offline_ack_never_retires_source() {
        for mode in ["changed", "review", "offline"] {
            let f = fixture();
            let p = prepare(&f.source, &f.target, "synthetic-native", "a").unwrap();
            let b = binding(&p);
            let hash = p.review_hash().to_owned();
            if mode == "changed" {
                std::fs::write(f.source.join(&f.original), b"changed source").unwrap();
            }
            let result = retire_source(
                p,
                &operator(),
                &b,
                if mode == "review" { "wrong" } else { &hash },
                mode != "offline",
            );
            assert!(result.is_err());
            assert!(result.err().unwrap().progress.is_empty());
            let db =
                Connection::open(f.source.join(".rentemester/workspace-control.sqlite")).unwrap();
            assert!(retirement::verify_overlay(&db).unwrap().is_none());
        }
    }
    #[test]
    fn existing_target_is_never_overwritten_and_unfenced_source_has_no_adoption_value() {
        let f = fixture();
        let p = prepare(&f.source, &f.target, "synthetic-native", "a").unwrap();
        assert!(p.control.retirement_binding().is_none());
        let r = retire(p);
        let hash = r.review_hash().to_owned();
        std::fs::create_dir(&f.target).unwrap();
        std::fs::write(f.target.join("preserve"), b"untouched").unwrap();
        assert!(
            adopt(
                r,
                &f.target,
                1,
                NamespaceProfile::LocalUserMacos,
                &operator(),
                &hash
            )
            .is_err()
        );
        assert_eq!(
            std::fs::read(f.target.join("preserve")).unwrap(),
            b"untouched"
        );
    }
}
