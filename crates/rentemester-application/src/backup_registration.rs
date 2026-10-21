//! Company-admin inert destination registrations and explicitly unverified transfer declarations.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
};
use rentemester_engine::Error;
use rentemester_runtime::backup_destination::Config;
use rentemester_storage::backup_registration as store;
use rusqlite::{Connection, Transaction};
use serde_json::{Value, json};
pub use store::Request;
pub enum Action<'a> {
    List,
    Inspect { operation: &'a str },
    Apply(&'a Request),
}
fn refused() -> Error {
    Error::new(
        "backup_registration_refused",
        "Backup registration or declaration refused",
    )
}
fn qualified(
    control: &Transaction<'_>,
    company: &Transaction<'_>,
    workspace: &str,
    slug: &str,
    now: i64,
) -> Result<(Vec<store::ProtectedBackup>, Vec<String>), Error> {
    let uuid: String = company
        .query_row(
            "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
            [],
            |r| r.get(0),
        )
        .map_err(|_| refused())?;
    let snapshots =
        rentemester_storage::backup_inventory::for_company(control, workspace, slug, &uuid)?;
    let path: String = control
        .query_row(
            "SELECT file FROM pragma_database_list WHERE name='main'",
            [],
            |r| r.get(0),
        )
        .map_err(|_| refused())?;
    let mut seen = std::collections::BTreeSet::new();
    let mut backups = vec![];
    let mut errors = vec![];
    for snapshot in snapshots {
        if !seen.insert(snapshot.archive_sha256.clone()) {
            continue;
        }
        let checked = (|| {
            if snapshot.created_at_ms > now {
                return Err(());
            }
            let key = rentemester_runtime::backup_artifact::TrustedPublicKey::from_bytes(
                snapshot.signing_public_key.as_deref().ok_or(())?,
            )
            .map_err(|_| ())?;
            let receipt = rentemester_runtime::backup_destination::verify_retained(
                std::path::Path::new(&path),
                &snapshot.archive_sha256,
                &key,
                workspace,
            )
            .map_err(|_| ())?;
            if receipt["archiveSha256"] != snapshot.archive_sha256
                || receipt["archiveSizeBytes"] != snapshot.archive_size_bytes
                || receipt["backupCreatedAt"] != snapshot.created_at
            {
                return Err(());
            }
            Ok(())
        })();
        if checked.is_ok() {
            backups.push(store::ProtectedBackup {
                backup_id: snapshot.archive_sha256.clone(),
                archive_sha256: snapshot.archive_sha256,
                archive_size_bytes: snapshot.archive_size_bytes,
                backup_created_at: snapshot.created_at,
            });
        } else {
            errors.push("En registreret lokal backup kunne ikke genlæses og verificeres.".into());
        }
    }
    Ok((backups, errors))
}
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    context: Context<'_>,
    configs: &[Config],
    action: Action<'_>,
) -> Result<Committed<Value>, Failure> {
    let workspace = context.selection.workspace;
    let slug = context.selection.slug;
    let clock = context.clock;
    let readonly = !matches!(action, Action::Apply(_));
    control_fence::execute_cookie_both(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock,
            requirement: SessionRequirement::Current,
        },
        context.selection,
        Permission::CompanyAdmin,
        readonly,
        true,
        |control, company, subject| match action {
            Action::Inspect { operation } => Ok(
                json!({"ok":true,"workspaceId":workspace,"slug":slug,"operation":store::inspect(control,slug,operation)?}),
            ),
            Action::List => {
                let (destinations, history) = store::list(control, slug)?;
                let now = clock.now_unix_ms().map_err(|_| refused())?;
                if now < 0 {
                    return Err(refused());
                }
                let (backups, errors) = qualified(control, company, workspace, slug, now)?;
                Ok(
                    json!({"ok":true,"workspaceId":workspace,"slug":slug,"destinations":destinations,"history":history,"backups":backups.into_iter().map(|b|json!({"backupId":b.backup_id,"archiveSha256":b.archive_sha256,"archiveSizeBytes":b.archive_size_bytes,"backupCreatedAt":b.backup_created_at})).collect::<Vec<_>>(),"operatorTargets":configs.iter().map(Config::view).collect::<Vec<_>>(),"errors":errors}),
                )
            }
            Action::Apply(input) => {
                let input = input.canonical()?;
                if let Some(receipt) = store::replay(control, slug, &input)? {
                    return Ok(
                        json!({"ok":true,"workspaceId":workspace,"slug":slug,"operation":receipt}),
                    );
                }
                let now = clock.now_unix_ms().map_err(|_| refused())?;
                if now < 0 {
                    return Err(refused());
                }
                let at = rentemester_runtime::backup_keys::timestamp(now).map_err(|_| refused())?;
                if let Request::Add {
                    kind,
                    operator_target_id: Some(target),
                    ..
                } = &input
                {
                    let matches: Vec<_> = configs
                        .iter()
                        .filter(|config| &config.id == target && &config.kind == kind)
                        .collect();
                    if matches.len() != 1 {
                        return Err(refused());
                    }
                }
                let backup = if let Request::Declare {
                    destination_id,
                    backup_id,
                    ..
                } = &input
                {
                    if !store::list(control, slug)?
                        .0
                        .iter()
                        .any(|v| v["destinationId"] == destination_id.as_str())
                    {
                        return Err(refused());
                    }
                    qualified(control, company, workspace, slug, now)?
                        .0
                        .into_iter()
                        .find(|b| &b.backup_id == backup_id)
                        .ok_or_else(refused)
                        .map(Some)?
                } else {
                    None
                };
                let receipt = store::apply(
                    control,
                    slug,
                    &input,
                    &format!("user:{subject}"),
                    &at,
                    backup.as_ref(),
                )?;
                Ok(json!({"ok":true,"workspaceId":workspace,"slug":slug,"operation":receipt}))
            }
        },
    )
}
#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests;
