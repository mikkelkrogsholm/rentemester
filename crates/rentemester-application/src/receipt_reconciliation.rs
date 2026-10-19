//! Current, selected-company restore management. No business execution authority is imported.
use crate::{
    access::{self, CompanyRole, Permission, WorkspaceRole},
    control_fence::{self, Committed, CompanySelection, Failure, SessionRequirement},
};
use rentemester_engine::Error;
use rentemester_runtime::auth::{Clock, VerifiedCookie};
use rentemester_storage::receipt_reconciliation::{self as store, Reconcile};
use rusqlite::Connection;
use serde_json::{Value, json};
pub enum Action<'a> {
    Inventory,
    Reconcile(&'a Reconcile),
}
#[allow(clippy::too_many_arguments)]
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    cookie: &VerifiedCookie,
    clock: &dyn Clock,
    selection: CompanySelection<'_>,
    action: Action<'_>,
) -> Result<Committed<Value>, Failure> {
    control_fence::execute_authorized(
        control,
        company,
        |tx| {
            let denied = || {
                Error::new(
                    "access_denied",
                    "Current workspace management and selected company administration required",
                )
            };
            if cookie.workspace() != selection.workspace
                || !selection.registered
                || selection.archived
            {
                return Err(denied());
            }
            let current = rentemester_storage::cookie_session::resolve(
                tx,
                cookie.token(),
                clock.now_unix_ms().map_err(|_| denied())?,
                if matches!(action, Action::Reconcile(_)) {
                    SessionRequirement::Fresh
                } else {
                    SessionRequirement::Current
                },
            )?;
            rentemester_storage::session_control::authorize(tx, &current)?;
            let workspace = rentemester_storage::workspace_members::access(tx, &current.subject)?
                .ok_or_else(denied)?;
            let workspace: WorkspaceRole =
                serde_json::from_value(json!(workspace)).map_err(|_| denied())?;
            let company = rentemester_storage::workspace_members::membership(
                tx,
                &current.subject,
                selection.slug,
            )?
            .ok_or_else(denied)?;
            let company: CompanyRole =
                serde_json::from_value(json!(company)).map_err(|_| denied())?;
            if !access::workspace_role_allows(workspace, Permission::WorkspaceManage)
                || !access::company_role_allows(company, Permission::CompanyAdmin)
            {
                return Err(denied());
            }
            Ok(current.subject)
        },
        |tx, subject| {
            let value = match action {
                Action::Inventory => store::inventory(tx)?,
                Action::Reconcile(input) => {
                    let ms = clock.now_unix_ms().map_err(|_| {
                        Error::new("clock_unavailable", "trusted clock unavailable")
                    })?;
                    let now = rentemester_runtime::backup_keys::timestamp(ms).map_err(|_| {
                        Error::new("clock_unavailable", "trusted clock unavailable")
                    })?;
                    store::reconcile(tx, input, subject, &now)?
                }
            };
            Ok(json!({"restoredReceipts":value}))
        },
    )
}
#[cfg(test)]
mod tests;
