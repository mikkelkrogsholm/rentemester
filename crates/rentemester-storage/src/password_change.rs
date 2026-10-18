//! Own credential/session effects under one admitted control writer. Caller verifies current KDF.
use crate::cookie_session::SessionIdentity;
use rentemester_engine::Error;
use rusqlite::{OptionalExtension, Transaction, params};
use zeroize::Zeroizing;
pub struct Rotation<'a> {
    pub id: &'a str,
    pub token: &'a str,
}
fn invalid(_: impl std::fmt::Display) -> Error {
    Error::new("invalid_control", "password change facts refused")
}
fn text(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= 4096 && !s.chars().any(char::is_control)
}
/// The application must recheck the exact KDF credential snapshot and current fresh cookie
/// under this same writer before invoking. A SessionIdentity is not a standalone KDF grant.
pub fn complete(
    tx: &Transaction<'_>,
    current: &SessionIdentity,
    hash: &str,
    rotation: Option<Rotation<'_>>,
    now: i64,
    version: u64,
) -> Result<String, Error> {
    crate::session_control::authorize(tx, current)?;
    crate::session_control::check_now(now)?;
    if version > 9_007_199_254_740_991 {
        return Err(Error::new("invalid_auth_config", "audit version refused"));
    }
    let b = hash.as_bytes();
    if b.len() != 161
        || b[32] != b':'
        || !b
            .iter()
            .enumerate()
            .all(|(i, c)| i == 32 || c.is_ascii_digit() || (b'a'..=b'f').contains(c))
    {
        return Err(invalid("hash encoding"));
    }
    let row:Option<(String,String,String,String)>=tx.query_row("SELECT id,userId,providerId,password FROM main.account WHERE issuer='local:credential' COLLATE BINARY AND accountId=?1 COLLATE BINARY",[&current.subject],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(invalid)?;
    let Some((id, owner, provider, old_hash)) = row else {
        return Err(crate::password_credential::denied());
    };
    let old_hash = Zeroizing::new(old_hash);
    if !text(&id)
        || owner != current.subject
        || provider != "credential"
        || old_hash.as_str() == hash
    {
        return Err(invalid("credential ownership/change"));
    }
    let date = crate::mfa_login::date(now)?;
    if tx.execute("UPDATE main.account SET password=?1,updatedAt=?2 WHERE id=?3 AND userId=?4 COLLATE BINARY AND password=?5 COLLATE BINARY AND issuer='local:credential' COLLATE BINARY AND providerId='credential' COLLATE BINARY AND accountId=?4 COLLATE BINARY",params![hash,date,id,current.subject,old_hash.as_str()]).map_err(invalid)?!=1 {return Err(invalid("credential CAS"));}
    let consumed=tx.execute("DELETE FROM main.verification WHERE value=?1 COLLATE BINARY AND substr(identifier,1,15)='reset-password:' COLLATE BINARY",[&current.subject]).map_err(invalid)?;
    let (deleted, new_session, token) = if let Some(r) = rotation {
        if !text(r.id) || !text(r.token) {
            return Err(invalid("rotation"));
        }
        let deleted = tx
            .execute(
                "DELETE FROM main.session WHERE userId=?1 COLLATE BINARY",
                [&current.subject],
            )
            .map_err(invalid)?;
        if deleted == 0 {
            return Err(invalid("current deletion"));
        }
        tx.execute("INSERT INTO main.session(id,token,userId,createdAt,updatedAt,expiresAt) VALUES(?1,?2,?3,?4,?4,?5)",params![r.id,r.token,current.subject,date,crate::mfa_login::date(now.checked_add(43_200_000).ok_or_else(||invalid("expiry"))?)?]).map_err(invalid)?;
        (deleted, Some(r.id), r.token.to_owned())
    } else {
        let token: String = tx
            .query_row(
                "SELECT token FROM main.session WHERE id=?1 AND userId=?2 COLLATE BINARY",
                params![current.session_id, current.subject],
                |r| r.get(0),
            )
            .map_err(invalid)?;
        (0, None, token)
    };
    let token = Zeroizing::new(token);
    let entity = serde_json::to_string(&(
        &current.subject,
        &current.session_id,
        version,
        new_session,
        deleted,
        consumed,
    ))
    .map_err(invalid)?;
    tx.execute("INSERT INTO main.workspace_audit(event_type,entity_type,entity_id,actor,created_at) VALUES('password_changed','native_password_change_v1',?1,?2,?3)",params![entity,format!("user:{}",current.subject),date]).map_err(invalid)?;
    let view = crate::session_view::current(tx, &token, now)?;
    let user_json = serde_json::to_string(view.user()).map_err(invalid)?;
    if user_json.len() > 65_536 {
        return Err(invalid("user result budget"));
    }
    Ok(user_json)
}
