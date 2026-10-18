//! Same-writer human display context; never a reusable company authorization grant.
use crate::mfa_login::{error, failure};
pub use crate::password_login::Failure;
use rentemester_runtime::auth::{Clock, CookieVerifier, VerifiedCookie};
pub use rentemester_storage::workspace_context::{Catalog, RegisteredCompany, WorkspaceContext};
use rusqlite::{Connection, TransactionBehavior};

/// The trusted owner retains catalog/physical/control/workspace lifetime admission throughout.
pub fn execute(
    control: &mut Connection,
    cookies: &CookieVerifier,
    clock: &dyn Clock,
    proof: &VerifiedCookie,
    catalog: &Catalog,
) -> Result<WorkspaceContext, Failure> {
    crate::control_fence::connection_ready(control)
        .map_err(|_| failure(error("connection_refused"), false))?;
    if !cookies.accepts_session(proof) || catalog.workspace != proof.workspace() {
        return Err(failure(error("invalid_credentials"), true));
    }
    rentemester_storage::workspace_context::validate_catalog(catalog)
        .map_err(|e| failure(e, true))?;
    let tx = control
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| failure(error("control_busy_or_unavailable"), false))?;
    let result = (|| {
        let now = clock
            .now_unix_ms()
            .map_err(|_| error("invalid_auth_clock"))?;
        let result =
            rentemester_storage::workspace_context::current(&tx, proof.token(), now, catalog);
        #[cfg(test)]
        crate::control_fence::checkpoint("context-read");
        result
    })();
    if tx.rollback().is_err() || crate::control_fence::connection_ready(control).is_err() {
        return Err(failure(error("connection_refused"), false));
    }
    result.map_err(|e| failure(e, true))
}
