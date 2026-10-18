//! Own authenticated session metadata. No bearer serialization, refresh or cleanup.
use rentemester_engine::Error;
use rusqlite::{OptionalExtension, Transaction};
use serde::Serialize;
fn invalid() -> Error {
    Error::new("invalid_control", "current session view refused")
}
fn text(s: String) -> Result<String, Error> {
    if s.trim().is_empty() || s.len() > 4096 || s.chars().any(char::is_control) {
        Err(invalid())
    } else {
        Ok(s)
    }
}
fn optional(s: Option<String>) -> Result<Option<String>, Error> {
    s.map(|v| {
        if v.len() > 4096 || v.chars().any(char::is_control) {
            Err(invalid())
        } else {
            Ok(v)
        }
    })
    .transpose()
}
fn date(s: String) -> Result<String, Error> {
    let s = if s.len() == 10 {
        format!("{s}T00:00:00.000Z")
    } else {
        s
    };
    crate::cookie_session::timestamp(&s).map_err(|_| invalid())?;
    Ok(s)
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionUser {
    id: String,
    name: String,
    email: String,
    email_verified: bool,
    two_factor_enabled: bool,
    created_at: String,
    updated_at: String,
    image: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMetadata {
    id: String,
    user_id: String,
    created_at: String,
    updated_at: String,
    expires_at: String,
    ip_address: Option<String>,
    user_agent: Option<String>,
}
#[derive(Serialize)]
pub struct SessionView {
    session: SessionMetadata,
    user: SessionUser,
}
impl SessionView {
    pub fn user(&self) -> &SessionUser {
        &self.user
    }
}
/// Requires existing control-writer admission. Result is display data, never later authority.
pub fn current(tx: &Transaction<'_>, token: &str, now: i64) -> Result<SessionView, Error> {
    let current = crate::cookie_session::resolve(
        tx,
        token,
        now,
        crate::cookie_session::SessionRequirement::Current,
    )?;
    crate::session_control::authorize(tx, &current)?;
    let user=tx.query_row("SELECT id,name,email,createdAt,updatedAt,image FROM main.user WHERE id=?1 COLLATE BINARY",[&current.subject],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,Option<String>>(5)?))).optional().map_err(|_|invalid())?.ok_or_else(invalid)?;
    let session=tx.query_row("SELECT id,userId,createdAt,updatedAt,expiresAt,ipAddress,userAgent FROM main.session WHERE id=?1 COLLATE BINARY AND userId=?2 COLLATE BINARY",[&current.session_id,&current.subject],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,Option<String>>(5)?,r.get::<_,Option<String>>(6)?))).map_err(|_|invalid())?;
    Ok(SessionView {
        user: SessionUser {
            id: text(user.0)?,
            name: optional(Some(user.1))?.ok_or_else(invalid)?,
            email: text(user.2)?,
            email_verified: true,
            two_factor_enabled: true,
            created_at: date(user.3)?,
            updated_at: date(user.4)?,
            image: optional(user.5)?,
        },
        session: SessionMetadata {
            id: text(session.0)?,
            user_id: text(session.1)?,
            created_at: date(session.2)?,
            updated_at: date(session.3)?,
            expires_at: date(session.4)?,
            ip_address: optional(session.5)?,
            user_agent: optional(session.6)?,
        },
    })
}
