//! Original dashboard JSON with current company authority and retained-source assurance.
use crate::{
    access::{CompanyRole, Permission, company_role_allows},
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
};
use rentemester_engine::Error;
use rentemester_runtime::{auth::Clock, namespace::NamespaceGuard};
use rusqlite::{Connection, Transaction};
use serde_json::{Value, json};
pub fn read(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    as_of: &str,
    today: &str,
) -> Result<Committed<Value>, Failure> {
    let selection = crate::control_fence::CompanySelection {
        workspace: context.selection.workspace,
        slug: context.selection.slug,
        registered: context.selection.registered,
        archived: context.selection.archived,
    };
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
        Permission::CompanyRead,
        true,
        false,
        |control, company, subject| {
            if !crate::periods::verify_sources(company, guard)?.is_empty() {
                return Err(Error::new(
                    "report_source_unavailable",
                    "Dashboard source assurance differs",
                ));
            }
            crate::archive_import::verify_physical(company, guard)?;
            compose_authorized(control, company, selection, subject, clock, as_of, today)
        },
    )
}
/// Compose only after current CompanyRead authority and physical-source checks
/// have succeeded on these same held control/company transactions.
pub fn compose_authorized(
    control: &Transaction<'_>,
    company: &Transaction<'_>,
    selection: crate::control_fence::CompanySelection<'_>,
    subject: &str,
    clock: &dyn Clock,
    as_of: &str,
    today: &str,
) -> Result<Value, Error> {
    let workspace = selection.workspace;
    let slug = selection.slug;
    let mut snapshot = rentemester_storage::dashboard_json::read(company, slug, as_of, today)?;
    let role = rentemester_storage::control_access::observe_company_role(control, subject, slug)?;
    let admin = role
        .map(|r| serde_json::from_value::<CompanyRole>(Value::String(r)))
        .transpose()
        .map_err(|_| Error::new("invalid_control", "Invalid company role"))?
        .is_some_and(|r| company_role_allows(r, Permission::CompanyAdmin));
    snapshot["backup"] = if admin {
        let status = crate::backup_status::data(control, company, workspace, slug, clock, true)?;
        let b = &status["backup"];
        json!({"backupDue":b["backupDue"],"backupsFound":b["backupsFound"],"latestBackupAt":b["latestBackupAt"],"daysSinceLatestBackup":b["daysSinceLatestBackup"],"hasActivitySinceBackup":b["hasActivitySinceBackup"],"available":true,"scope":b["scope"],"checkedAt":b["checkedAt"],"errors":b["errors"],"basis":"current_state","remoteSyncVerified":false})
    } else {
        json!({"backupDue":null,"backupsFound":null,"latestBackupAt":null,"daysSinceLatestBackup":null,"hasActivitySinceBackup":null,"available":false,"requiredPermission":"company.admin","scope":"local-retained-snapshots","checkedAt":null,"errors":[],"basis":"current_state"})
    };
    Ok(snapshot)
}
#[cfg(all(test, target_os = "macos"))]
mod tests;
