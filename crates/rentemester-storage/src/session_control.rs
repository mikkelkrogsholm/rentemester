//! Own-session lifecycle on a held canonical control writer, never a bearer list.
use crate::{cookie_session::SessionIdentity, mfa_login::date};
use rentemester_engine::Error;
use rusqlite::{MAIN_DB, OptionalExtension, Transaction, TransactionState, params};
use serde::Serialize;
#[derive(Clone, Copy)]
pub enum Operation<'a> {
    List,
    SignOut,
    RevokeOne(&'a str),
    RevokeOthers,
    RevokeAll,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub id: String,
    pub created_at: String,
    pub expires_at: String,
    pub current: bool,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
}
pub struct Effect {
    pub sessions: Vec<SessionSummary>,
    pub deleted: usize,
    pub clear: bool,
}
fn invalid(_: impl std::fmt::Display) -> Error {
    Error::new("invalid_control", "session control facts refused")
}
fn denied() -> Error {
    Error::new(
        "invalid_credentials",
        "session control authorization refused",
    )
}
fn bounded(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}
fn writer(tx: &Transaction<'_>) -> Result<(), Error> {
    if tx.transaction_state(Some(MAIN_DB)).map_err(invalid)? != TransactionState::Write {
        return Err(Error::new(
            "control_fence_required",
            "control writer required",
        ));
    }
    crate::control_migration::validate_current_control(tx).map_err(invalid)
}
pub fn check_now(now: i64) -> Result<(), Error> {
    if !(0..=253_402_300_799_999).contains(&now) {
        Err(Error::new("invalid_auth_clock", "session clock refused"))
    } else {
        Ok(())
    }
}
pub fn authorize(tx: &Transaction<'_>, current: &SessionIdentity) -> Result<(), Error> {
    writer(tx)?;
    if !bounded(&current.subject, 4096) || !bounded(&current.session_id, 4096) {
        return Err(invalid("identity"));
    }
    let f = crate::control_access::observe_human(tx, &current.subject, &current.session_id, "")?;
    if !f.user_exists
        || !f.email_verified
        || !f.mfa_enabled
        || !f.session_current
        || f.workspace_role.is_none()
    {
        return Err(denied());
    }
    let mut q = tx
        .prepare("SELECT id,verified FROM main.twoFactor WHERE userId=?1 COLLATE BINARY LIMIT 2")
        .map_err(invalid)?;
    let mut rows = q.query([&current.subject]).map_err(invalid)?;
    let Some(r) = rows.next().map_err(invalid)? else {
        return Err(denied());
    };
    let id: String = r.get(0).map_err(invalid)?;
    let verified: Option<i64> = r.get(1).map_err(invalid)?;
    if !bounded(&id, 4096) || rows.next().map_err(invalid)?.is_some() {
        return Err(invalid("factor"));
    }
    if verified != Some(1) {
        return Err(denied());
    }
    Ok(())
}
fn display(value: Option<String>, max: usize) -> Result<Option<String>, Error> {
    if value
        .as_ref()
        .is_some_and(|s| s.len() > max || s.chars().any(char::is_control))
    {
        Err(invalid("display metadata"))
    } else {
        Ok(value)
    }
}
fn list(
    tx: &Transaction<'_>,
    current: &SessionIdentity,
    now: i64,
) -> Result<Vec<SessionSummary>, Error> {
    let mut q=tx.prepare("SELECT id,createdAt,expiresAt,ipAddress,userAgent FROM main.session WHERE userId=?1 COLLATE BINARY ORDER BY createdAt DESC,id COLLATE BINARY ASC LIMIT 1001").map_err(invalid)?;
    let mut rows = q.query([&current.subject]).map_err(invalid)?;
    let mut output = Vec::new();
    let mut count = 0;
    let mut bytes = 0;
    while let Some(r) = rows.next().map_err(invalid)? {
        count += 1;
        if count > 1000 {
            return Err(Error::new(
                "session_listing_limit",
                "session listing bound exceeded",
            ));
        }
        let id: String = r.get(0).map_err(invalid)?;
        let created_at: String = r.get(1).map_err(invalid)?;
        let expires_at: String = r.get(2).map_err(invalid)?;
        if !bounded(&id, 4096) {
            return Err(invalid("id"));
        }
        let created = crate::cookie_session::timestamp(&created_at).map_err(invalid)?;
        let expires = crate::cookie_session::timestamp(&expires_at).map_err(invalid)?;
        if created > now || expires <= created || expires - created > 43_200_000 {
            return Err(invalid("time"));
        }
        let ip_address = display(r.get(3).map_err(invalid)?, 256)?;
        let user_agent = display(r.get(4).map_err(invalid)?, 2048)?;
        if expires <= now {
            continue;
        }
        bytes += id.len()
            + created_at.len()
            + expires_at.len()
            + ip_address.as_ref().map_or(0, String::len)
            + user_agent.as_ref().map_or(0, String::len);
        if bytes > 1_048_576 {
            return Err(Error::new(
                "session_listing_limit",
                "session listing bound exceeded",
            ));
        }
        let is_current = id == current.session_id;
        output.push(SessionSummary {
            id,
            created_at,
            expires_at,
            current: is_current,
            ip_address,
            user_agent,
        });
    }
    if serde_json::to_vec(&output).map_err(invalid)?.len() > 1_048_576 {
        return Err(Error::new(
            "session_listing_limit",
            "session listing bound exceeded",
        ));
    }
    Ok(output)
}
fn audit(
    tx: &Transaction<'_>,
    identity: &SessionIdentity,
    event: &str,
    target: Option<&str>,
    deleted: usize,
    version: u64,
    now: i64,
) -> Result<(), Error> {
    let SessionIdentity {
        subject,
        session_id: current,
    } = identity;
    if version > 9_007_199_254_740_991 {
        return Err(Error::new("invalid_auth_config", "audit version refused"));
    }
    let entity =
        serde_json::to_string(&(subject, current, version, target, deleted)).map_err(invalid)?;
    tx.execute("INSERT INTO main.workspace_audit(event_type,entity_type,entity_id,actor,created_at) VALUES(?1,'native_session_control_v1',?2,?3,?4)",params![event,entity,format!("user:{subject}"),date(now)?]).map_err(invalid)?;
    Ok(())
}
pub fn sign_out(
    tx: &Transaction<'_>,
    token: &str,
    version: u64,
    now: i64,
) -> Result<Effect, Error> {
    writer(tx)?;
    check_now(now)?;
    let row: Option<(String, String)> = tx
        .query_row(
            "SELECT id,userId FROM main.session WHERE token=?1 COLLATE BINARY",
            [token],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(invalid)?;
    let mut deleted = 0;
    if let Some((id, subject)) = row {
        if !bounded(&id, 4096) || !bounded(&subject, 4096) {
            return Err(invalid("logout identity"));
        }
        deleted = tx
            .execute(
                "DELETE FROM main.session WHERE id=?1 AND token=?2 COLLATE BINARY",
                params![id, token],
            )
            .map_err(invalid)?;
        if deleted != 1 {
            return Err(invalid("delete"));
        }
        audit(
            tx,
            &SessionIdentity {
                subject,
                session_id: id,
            },
            "session_signed_out",
            None,
            deleted,
            version,
            now,
        )?;
    }
    Ok(Effect {
        sessions: Vec::new(),
        deleted,
        clear: true,
    })
}
pub fn execute(
    tx: &Transaction<'_>,
    current: &SessionIdentity,
    op: Operation<'_>,
    version: u64,
    now: i64,
) -> Result<Effect, Error> {
    authorize(tx, current)?;
    check_now(now)?;
    let mut sessions = Vec::new();
    let mut target = None;
    let (event, deleted, clear) = match op {
        Operation::List => {
            sessions = list(tx, current, now)?;
            ("session_listed", 0, false)
        }
        Operation::RevokeOne(id) => {
            if !bounded(id, 4096) {
                return Err(Error::new("invalid_request", "session target refused"));
            }
            target = Some(id);
            let deleted = tx
                .execute(
                    "DELETE FROM main.session WHERE id=?1 AND userId=?2 COLLATE BINARY",
                    params![id, current.subject],
                )
                .map_err(invalid)?;
            (
                "session_revoked",
                deleted,
                deleted == 1 && id == current.session_id,
            )
        }
        Operation::RevokeOthers => {
            let n=tx.execute("DELETE FROM main.session WHERE userId=?1 COLLATE BINARY AND id<>?2 COLLATE BINARY",params![current.subject,current.session_id]).map_err(invalid)?;
            ("other_sessions_revoked", n, false)
        }
        Operation::RevokeAll => {
            let n = tx
                .execute(
                    "DELETE FROM main.session WHERE userId=?1 COLLATE BINARY",
                    [&current.subject],
                )
                .map_err(invalid)?;
            ("sessions_revoked", n, true)
        }
        Operation::SignOut => return Err(invalid("logout dispatch")),
    };
    audit(tx, current, event, target, deleted, version, now)?;
    Ok(Effect {
        sessions,
        deleted,
        clear,
    })
}
