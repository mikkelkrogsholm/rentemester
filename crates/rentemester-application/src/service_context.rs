//! One current service admission/quota/rate transaction and bounded own display receipt.
use crate::mfa_login::{error, failure};
pub use crate::password_login::{Failure, FailureStage};
use rentemester_runtime::{auth::Clock, service_key::ServiceKey};
use rentemester_storage::workspace_context::{Catalog, WorkspaceContext};
use rusqlite::{Connection, TransactionBehavior};
/// Caller retains admitted control/catalog/physical/workspace lifetime. No opener or later grant.
pub fn execute(
    control: &mut Connection,
    workspace: &str,
    clock: &dyn Clock,
    key: &ServiceKey,
    catalog: &Catalog,
) -> Result<WorkspaceContext, Failure> {
    crate::control_fence::connection_ready(control)
        .map_err(|_| failure(error("connection_refused"), false))?;
    if workspace != key.workspace() || workspace != catalog.workspace {
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
        let context =
            rentemester_storage::service_context::current(&tx, key.lookup_hash(), now, catalog)?;
        #[cfg(test)]
        if std::env::var("RM_MFA_FAIL_SQL").ok().as_deref() == Some("1") {
            tx.execute("INSERT INTO main.session(id) VALUES(NULL)", [])
                .map_err(|_| error("invalid_control"))?;
        }
        #[cfg(test)]
        crate::mfa_login::checkpoint("service-before-commit");
        Ok(context)
    })();
    let value = match result {
        Ok(v) => v,
        Err(e) => {
            let reusable =
                tx.rollback().is_ok() && crate::control_fence::connection_ready(control).is_ok();
            return Err(failure(e, reusable));
        }
    };
    tx.commit().map_err(|_| Failure {
        stage: FailureStage::OutcomeUnknown,
        error: error("commit_outcome_unknown"),
        connection_reusable: false,
    })?;
    Ok(value)
}
