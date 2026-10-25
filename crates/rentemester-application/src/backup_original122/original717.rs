//! Original artifact operations under the existing current company/admin recovery fence.
use super::*;
use rentemester_runtime::namespace::NamespaceGuard;
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Create {
    pub confirm: bool,
    #[serde(default, deserialize_with = "present")]
    pub at: Option<String>,
    #[serde(default, deserialize_with = "present")]
    pub archive: Option<bool>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Archive {
    pub confirm: bool,
    #[serde(default, deserialize_with = "present")]
    pub backup_id: Option<String>,
    #[serde(default, deserialize_with = "present")]
    pub out: Option<String>,
}
pub enum Operation {
    Create(Create),
    Archive(Archive),
}
impl Operation {
    pub fn confirmed(&self) -> bool {
        match self {
            Self::Create(i) => i.confirm,
            Self::Archive(i) => i.confirm,
        }
    }
}
/// The callback is trusted Owner code and receives only an admitted company transaction.
/// Filesystem effects are tracked outside SQL so a later rollback never implies a safe retry.
pub fn human(
    c: &mut Connection,
    l: &mut Connection,
    ctx: Context<'_>,
    work: impl FnOnce(&Transaction<'_>, &Transaction<'_>, &str) -> Result<Value, Error>,
) -> Result<Committed<Value>, Failure> {
    control_fence::execute_cookie_both(
        c,
        l,
        CookieAuthorization {
            cookie: ctx.cookie,
            clock: ctx.clock,
            requirement: SessionRequirement::Current,
        },
        ctx.selection,
        Permission::CompanyAdmin,
        false,
        true,
        work,
    )
}
pub fn service(
    c: &mut Connection,
    l: &mut Connection,
    ctx: service_company::Context<'_>,
    work: impl FnOnce(&Transaction<'_>, &Transaction<'_>, &str) -> Result<Value, Error>,
) -> Result<service_company::ServiceResult<Value>, service_company::ServiceFailure> {
    service_company::execute_recovery_both(
        c,
        l,
        ctx,
        Permission::CompanyAdmin,
        |c, l, subject, _| {
            Ok(service_company::OperationResult {
                result: work(c, l, subject)?,
                receipt: None,
            })
        },
    )
}
pub fn pack(
    guard: &mut NamespaceGuard,
    input: Archive,
    operator: Option<&rentemester_runtime::bootstrap::Operator>,
    effect: &mut bool,
) -> Result<Value, Error> {
    if !input.confirm {
        return Ok(refusal("confirm must be true"));
    }
    let selected = input
        .backup_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let proof = match guard.load_original717(selected, None) {
        Ok(p) => p,
        Err(_) => {
            return Ok(refusal(&selected.map_or_else(
                || "no backup found to archive; run 'system backup' first".into(),
                |id| format!("backup not found or has no manifest: {id}"),
            )));
        }
    };
    archive(
        guard,
        &proof,
        input
            .out
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty()),
        operator,
        effect,
    )
}
pub fn archive(
    guard: &mut NamespaceGuard,
    proof: &rentemester_runtime::backup_artifact::original717::VerifiedOriginal717,
    out: Option<&str>,
    operator: Option<&rentemester_runtime::bootstrap::Operator>,
    effect: &mut bool,
) -> Result<Value, Error> {
    let bytes = rentemester_runtime::backup_artifact::original717::archive(proof.files())
        .map_err(|_| invalid())?;
    *effect = true;
    let path = guard
        .pack_original717(proof, out.map(std::path::Path::new), operator)
        .map_err(|_| invalid())?;
    Ok(
        json!({"ok":true,"backupId":proof.manifest().backup_id,"archivePath":path,"archiveSha256":rentemester_runtime::backup_artifact::original717::sha256(&bytes),"archiveSizeBytes":bytes.len(),"sha256Path":format!("{}.sha256",path.display()),"appliedRules":[RULE],"errors":[]}),
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Restore {
    pub backup_dir: String,
    pub target_company: String,
    #[serde(default, deserialize_with = "present")]
    pub verify_key: Option<String>,
    #[serde(default, deserialize_with = "present")]
    pub public_key: Option<String>,
    pub confirm: bool,
    #[serde(default, deserialize_with = "present")]
    pub confirm_text: Option<String>,
}
/// Restore has no company argument in the source. The local operator must independently hold
/// current workspace management and every company-admin grant; actorKind cannot supply this.
pub fn restore_authorize(
    tx: &Transaction<'_>,
    cookies: &rentemester_runtime::auth::CookieVerifier,
    clock: &dyn Clock,
    cookie: &rentemester_runtime::auth::VerifiedCookie,
    catalog: &rentemester_storage::workspace_context::Catalog,
    operator: &rentemester_runtime::bootstrap::Operator,
) -> Result<(), Error> {
    crate::portable_backup::authorize(tx, cookies, clock, cookie, catalog)?;
    let current = rentemester_storage::cookie_session::resolve(
        tx,
        cookie.token(),
        clock.now_unix_ms().map_err(|_| invalid())?,
        SessionRequirement::Current,
    )?;
    if operator.workspace() != cookies.workspace()
        || operator.actor() != format!("user:{}", current.subject)
    {
        return Err(Error::new(
            "access_denied",
            "Current local operator required",
        ));
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Place {
    pub archive_path: String,
    pub destination_id: String,
    #[serde(default, deserialize_with = "present")]
    pub actor_kind: Option<String>,
    #[serde(default, deserialize_with = "present")]
    pub at: Option<String>,
    #[serde(default, deserialize_with = "present")]
    pub note: Option<String>,
    pub confirm: bool,
}
pub fn place(
    c: &Transaction<'_>,
    l: &Transaction<'_>,
    guard: &mut NamespaceGuard,
    slug: &str,
    clock: &dyn Clock,
    subject: &str,
    input: Place,
    configs: &[rentemester_runtime::backup_destination::Config],
    uid: u32,
    canonical_root: &std::path::Path,
    effect: &mut bool,
) -> Result<Value, Error> {
    if !input.confirm {
        return Ok(refusal("confirm must be true"));
    }
    let kind = input.actor_kind.as_deref().unwrap_or("agent");
    if !matches!(kind, "agent" | "human") {
        return Ok(refusal("actorKind must be human or agent"));
    }
    let now = clock.now_unix_ms().map_err(|_| invalid())?;
    let at = match input.at.as_deref() {
        None => now,
        Some(s) => match rentemester_storage::backup_state::original122_moment(s) {
            Ok(n) => n,
            Err(_) => {
                return Ok(refusal(
                    "at must be a valid ISO-8601 datetime when provided",
                ));
            }
        },
    };
    let destinations = rentemester_storage::backup_registration::list(c, slug)?.0;
    let Some(destination) = destinations
        .iter()
        .find(|d| d["destinationId"] == input.destination_id)
    else {
        return Ok(refusal(&format!(
            "no backup destination with id: {}",
            input.destination_id
        )));
    };
    let Some(config) = configs
        .iter()
        .find(|d| destination["operatorTargetId"] == d.id)
    else {
        return Ok(refusal("destination has no admitted local operator target"));
    };
    let archive = std::path::Path::new(&input.archive_path);
    let Some(id) = archive.file_stem().and_then(|s| s.to_str()) else {
        return Ok(refusal("archivePath is invalid"));
    };
    if archive
        != canonical_root
            .join(slug)
            .join("backups")
            .join(format!("{id}.tar"))
    {
        return Ok(refusal(
            "archivePath must name this company's canonical retained archive",
        ));
    }
    let proof = match guard.load_original717(Some(id), None) {
        Ok(p) => p,
        Err(_) => {
            return Ok(refusal(
                "canonical backup directory failed manifest integrity verification",
            ));
        }
    };
    // Read-only authentication of the actual retained TAR and sidecar is required. Never regenerate
    // a missing archive here: placement cannot implicitly become a backup/archive operation.
    let bytes = rentemester_runtime::backup_artifact::original717::archive(proof.files())
        .map_err(|_| invalid())?;
    rentemester_runtime::backup_destination::verify_original717_archive(archive, uid, &bytes)
        .map_err(|_| invalid())?;
    *effect = true;
    rentemester_runtime::backup_destination::place_original717(config, uid, &proof)
        .map_err(|_| invalid())?;
    guard.ensure_current().map_err(|_| invalid())?;
    let placement = json!({"backupId":id,"archiveSha256":rentemester_runtime::backup_artifact::original717::sha256(&bytes),"archiveSizeBytes":bytes.len(),"placedAt":timestamp(at)?,"actor":format!("user:{subject}"),"actorKind":kind,"verified":true,"verifyMethod":"sha256-reread","note":input.note.map(|s|s.trim().to_owned()).filter(|s|!s.is_empty())});
    rentemester_storage::backup_state::original717::placement(
        l,
        &input.destination_id,
        &placement,
        &format!("user:{subject}"),
        &timestamp(now)?,
    )?;
    Ok(
        json!({"ok":true,"placement":placement,"copiedTo":config.directory.join(format!("{id}.tar")),"appliedRules":[RULE],"errors":[]}),
    )
}

#[derive(Deserialize, Clone)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Remote {
    pub destination_id: String,
    pub backup_id: String,
    pub remote_object_id: String,
    #[serde(default, deserialize_with = "present")]
    pub max_metadata_age_ms: Option<u64>,
    #[serde(default, deserialize_with = "present")]
    pub actor_kind: Option<String>,
    #[serde(default, deserialize_with = "present")]
    pub at: Option<String>,
    #[serde(default, deserialize_with = "present")]
    pub note: Option<String>,
    pub confirm: bool,
}
pub enum RemotePreparation {
    Complete(Value),
    Ready(PreparedRemote),
}
/// Carries exact authenticated company/source/destination identity across provider I/O.
pub struct PreparedRemote {
    pub expected: rentemester_runtime::backup_remote::Expected,
    input: Remote,
    subject: String,
    workspace: String,
    slug: String,
    boundary: i64,
    at: String,
    destination: Value,
}
pub fn prepare_remote(
    c: &Transaction<'_>,
    guard: &mut NamespaceGuard,
    workspace: &str,
    slug: &str,
    root: &std::path::Path,
    uid: u32,
    clock: &dyn Clock,
    subject: &str,
    input: Remote,
    bindings: &[crate::backup_remote::Binding],
) -> Result<RemotePreparation, Error> {
    let complete = |message: &str| Ok(RemotePreparation::Complete(refusal(message)));
    if !input.confirm {
        return complete("confirm must be true");
    }
    if input
        .actor_kind
        .as_deref()
        .is_some_and(|v| !matches!(v, "human" | "agent"))
    {
        return complete("actorKind must be human or agent");
    }
    if !rentemester_runtime::backup_remote::valid_object_id(&input.remote_object_id) {
        return complete("remoteObjectId is required");
    }
    let now = clock.now_unix_ms().map_err(|_| invalid())?;
    let at = match input.at.as_deref() {
        None => now,
        Some(s) => match rentemester_storage::backup_state::original122_moment(s) {
            Ok(n) => n,
            Err(_) => return complete("at must be a valid ISO-8601 datetime when provided"),
        },
    };
    if at > now.saturating_add(1000) {
        return complete("at must not be in the future");
    }
    let destinations = rentemester_storage::backup_registration::list(c, slug)?.0;
    let Some(destination) = destinations
        .iter()
        .find(|d| d["destinationId"] == input.destination_id)
    else {
        return complete(&format!(
            "no backup destination with id: {}",
            input.destination_id
        ));
    };
    if destination["kind"] != "google-drive" {
        return complete("remote verification requires a google-drive destination");
    }
    let Some(binding) = bindings
        .iter()
        .find(|b| destination["operatorTargetId"] == b.destination_id)
    else {
        return complete(
            "remote backup provider is not configured; declared evidence cannot be upgraded",
        );
    };
    let location = destination["location"].as_str().unwrap_or("");
    let expected_location = format!("google-drive://{}", binding.folder_id);
    let expected_url = format!(
        "https://drive.google.com/drive/folders/{}",
        binding.folder_id
    );
    if location != expected_location && location.trim_end_matches('/') != expected_url {
        return complete(
            "google-drive destination location does not match admitted provider folder",
        );
    }
    let proof = match guard.load_original717(Some(&input.backup_id), None) {
        Ok(p) => p,
        Err(_) => {
            return complete("canonical backup directory failed manifest integrity verification");
        }
    };
    let bytes = rentemester_runtime::backup_artifact::original717::archive(proof.files())
        .map_err(|_| invalid())?;
    let path = root
        .join(slug)
        .join("backups")
        .join(format!("{}.tar", input.backup_id));
    if rentemester_runtime::backup_destination::verify_original717_archive(&path, uid, &bytes)
        .is_err()
    {
        return complete(
            "canonical local backup archive is missing or checksum sidecar does not match archive",
        );
    }
    let expected = rentemester_runtime::backup_remote::Expected {
        object_id: input.remote_object_id.clone(),
        folder_id: binding.folder_id.clone(),
        file_name: format!("{}.tar", input.backup_id),
        archive_sha256: rentemester_runtime::backup_artifact::original717::sha256(&bytes),
        archive_size_bytes: bytes.len() as u64,
    };
    Ok(RemotePreparation::Ready(PreparedRemote {
        expected,
        input,
        subject: subject.into(),
        workspace: workspace.into(),
        slug: slug.into(),
        boundary: rentemester_storage::backup_state::boundary(c)?,
        at: timestamp(at)?,
        destination: destination.clone(),
    }))
}
pub fn finish_remote(
    c: &Transaction<'_>,
    l: &Transaction<'_>,
    guard: &mut NamespaceGuard,
    workspace: &str,
    slug: &str,
    root: &std::path::Path,
    uid: u32,
    clock: &dyn Clock,
    subject: &str,
    prepared: &PreparedRemote,
    observed: Result<&rentemester_runtime::backup_remote::Observation, ()>,
    bindings: &[crate::backup_remote::Binding],
) -> Result<Value, Error> {
    if prepared.subject != subject
        || prepared.workspace != workspace
        || prepared.slug != slug
        || prepared.boundary != rentemester_storage::backup_state::boundary(c)?
    {
        return Err(Error::new(
            "access_denied",
            "Remote verification authority changed",
        ));
    }
    let refreshed = prepare_remote(
        c,
        guard,
        workspace,
        slug,
        root,
        uid,
        clock,
        subject,
        prepared.input.clone(),
        bindings,
    )?;
    let RemotePreparation::Ready(current) = refreshed else {
        return Ok(refusal(
            "canonical backup or destination changed during remote verification",
        ));
    };
    if current.expected != prepared.expected || current.destination != prepared.destination {
        return Ok(refusal(
            "canonical backup or destination changed during remote verification",
        ));
    }
    let Ok(observed) = observed else {
        return Ok(refusal(
            "remote backup identity or content could not be verified",
        ));
    };
    let now = clock.now_unix_ms().map_err(|_| invalid())?;
    if observed.object_id != prepared.expected.object_id
        || observed.folder_id != prepared.expected.folder_id
        || observed.file_name != prepared.expected.file_name
        || observed.archive_sha256 != prepared.expected.archive_sha256
        || observed.archive_size_bytes != prepared.expected.archive_size_bytes
        || observed.observed_at_ms > now
        || now.saturating_sub(observed.observed_at_ms) as u64
            > prepared.input.max_metadata_age_ms.unwrap_or(300000)
    {
        return Ok(refusal(
            "remote backup metadata or content mismatch or stale observation",
        ));
    }
    let placement = json!({"backupId":prepared.input.backup_id,"archiveSha256":prepared.expected.archive_sha256,"archiveSizeBytes":prepared.expected.archive_size_bytes,"placedAt":prepared.at,"actor":format!("user:{subject}"),"actorKind":prepared.input.actor_kind.as_deref().unwrap_or("agent"),"verified":true,"verifyMethod":"remote-provider","remoteEvidence":{"provider":"google-drive","objectId":observed.object_id,"name":observed.file_name,"parentId":observed.folder_id,"sizeBytes":observed.archive_size_bytes,"checksumSha256":observed.archive_sha256,"metadataObservedAt":timestamp(observed.observed_at_ms)?,"verifiedAt":timestamp(observed.observed_at_ms)?},"note":prepared.input.note.as_deref().map(str::trim).filter(|s|!s.is_empty())});
    rentemester_storage::backup_state::original717::placement(
        l,
        &prepared.input.destination_id,
        &placement,
        &format!("user:{subject}"),
        &timestamp(now)?,
    )?;
    Ok(json!({"ok":true,"placement":placement,"appliedRules":[RULE],"errors":[]}))
}

pub fn human_prepare<T>(
    c: &mut Connection,
    l: &mut Connection,
    ctx: Context<'_>,
    work: impl FnOnce(&Transaction<'_>, &Transaction<'_>, &str) -> Result<T, Error>,
) -> Result<Committed<T>, Failure> {
    control_fence::execute_cookie_both(
        c,
        l,
        CookieAuthorization {
            cookie: ctx.cookie,
            clock: ctx.clock,
            requirement: SessionRequirement::Current,
        },
        ctx.selection,
        Permission::CompanyAdmin,
        false,
        true,
        work,
    )
}

pub fn service_prepare<T>(
    c: &mut Connection,
    l: &mut Connection,
    ctx: service_company::Context<'_>,
    work: impl FnOnce(&Transaction<'_>, &Transaction<'_>, &str) -> Result<T, Error>,
) -> Result<service_company::ServiceResult<T>, service_company::ServiceFailure> {
    service_company::execute_recovery_both(
        c,
        l,
        ctx,
        Permission::CompanyAdmin,
        |c, l, subject, _| {
            Ok(service_company::OperationResult {
                result: work(c, l, subject)?,
                receipt: None,
            })
        },
    )
}
