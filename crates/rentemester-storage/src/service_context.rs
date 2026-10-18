//! Current service credential admission and usage on a held canonical writer.
use crate::{
    cookie_session::timestamp,
    workspace_context::{Catalog, WorkspaceContext},
};
use rentemester_engine::Error;
use rusqlite::{MAIN_DB, OptionalExtension, Transaction, TransactionState, params};
const SAFE: i64 = 9_007_199_254_740_991;
fn invalid(_: impl std::fmt::Display) -> Error {
    Error::new("invalid_control", "service credential facts refused")
}
fn denied() -> Error {
    Error::new("invalid_credentials", "service admission refused")
}
fn text(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= 4096 && !s.chars().any(char::is_control)
}
fn number(n: Option<i64>) -> Result<Option<i64>, Error> {
    if n.is_some_and(|v| !(0..=SAFE).contains(&v)) {
        Err(invalid("number"))
    } else {
        Ok(n)
    }
}
fn date(s: &str, now: i64, created: i64) -> Result<i64, Error> {
    let v = timestamp(s).map_err(invalid)?;
    if v > now || v < created {
        Err(invalid("time"))
    } else {
        Ok(v)
    }
}
fn flag(v: Option<i64>, nullable: bool) -> Result<bool, Error> {
    match v {
        Some(0) => Ok(false),
        Some(1) => Ok(true),
        None if nullable => Ok(false),
        _ => Err(invalid("flag")),
    }
}
/// Caller verifies workspace binding and owns control/catalog lifetime. Hash is sensitive lookup,
/// not a standalone principal. All updates must commit with the protected operation or roll back.
pub fn current(
    tx: &Transaction<'_>,
    hash: &str,
    now: i64,
    catalog: &Catalog,
) -> Result<WorkspaceContext, Error> {
    if tx.transaction_state(Some(MAIN_DB)).map_err(invalid)? != TransactionState::Write {
        return Err(Error::new(
            "control_fence_required",
            "control writer required",
        ));
    }
    crate::control_migration::validate_current_control(tx).map_err(invalid)?;
    crate::session_control::check_now(now)?;
    crate::workspace_context::validate_catalog(catalog)?;
    if hash.len() != 43
        || !hash
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(denied());
    }
    let mut q=tx.prepare("SELECT id,configId,referenceId,enabled,rateLimitEnabled,rateLimitTimeWindow,rateLimitMax,requestCount,remaining,refillInterval,refillAmount,lastRefillAt,lastRequest,expiresAt,createdAt,updatedAt FROM main.apikey WHERE key=?1 COLLATE BINARY LIMIT 2").map_err(invalid)?;
    let mut rows = q.query([hash]).map_err(invalid)?;
    let r = rows.next().map_err(invalid)?.ok_or_else(denied)?;
    let id: String = r.get(0).map_err(invalid)?;
    let config: String = r.get(1).map_err(invalid)?;
    let subject: String = r.get(2).map_err(invalid)?;
    let enabled: Option<i64> = r.get(3).map_err(invalid)?;
    let rate_enabled: Option<i64> = r.get(4).map_err(invalid)?;
    let window = number(r.get(5).map_err(invalid)?)?;
    let max = number(r.get(6).map_err(invalid)?)?;
    let count = number(r.get(7).map_err(invalid)?)?.ok_or_else(|| invalid("count"))?;
    let mut remaining = number(r.get(8).map_err(invalid)?)?;
    let interval = number(r.get(9).map_err(invalid)?)?;
    let amount = number(r.get(10).map_err(invalid)?)?;
    let mut refill: Option<String> = r.get(11).map_err(invalid)?;
    let last: Option<String> = r.get(12).map_err(invalid)?;
    let expires: Option<String> = r.get(13).map_err(invalid)?;
    let created: String = r.get(14).map_err(invalid)?;
    let updated: String = r.get(15).map_err(invalid)?;
    if rows.next().map_err(invalid)?.is_some() {
        return Err(invalid("ambiguous key"));
    }
    if !text(&id) || !text(&subject) || config != "workspace-service-principal" {
        return Err(denied());
    }
    match enabled {
        Some(1) => {}
        Some(0) | None => return Err(denied()),
        _ => return Err(invalid("enabled")),
    }
    let rate_enabled = match rate_enabled {
        None | Some(1) => true,
        Some(0) => false,
        _ => return Err(invalid("rate enabled")),
    };
    if rate_enabled && window.is_some() && max.is_some() && (window == Some(0) || max == Some(0)) {
        return Err(invalid("policy"));
    }
    let created = date(&created, now, 0)?;
    date(&updated, now, created)?;
    let last_ms = last.as_deref().map(|v| date(v, now, created)).transpose()?;
    let refill_ms = refill
        .as_deref()
        .map(|v| date(v, now, created))
        .transpose()?;
    if expires
        .as_deref()
        .map(|v| timestamp(v).map_err(invalid))
        .transpose()?
        .is_some_and(|v| v <= now)
    {
        return Err(denied());
    }
    let user:Option<(String,Option<i64>,Option<i64>)>=tx.query_row("SELECT u.email,u.emailVerified,u.twoFactorEnabled FROM main.user u JOIN main.rm_workspace_service_principals s ON s.user_id=u.id WHERE u.id=?1 COLLATE BINARY",[&subject],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(invalid)?;
    let (email, email_verified, mfa) = user.ok_or_else(denied)?;
    let email_verified = flag(email_verified, false)?;
    let mfa = flag(mfa, true)?;
    let access:Option<(String,Option<String>)>=tx.query_row("SELECT event_type,workspace_role FROM main.rm_workspace_user_access_events WHERE user_id=?1 COLLATE BINARY ORDER BY id DESC LIMIT 1",[&subject],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(invalid)?;
    let role = match access {
        Some((event, Some(role)))
            if event == "activate" && matches!(role.as_str(), "member" | "workspace_owner") =>
        {
            role
        }
        Some((event, None)) if event == "disable" => return Err(denied()),
        None => return Err(denied()),
        _ => return Err(invalid("access")),
    };
    let now_text = crate::mfa_login::date(now)?;
    if let Some(left) = remaining {
        if interval.zip(amount).is_some_and(|(i, _)| {
            i > 0 && amount.is_some_and(|a| a > 0) && now - refill_ms.unwrap_or(created) > i
        }) {
            remaining = Some(amount.expect("validated refill") - 1);
            refill = Some(now_text.clone());
        } else if left > 0 {
            remaining = Some(left - 1);
        } else {
            return Err(denied());
        }
    }
    let (next_count, next_last) = if !rate_enabled {
        (count, Some(now_text.clone()))
    } else if let Some((window, max)) = window.zip(max) {
        if last_ms.is_none_or(|last| now - last > window) {
            (1, Some(now_text.clone()))
        } else if count < max {
            (
                count
                    .checked_add(1)
                    .filter(|n| *n <= SAFE)
                    .ok_or_else(|| invalid("count overflow"))?,
                Some(now_text.clone()),
            )
        } else {
            return Err(denied());
        }
    } else {
        (count, last)
    };
    let context =
        crate::workspace_context::project(tx, subject, role, email, email_verified, mfa, catalog)?;
    if tx.execute("UPDATE main.apikey SET requestCount=?1,remaining=?2,lastRequest=?3,lastRefillAt=?4,updatedAt=?5 WHERE id=?6 COLLATE BINARY AND key=?7 COLLATE BINARY",params![next_count,remaining,next_last,refill,now_text,id,hash]).map_err(invalid)?!=1 {return Err(invalid("usage update"));}
    Ok(context)
}
