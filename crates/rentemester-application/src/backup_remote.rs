//! Current whole-workspace admission around a transaction-free remote observation.
use crate::password_login::{Failure, FailureStage};
use rentemester_engine::Error;
use rentemester_runtime::{
    auth::{Clock, CookieVerifier, VerifiedCookie},
    backup_destination as destination,
    backup_remote::{Expected, Observation},
};
use rentemester_storage::{backup_remote as store, workspace_context::Catalog};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde_json::{Value, json};

#[derive(Clone)]
pub struct Request {
    pub destination_id: String,
    pub operation_id: String,
    pub placement_operation_id: String,
    pub remote_object_id: String,
}
#[derive(Clone)]
pub struct Binding {
    pub destination_id: String,
    pub folder_id: String,
}
pub struct Runtime<'a> {
    pub local: &'a crate::backup_destination::Runtime<'a>,
    pub bindings: &'a [Binding],
}
pub enum Preparation {
    Complete(Value),
    Ready(Box<Prepared>),
}
/// Only `prepare` constructs this capability. No filesystem path, credential or caller-supplied proof escapes.
pub struct Prepared {
    pub expected: Expected,
    request: Request,
    local_receipt: Value,
    workspace: String,
    actor: String,
    restore_boundary: i64,
}
fn refused(code: &'static str) -> Error {
    Error::new(
        code,
        "Remote backup verification refused; inspect the exact operation ID before a new explicit observation",
    )
}
fn failure(error: Error, reusable: bool) -> Failure {
    Failure {
        stage: FailureStage::NotCommitted,
        error,
        connection_reusable: reusable,
    }
}
fn transaction<T>(
    control: &mut Connection,
    write: bool,
    work: impl FnOnce(&Transaction<'_>) -> Result<T, Error>,
) -> Result<T, Failure> {
    crate::control_fence::connection_ready(control).map_err(|error| failure(error, false))?;
    let tx = control
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| failure(refused("control_unavailable"), true))?;
    match work(&tx) {
        Ok(value) => {
            if write {
                tx.commit().map_err(|_| Failure {
                    stage: FailureStage::OutcomeUnknown,
                    error: refused("backup_remote_commit_unknown"),
                    connection_reusable: false,
                })?;
            } else {
                tx.rollback()
                    .map_err(|_| failure(refused("control_unavailable"), false))?;
            }
            Ok(value)
        }
        Err(error) => {
            let reusable = tx.rollback().is_ok();
            Err(failure(error, reusable))
        }
    }
}
fn authorize(
    tx: &Transaction<'_>,
    proof: &VerifiedCookie,
    catalog: &Catalog,
    cookies: &CookieVerifier,
    clock: &dyn Clock,
) -> Result<String, Error> {
    crate::portable_backup::authorize(tx, cookies, clock, proof, catalog)?;
    let now = clock
        .now_unix_ms()
        .map_err(|_| refused("clock_unavailable"))?;
    let current = rentemester_storage::cookie_session::resolve(
        tx,
        proof.token(),
        now,
        rentemester_storage::cookie_session::SessionRequirement::Current,
    )?;
    Ok(format!("user:{}", current.subject))
}
fn request_valid(input: &Request) -> Result<(), Error> {
    if !destination::valid_id(&input.destination_id)
        || !destination::valid_id(&input.operation_id)
        || !destination::valid_id(&input.placement_operation_id)
        || !rentemester_runtime::backup_remote::valid_object_id(&input.remote_object_id)
    {
        return Err(refused("backup_remote_invalid_input"));
    }
    Ok(())
}
fn completed(tx: &Transaction<'_>, request: &Request) -> Result<Option<Value>, Error> {
    let Some(receipt) = store::inspect(tx, &request.destination_id, &request.operation_id)? else {
        return Ok(None);
    };
    if receipt.placement_operation_id != request.placement_operation_id
        || receipt.remote_object_id != request.remote_object_id
    {
        return Err(refused("backup_remote_conflict"));
    }
    Ok(Some(json!({"ok":true,"verification":receipt})))
}
fn local(
    tx: &Transaction<'_>,
    catalog: &Catalog,
    runtime: &Runtime<'_>,
    request: &Request,
) -> Result<(Value, Expected), Error> {
    let mut configs = runtime
        .local
        .configs
        .iter()
        .filter(|config| config.id == request.destination_id);
    let config = configs
        .next()
        .ok_or_else(|| refused("backup_remote_destination_unavailable"))?;
    if configs.next().is_some() || config.kind != "google-drive" {
        return Err(refused("backup_remote_destination_unavailable"));
    }
    let mut bindings = runtime
        .bindings
        .iter()
        .filter(|binding| binding.destination_id == request.destination_id);
    let binding = bindings
        .next()
        .ok_or_else(|| refused("backup_remote_destination_unavailable"))?;
    if bindings.next().is_some()
        || !rentemester_runtime::backup_remote::valid_object_id(&binding.folder_id)
    {
        return Err(refused("backup_remote_destination_unavailable"));
    }
    let receipt = destination::verify(
        config,
        runtime.local.uid,
        &request.placement_operation_id,
        runtime.local.trusted,
        &catalog.workspace,
    )
    .map_err(|_| refused("backup_remote_local_unavailable"))?;
    // A matching private file alone cannot manufacture a successful placement history.
    let audit: Option<String> = tx.query_row("SELECT entity_id FROM workspace_audit WHERE event_type='native_backup_placed' AND entity_type='native_backup_placement' AND id>?1 AND json_valid(entity_id) AND json_extract(entity_id,'$.destinationId')=?2 AND json_extract(entity_id,'$.operationId')=?3 AND json_extract(entity_id,'$.archiveSha256')=?4 ORDER BY id DESC LIMIT 1", params![store::boundary(tx)?, request.destination_id, request.placement_operation_id, receipt["archiveSha256"].as_str().ok_or_else(|| refused("backup_remote_local_unavailable"))?], |row| row.get(0)).optional().map_err(|_| refused("backup_remote_local_unavailable"))?;
    let audit = audit
        .filter(|wire| wire.len() <= 16384)
        .ok_or_else(|| refused("backup_remote_placement_unconfirmed"))?;
    let audit: Value =
        serde_json::from_str(&audit).map_err(|_| refused("backup_remote_placement_unconfirmed"))?;
    for field in [
        "format",
        "destinationId",
        "operationId",
        "archiveSha256",
        "archiveSizeBytes",
        "backupCreatedAt",
        "verified",
        "signatureVerified",
        "verifyMethod",
    ] {
        if audit[field] != receipt[field] {
            return Err(refused("backup_remote_placement_unconfirmed"));
        }
    }
    if !audit["placedBy"]
        .as_str()
        .is_some_and(|actor| actor.starts_with("user:"))
        || audit["placedAt"].as_str().is_none()
    {
        return Err(refused("backup_remote_placement_unconfirmed"));
    }
    let expected = Expected {
        object_id: request.remote_object_id.clone(),
        folder_id: binding.folder_id.clone(),
        file_name: format!("{}.native-backup.json", request.placement_operation_id),
        archive_sha256: receipt["archiveSha256"]
            .as_str()
            .ok_or_else(|| refused("backup_remote_local_unavailable"))?
            .into(),
        archive_size_bytes: receipt["archiveSizeBytes"]
            .as_u64()
            .ok_or_else(|| refused("backup_remote_local_unavailable"))?,
    };
    Ok((receipt, expected))
}
pub fn prepare(
    control: &mut Connection,
    proof: &VerifiedCookie,
    catalog: &Catalog,
    runtime: &Runtime<'_>,
    request: &Request,
) -> Result<Preparation, Failure> {
    transaction(control, false, |tx| {
        let actor = authorize(
            tx,
            proof,
            catalog,
            runtime.local.cookies,
            runtime.local.clock,
        )?;
        request_valid(request)?;
        if let Some(value) = completed(tx, request)? {
            return Ok(Preparation::Complete(value));
        }
        let (local_receipt, expected) = local(tx, catalog, runtime, request)?;
        Ok(Preparation::Ready(Box::new(Prepared {
            expected,
            request: request.clone(),
            local_receipt,
            workspace: catalog.workspace.clone(),
            actor,
            restore_boundary: store::boundary(tx)?,
        })))
    })
}
pub fn finish(
    control: &mut Connection,
    proof: &VerifiedCookie,
    catalog: &Catalog,
    runtime: &Runtime<'_>,
    prepared: &Prepared,
    observation: &Observation,
) -> Result<Value, Failure> {
    transaction(control, true, |tx| {
        let actor = authorize(
            tx,
            proof,
            catalog,
            runtime.local.cookies,
            runtime.local.clock,
        )?;
        if actor != prepared.actor
            || catalog.workspace != prepared.workspace
            || store::boundary(tx)? != prepared.restore_boundary
        {
            return Err(refused("backup_remote_context_changed"));
        }
        if let Some(value) = completed(tx, &prepared.request)? {
            return Ok(value);
        }
        let (local_receipt, expected) = local(tx, catalog, runtime, &prepared.request)?;
        if local_receipt != prepared.local_receipt || expected != prepared.expected {
            return Err(refused("backup_remote_context_changed"));
        }
        let now = runtime
            .local
            .clock
            .now_unix_ms()
            .map_err(|_| refused("clock_unavailable"))?;
        let age = now
            .checked_sub(observation.observed_at_ms)
            .ok_or_else(|| refused("backup_remote_observation_refused"))?;
        if now < 0
            || observation.observed_at_ms < 0
            || !(0..=300_000).contains(&age)
            || observation.object_id != expected.object_id
            || observation.folder_id != expected.folder_id
            || observation.file_name != expected.file_name
            || observation.archive_sha256 != expected.archive_sha256
            || observation.archive_size_bytes != expected.archive_size_bytes
        {
            return Err(refused("backup_remote_observation_refused"));
        }
        let stamp = |ms| {
            rentemester_runtime::backup_keys::timestamp(ms)
                .map_err(|_| refused("clock_unavailable"))
        };
        let receipt = store::Receipt {
            format: store::FORMAT.into(),
            destination_id: prepared.request.destination_id.clone(),
            operation_id: prepared.request.operation_id.clone(),
            placement_operation_id: prepared.request.placement_operation_id.clone(),
            remote_object_id: expected.object_id,
            archive_sha256: expected.archive_sha256,
            archive_size_bytes: expected.archive_size_bytes,
            backup_created_at: local_receipt["backupCreatedAt"]
                .as_str()
                .ok_or_else(|| refused("backup_remote_local_unavailable"))?
                .into(),
            provider: "google-drive".into(),
            remote_file_name: expected.file_name,
            remote_parent_id: expected.folder_id,
            observed_at: stamp(observation.observed_at_ms)?,
            verified_at: stamp(now)?,
            verified_by: actor.clone(),
            remote_sync_verified: true,
            verify_method: "google-drive-metadata-and-sha256-reread".into(),
            signature_verified: true,
            attestation_independently_verified: false,
        };
        let receipt = store::append(tx, &receipt, &actor)?;
        Ok(json!({"ok":true,"verification":receipt}))
    })
}
pub fn inspect(
    control: &mut Connection,
    proof: &VerifiedCookie,
    catalog: &Catalog,
    cookies: &CookieVerifier,
    clock: &dyn Clock,
    destination: &str,
    operation: &str,
) -> Result<Value, Failure> {
    transaction(control, false, |tx| {
        authorize(tx, proof, catalog, cookies, clock)?;
        if !destination::valid_id(destination) || !destination::valid_id(operation) {
            return Err(refused("backup_remote_invalid_input"));
        }
        Ok(json!({"ok":true,"verification":store::inspect(tx, destination, operation)?}))
    })
}
#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests;
