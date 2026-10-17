//! Existing source-issued recovery tokens, consumed with credential/session effects atomically.
use rentemester_engine::Error;
use rusqlite::{MAIN_DB, OptionalExtension, Transaction, TransactionState, params};
use zeroize::Zeroizing;

fn invalid(_: impl std::fmt::Display) -> Error {
    Error::new("invalid_control", "password recovery facts refused")
}
pub fn denied() -> Error {
    Error::new("invalid_credentials", "password recovery refused")
}
fn text(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 4096 && !value.chars().any(char::is_control)
}
/// The installed source generates 24 ASCII alphanumeric characters, without URL decoding.
pub fn validate_token(token: &str) -> Result<(), Error> {
    if token.len() != 24 || !token.bytes().all(|c| c.is_ascii_alphanumeric()) {
        Err(denied())
    } else {
        Ok(())
    }
}
/// Caller holds the one admitted control writer through commit. No session is created and
/// reset possession grants no workspace membership, email verification or MFA authority.
pub fn complete(
    tx: &Transaction<'_>,
    token: &str,
    password_hash: &str,
    new_account_id: &str,
    now: i64,
    audit_key_version: u64,
) -> Result<(), Error> {
    if tx.transaction_state(Some(MAIN_DB)).map_err(invalid)? != TransactionState::Write {
        return Err(Error::new(
            "control_fence_required",
            "control writer required",
        ));
    }
    crate::control_migration::validate_current_control(tx).map_err(invalid)?;
    crate::session_control::check_now(now)?;
    validate_token(token)?;
    if audit_key_version > 9_007_199_254_740_991 {
        return Err(Error::new("invalid_auth_config", "audit version refused"));
    }
    let b = password_hash.as_bytes();
    if !text(new_account_id)
        || b.len() != 161
        || b[32] != b':'
        || !b
            .iter()
            .enumerate()
            .all(|(i, c)| i == 32 || c.is_ascii_digit() || (b'a'..=b'f').contains(c))
    {
        return Err(invalid("credential encoding"));
    }
    let subject = recovery_subject(tx, token, now)?;
    // Resolve the unique canonical account key independently of its owner/provider. A corrupt
    // key belonging to someone else is refused, never adopted or overwritten.
    let account: Option<(String,String,String)> = tx.query_row("SELECT id,userId,providerId FROM main.account WHERE issuer='local:credential' COLLATE BINARY AND accountId=?1 COLLATE BINARY",[&subject],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(invalid)?;
    let date = crate::mfa_login::date(now)?;
    let created_account = account.is_none();
    if let Some((account_id, owner, provider)) = account {
        if !text(&account_id) || owner != subject || provider != "credential" {
            return Err(invalid("credential ownership"));
        }
        if tx.execute("UPDATE main.account SET password=?1,updatedAt=?2 WHERE id=?3 AND userId=?4 AND issuer='local:credential' COLLATE BINARY AND providerId='credential' COLLATE BINARY AND accountId=?4 COLLATE BINARY",params![password_hash,date,account_id,subject]).map_err(invalid)? != 1 { return Err(invalid("credential update")); }
    } else {
        tx.execute("INSERT INTO main.account(id,userId,accountId,providerId,issuer,password,createdAt,updatedAt) VALUES(?1,?2,?2,'credential','local:credential',?3,?4,?4)",params![new_account_id,subject,password_hash,date]).map_err(invalid)?;
    }
    // Strict native correction: success revokes all outstanding source reset links for this
    // subject, so an older link cannot undo recovery. Other verification namespaces survive.
    let consumed = tx.execute("DELETE FROM main.verification WHERE value=?1 COLLATE BINARY AND substr(identifier,1,15)='reset-password:' COLLATE BINARY",[&subject]).map_err(invalid)?;
    if consumed == 0 {
        return Err(invalid("reset consumption"));
    }
    let deleted = tx
        .execute(
            "DELETE FROM main.session WHERE userId=?1 COLLATE BINARY",
            [&subject],
        )
        .map_err(invalid)?;
    let entity = serde_json::to_string(&(
        &subject,
        audit_key_version,
        created_account,
        deleted,
        consumed,
    ))
    .map_err(invalid)?;
    tx.execute("INSERT INTO main.workspace_audit(event_type,entity_type,entity_id,actor,created_at) VALUES('password_reset_completed','native_password_reset_v1',?1,?2,?3)",params![entity,format!("user:{subject}"),date]).map_err(invalid)?;
    tx.execute("INSERT INTO main.rm_workspace_auth_telemetry_events(endpoint,outcome,user_id,identity_key_version,created_at) VALUES('reset-password','accepted',?1,?2,?3)",params![subject,audit_key_version as i64,date]).map_err(invalid)?;
    Ok(())
}

/// Stable current read for redirect only; final completion must resolve again under its writer.
pub fn inspect(tx: &Transaction<'_>, token: &str, now: i64) -> Result<(), Error> {
    if tx.transaction_state(Some(MAIN_DB)).map_err(invalid)? == TransactionState::None {
        return Err(invalid("snapshot required"));
    }
    crate::control_migration::validate_current_control(tx).map_err(invalid)?;
    crate::session_control::check_now(now)?;
    recovery_subject(tx, token, now).map(|_| ())
}
fn recovery_subject(tx: &Transaction<'_>, token: &str, now: i64) -> Result<String, Error> {
    validate_token(token)?;
    let identifier = Zeroizing::new(format!("reset-password:{token}"));
    let mut query = tx.prepare("SELECT id,value,createdAt,updatedAt,expiresAt FROM main.verification WHERE identifier=?1 COLLATE BINARY LIMIT 2").map_err(invalid)?;
    let mut rows = query.query([identifier.as_str()]).map_err(invalid)?;
    let Some(r) = rows.next().map_err(invalid)? else {
        return Err(denied());
    };
    let id: String = r.get(0).map_err(invalid)?;
    let subject: String = r.get(1).map_err(invalid)?;
    let stamp = |i| -> Result<i64, Error> {
        let s: String = r.get(i).map_err(invalid)?;
        crate::cookie_session::timestamp(&s).map_err(invalid)
    };
    let (created, updated, expires) = (stamp(2)?, stamp(3)?, stamp(4)?);
    if !text(&id)
        || !text(&subject)
        || rows.next().map_err(invalid)?.is_some()
        || created > updated
        || updated > now
        || expires <= created
        || expires - created > 3_600_000
    {
        return Err(invalid("recovery record"));
    }
    if expires <= now {
        return Err(denied());
    }
    drop(rows);
    drop(query);
    let exists: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM main.user WHERE id=?1 COLLATE BINARY)",
            [&subject],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    if !exists {
        return Err(denied());
    }
    Ok(subject)
}
