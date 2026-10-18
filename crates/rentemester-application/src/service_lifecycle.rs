//! Owner service lifecycle. Caller retains admitted control lifetime and legacy/provider drain.
use crate::mfa_login::{error, failure};
pub use crate::password_login::{Failure, FailureStage};
use rentemester_runtime::{
    auth::{Clock, CookieVerifier, VerifiedCookie},
    service_key::{NewServiceKey, new_uuid},
};
pub use rentemester_storage::service_lifecycle::{Operation, Receipt};
use rentemester_storage::{
    cookie_session::{self, SessionRequirement},
    service_lifecycle as store,
};
use rusqlite::{Connection, TransactionBehavior};
pub fn execute(
    control: &mut Connection,
    cookies: &CookieVerifier,
    clock: &dyn Clock,
    proof: &VerifiedCookie,
    op: &Operation,
) -> Result<Receipt, Failure> {
    crate::control_fence::connection_ready(control)
        .map_err(|_| failure(error("connection_refused"), false))?;
    if !cookies.accepts_session(proof) {
        return Err(failure(error("invalid_credentials"), true));
    }
    let tx = control
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| failure(error("control_busy_or_unavailable"), false))?;
    let result = (|| {
        let now = clock
            .now_unix_ms()
            .map_err(|_| error("invalid_auth_clock"))?;
        let current = cookie_session::resolve(
            &tx,
            proof.token(),
            now,
            if matches!(op, Operation::List) {
                SessionRequirement::Current
            } else {
                SessionRequirement::Fresh
            },
        )?;
        let uuid = || new_uuid().map_err(|e| error(e.code()));
        let (operation_id, marker, subject, credential, key) =
            if matches!(op, Operation::List | Operation::Recover { .. }) {
                (
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                    None,
                )
            } else {
                let key = if matches!(op, Operation::Create { .. } | Operation::Rotate { .. }) {
                    Some(
                        NewServiceKey::generate(cookies.workspace())
                            .map_err(|e| error(e.code()))?,
                    )
                } else {
                    None
                };
                (
                    uuid()?,
                    uuid()?.replace('-', ""),
                    format!("service_{}", uuid()?.replace('-', "")),
                    uuid()?,
                    key,
                )
            };
        let material = (!operation_id.is_empty()).then_some(store::Material {
            operation_id: &operation_id,
            marker: &marker,
            subject: &subject,
            credential: &credential,
            secret: key.as_ref().map(NewServiceKey::secret),
            hash: key.as_ref().map(NewServiceKey::lookup_hash),
        });
        let receipt = store::execute(&tx, &current, op, material, now)?;
        #[cfg(test)]
        if std::env::var("RM_MFA_FAIL_SQL").ok().as_deref() == Some("1") {
            tx.execute("INSERT INTO main.session(id) VALUES(NULL)", [])
                .map_err(|_| error("invalid_control"))?;
        }
        #[cfg(test)]
        crate::mfa_login::checkpoint("lifecycle-before-commit");
        Ok(receipt)
    })();
    let receipt = match result {
        Ok(r) => r,
        Err(e) => {
            let reusable =
                tx.rollback().is_ok() && crate::control_fence::connection_ready(control).is_ok();
            return Err(failure(e, reusable));
        }
    };
    if matches!(op, Operation::List) {
        if tx.rollback().is_err() || crate::control_fence::connection_ready(control).is_err() {
            return Err(failure(error("connection_refused"), false));
        }
    } else {
        tx.commit().map_err(|_| Failure {
            stage: FailureStage::OutcomeUnknown,
            error: error("commit_outcome_unknown"),
            connection_reusable: false,
        })?;
    }
    Ok(receipt)
}
