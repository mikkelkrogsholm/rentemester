//! Current own-session snapshot, without refresh/cache/cleanup or company authorization grant.
use crate::mfa_login::{error, failure};
pub use crate::password_login::Failure;
use rentemester_runtime::auth::{Clock, CookieVerifier, VerifiedCookie};
pub use rentemester_storage::session_view::SessionView;
use rusqlite::{Connection, TransactionBehavior};
pub fn execute(
    control: &mut Connection,
    cookies: &CookieVerifier,
    clock: &dyn Clock,
    proof: &VerifiedCookie,
) -> Result<SessionView, Failure> {
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
        rentemester_storage::session_view::current(&tx, proof.token(), now)
    })();
    if tx.rollback().is_err() || crate::control_fence::connection_ready(control).is_err() {
        return Err(failure(error("connection_refused"), false));
    }
    result.map_err(|e| failure(e, true))
}
