//! Current retained backup plus exact historical placement/remote evidence, with declarations kept separate.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
};
use rentemester_engine::Error;
use rentemester_runtime::backup_destination::Config;
use rusqlite::Connection;
use serde_json::{Value, json};
fn invalid() -> Error {
    Error::new(
        "backup_governance_unavailable",
        "Current backup governance cannot be observed safely",
    )
}
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    context: Context<'_>,
    configs: &[Config],
) -> Result<Committed<Value>, Failure> {
    let workspace = context.selection.workspace;
    let slug = context.selection.slug;
    let clock = context.clock;
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
        true,
        true,
        |control, company, _| {
            let mut current = crate::backup_status::data_at_kind(
                control,
                company,
                workspace,
                slug,
                clock.now_unix_ms().map_err(|_| invalid())?,
                true,
                false,
            )?;
            let mut ids = std::collections::BTreeSet::new();
            for config in configs {
                if !ids.insert(&config.id) {
                    return Err(invalid());
                }
            }
            let mut destinations: Vec<Value> = configs
                .iter()
                .map(|config| {
                    let mut value = config.view();
                    value["placements"] = json!([]);
                    value["remoteVerifications"] = json!([]);
                    value
                })
                .collect();
            let mut placement_count = 0;
            let mut remote_count = 0;
            let mut declared_copy = false;
            if let Some(hash) = current["backup"]["latestBackupId"]
                .as_str()
                .filter(|id| id.len() == 64)
            {
                let uuid: String = company
                    .query_row(
                        "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
                        [],
                        |row| row.get(0),
                    )
                    .map_err(|_| invalid())?;
                let inventory = rentemester_storage::backup_inventory::for_company(
                    control, workspace, slug, &uuid,
                )?;
                let snapshot = inventory
                    .iter()
                    .find(|snapshot| {
                        snapshot.archive_sha256 == hash
                            && current["backup"]["latestBackupAt"] == snapshot.created_at
                    })
                    .ok_or_else(invalid)?;
                let evidence = rentemester_storage::backup_governance::matching(
                    control,
                    &configs
                        .iter()
                        .map(|config| config.id.clone())
                        .collect::<Vec<_>>(),
                    hash,
                    snapshot.archive_size_bytes,
                    &snapshot.created_at,
                    current["backup"]["checkedAt"]
                        .as_str()
                        .ok_or_else(invalid)?,
                )?;
                for (config, view) in configs.iter().zip(&mut destinations) {
                    let placements: Vec<Value> = evidence
                        .placements
                        .iter()
                        .filter(|receipt| receipt["destinationId"] == config.id)
                        .map(|receipt| {
                            let mut value = receipt.clone();
                            value["destination"] = config.view();
                            value
                        })
                        .collect();
                    let remote: Vec<_> = evidence
                        .remote_verifications
                        .iter()
                        .filter(|receipt| receipt.destination_id == config.id)
                        .collect();
                    placement_count += placements.len();
                    remote_count += remote.len();
                    if !remote.is_empty() && config.view()["offsiteAttested"] == true {
                        declared_copy = true;
                    }
                    view["placements"] = json!(placements);
                    view["remoteVerifications"] = json!(remote);
                }
            }
            let no_qualification_errors = current["backup"]["errors"]
                .as_array()
                .is_some_and(Vec::is_empty);
            let external = remote_count > 0 && no_qualification_errors;
            current["governance"] = json!({"format":"rentemester-native-backup-governance/1","checkedAt":current["backup"]["checkedAt"],"latestBackupId":current["backup"]["latestBackupId"],"destinationCount":destinations.len(),"declaredOffsiteDestinationCount":configs.iter().filter(|config| config.view()["offsiteAttested"] == true).count(),"latestBackupPlacementCount":placement_count,"latestBackupRemoteVerificationCount":remote_count,"latestBackupExternalCopyObserved":external,"latestBackupExternalCopyAtDeclaredDestination":external && declared_copy,"attestationIndependentlyVerified":false,"recoveryVerified":false,"destinations":destinations,"errors":current["backup"]["errors"]});
            Ok(current)
        },
    )
}
pub mod original717;
#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests;
