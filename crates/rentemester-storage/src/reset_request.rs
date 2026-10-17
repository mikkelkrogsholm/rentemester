//! Recovery issuance/audit only, on a held canonical control writer.
use rentemester_engine::Error;
use rusqlite::{MAIN_DB, OptionalExtension, Transaction, TransactionState, params};
use zeroize::Zeroizing;
pub struct Issuance<'a> {
    pub token: &'a str,
    pub id: &'a str,
    pub now: i64,
    pub identity_hash: &'a str,
    pub audit_key_version: u64,
}
fn invalid(_: impl std::fmt::Display) -> Error {
    Error::new("invalid_control", "recovery issuance facts refused")
}
pub fn issue(tx: &Transaction<'_>, email: &str, input: Issuance<'_>) -> Result<bool, Error> {
    if tx.transaction_state(Some(MAIN_DB)).map_err(invalid)? != TransactionState::Write {
        return Err(Error::new(
            "control_fence_required",
            "control writer required",
        ));
    }
    crate::control_migration::validate_current_control(tx).map_err(invalid)?;
    crate::password_reset::validate_token(input.token)?;
    if crate::password_credential::lookup_email(email)? != email
        || input.id.trim().is_empty()
        || input.id.len() > 4096
        || input.id.chars().any(char::is_control)
        || input.identity_hash.len() != 64
        || !input
            .identity_hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid("issuance input"));
    }
    if !(0..=253_402_300_799_999 - 3_600_000).contains(&input.now) {
        return Err(Error::new(
            "invalid_auth_clock",
            "recovery issuance clock refused",
        ));
    }
    if input.audit_key_version > 9_007_199_254_740_991 {
        return Err(Error::new("invalid_auth_config", "audit version refused"));
    }
    let subject: Option<String> = tx
        .query_row(
            "SELECT id FROM main.user WHERE email=?1 COLLATE BINARY",
            [email],
            |r| r.get(0),
        )
        .optional()
        .map_err(invalid)?;
    if subject
        .as_ref()
        .is_some_and(|s| s.trim().is_empty() || s.len() > 4096 || s.chars().any(char::is_control))
    {
        return Err(invalid("subject"));
    }
    let created = crate::mfa_login::date(input.now)?;
    if let Some(subject) = &subject {
        let identifier = Zeroizing::new(format!("reset-password:{}", input.token));
        let exists: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM main.verification WHERE identifier=?1 COLLATE BINARY)",
                [identifier.as_str()],
                |r| r.get(0),
            )
            .map_err(invalid)?;
        if exists {
            return Err(invalid("token collision"));
        }
        tx.execute("INSERT INTO main.verification(id,identifier,value,createdAt,updatedAt,expiresAt) VALUES(?1,?2,?3,?4,?4,?5)",params![input.id,identifier.as_str(),subject,created,crate::mfa_login::date(input.now+3_600_000)?]).map_err(invalid)?;
    }
    let entity = serde_json::to_string(&(input.identity_hash, input.audit_key_version, &subject))
        .map_err(invalid)?;
    tx.execute("INSERT INTO main.workspace_audit(event_type,entity_type,entity_id,actor,created_at) VALUES('password_reset_requested','native_reset_request_v1',?1,'system:auth-recovery',?2)",params![entity,created]).map_err(invalid)?;
    // This commits the request attempt, never a claim that an external message was delivered.
    tx.execute("INSERT INTO main.rm_workspace_auth_telemetry_events(endpoint,outcome,user_id,identity_hash,identity_key_version,created_at) VALUES('request-password-reset','attempted',?1,?2,?3,?4)",params![subject,input.identity_hash,input.audit_key_version as i64,created]).map_err(invalid)?;
    Ok(subject.is_some())
}
