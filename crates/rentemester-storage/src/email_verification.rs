//! Persisted native verification intent. Caller owns the admitted writer through commit.
use rentemester_engine::Error;
use rusqlite::{MAIN_DB, OptionalExtension, Transaction, TransactionState, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;
const PREFIX: &str = "rm-native-email-v1:";
#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Binding {
    subject: String,
    email: String,
    workspace: String,
    facts: String,
}
pub struct Intent<'a> {
    pub digest: &'a [u8; 32],
    pub email: &'a str,
    pub workspace: &'a str,
    pub issued_at_seconds: i64,
    pub expires_at_seconds: i64,
}
fn invalid(_: impl std::fmt::Display) -> Error {
    Error::new("invalid_control", "native email verification facts refused")
}
fn denied() -> Error {
    Error::new("invalid_credentials", "native email verification refused")
}
fn text(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= 4096 && !s.chars().any(char::is_control)
}
fn validate(tx: &Transaction<'_>, intent: &Intent<'_>, now: i64) -> Result<(), Error> {
    if tx.transaction_state(Some(MAIN_DB)).map_err(invalid)? != TransactionState::Write {
        return Err(Error::new(
            "control_fence_required",
            "control writer required",
        ));
    }
    crate::control_migration::validate_current_control(tx).map_err(invalid)?;
    crate::session_control::check_now(now)?;
    if crate::password_credential::lookup_email(intent.email)? != intent.email
        || !text(intent.workspace)
        || intent.workspace.len() > 1024
        || !(0..=253_402_297_199).contains(&intent.issued_at_seconds)
        || intent.expires_at_seconds != intent.issued_at_seconds + 3600
        || now / 1000 < intent.issued_at_seconds
        || now / 1000 >= intent.expires_at_seconds
    {
        return Err(denied());
    }
    Ok(())
}
fn identifier(digest: &[u8; 32]) -> String {
    let hex: String = digest.iter().map(|v| format!("{v:02x}")).collect();
    format!("{PREFIX}{hex}")
}
/// Deliberately independent of workspace access: verifying an address grants no access.
fn current(tx: &Transaction<'_>, email: &str, workspace: &str) -> Result<Option<Binding>, Error> {
    let user: Option<(String, i64, Option<i64>, String, String)> = tx.query_row(
        "SELECT id,emailVerified,twoFactorEnabled,createdAt,updatedAt FROM main.user WHERE email=?1 COLLATE BINARY",
        [email], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))
    ).optional().map_err(invalid)?;
    let Some((subject, verified, mfa, created, updated)) = user else {
        return Ok(None);
    };
    if !text(&subject)
        || !text(&created)
        || !text(&updated)
        || ![0, 1].contains(&verified)
        || !matches!(mfa, None | Some(0 | 1))
    {
        return Err(invalid("user"));
    }
    if verified == 1 {
        return Ok(None);
    }
    let mut hash = Sha256::new();
    for value in [
        "rm-native-email-binding-v1",
        &subject,
        email,
        workspace,
        &created,
        &updated,
        &match mfa {
            None => "null".to_owned(),
            Some(v) => v.to_string(),
        },
    ] {
        hash.update((value.len() as u64).to_be_bytes());
        hash.update(value.as_bytes());
    }
    // Capture every local credential row, including malformed owner/provider coordinates.
    // A change, removal or replacement cannot retain an outstanding intent's authority.
    let mut q=tx.prepare("SELECT id,issuer,accountId,providerId,userId,password,createdAt,updatedAt FROM main.account WHERE userId=?1 COLLATE BINARY OR (issuer='local:credential' COLLATE BINARY AND accountId=?1 COLLATE BINARY) ORDER BY id COLLATE BINARY").map_err(invalid)?;
    let mut rows = q.query([&subject]).map_err(invalid)?;
    let mut count = 0_u64;
    while let Some(r) = rows.next().map_err(invalid)? {
        count += 1;
        if count > 64 {
            return Err(invalid("account bound"));
        }
        for i in 0..8 {
            let value: Option<String> = r.get(i).map_err(invalid)?;
            hash.update([u8::from(value.is_some())]);
            let value = Zeroizing::new(value.unwrap_or_default());
            if value.len() > 16384 {
                return Err(invalid("account bound"));
            }
            hash.update((value.len() as u64).to_be_bytes());
            hash.update(value.as_bytes());
        }
    }
    hash.update(count.to_be_bytes());
    let epoch:i64=tx.query_row("SELECT COUNT(*) FROM main.rm_workspace_auth_state_events WHERE user_id=?1 AND state_transition IN ('credential_updated','email_verified','mfa_enabled','session_deleted')",[&subject],|r|r.get(0)).map_err(invalid)?;
    hash.update(epoch.to_be_bytes());
    Ok(Some(Binding {
        subject,
        email: email.to_owned(),
        workspace: workspace.to_owned(),
        facts: format!("{:x}", hash.finalize()),
    }))
}
/// Unknown/already-verified addresses produce no intent; no bearer is stored in SQLite.
pub fn issue(tx: &Transaction<'_>, intent: &Intent<'_>, id: &str, now: i64) -> Result<bool, Error> {
    validate(tx, intent, now)?;
    if !text(id) || now / 1000 != intent.issued_at_seconds {
        return Err(denied());
    }
    let Some(binding) = current(tx, intent.email, intent.workspace)? else {
        return Ok(false);
    };
    let key = identifier(intent.digest);
    let exists: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM main.verification WHERE identifier=?1 COLLATE BINARY)",
            [&key],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    if exists {
        return Err(invalid("collision"));
    }
    let value = Zeroizing::new(serde_json::to_string(&binding).map_err(invalid)?);
    tx.execute("INSERT INTO main.verification(id,identifier,value,createdAt,updatedAt,expiresAt) VALUES(?1,?2,?3,?4,?4,?5)",params![id,key,value.as_str(),crate::mfa_login::date(intent.issued_at_seconds*1000)?,crate::mfa_login::date(intent.expires_at_seconds*1000)?]).map_err(invalid)?;
    Ok(true)
}
/// Atomically consumes one intent and flips emailVerified. No session, role or MFA grant.
/// Replays, recycled addresses, changed credentials and stale lifecycle contexts fail closed.
pub fn complete(
    tx: &Transaction<'_>,
    intent: &Intent<'_>,
    now: i64,
    audit_key_version: u64,
) -> Result<(), Error> {
    validate(tx, intent, now)?;
    if audit_key_version > 9_007_199_254_740_991 {
        return Err(Error::new("invalid_auth_config", "audit version refused"));
    }
    let key = identifier(intent.digest);
    let mut q=tx.prepare("SELECT id,value,createdAt,updatedAt,expiresAt FROM main.verification WHERE identifier=?1 COLLATE BINARY LIMIT 2").map_err(invalid)?;
    let mut rows = q.query([&key]).map_err(invalid)?;
    let Some(r) = rows.next().map_err(invalid)? else {
        return Err(denied());
    };
    let id: String = r.get(0).map_err(invalid)?;
    let value = Zeroizing::new(r.get::<_, String>(1).map_err(invalid)?);
    let created: String = r.get(2).map_err(invalid)?;
    let updated: String = r.get(3).map_err(invalid)?;
    let expires: String = r.get(4).map_err(invalid)?;
    if !text(&id)
        || value.len() > 16384
        || rows.next().map_err(invalid)?.is_some()
        || created != crate::mfa_login::date(intent.issued_at_seconds * 1000)?
        || updated != created
        || expires != crate::mfa_login::date(intent.expires_at_seconds * 1000)?
    {
        return Err(invalid("intent"));
    }
    drop(rows);
    drop(q);
    let saved: Binding = serde_json::from_str(&value).map_err(invalid)?;
    let actual = current(tx, intent.email, intent.workspace)?.ok_or_else(denied)?;
    if saved != actual {
        return Err(denied());
    }
    if tx.execute("DELETE FROM main.verification WHERE id=?1 COLLATE BINARY AND identifier=?2 COLLATE BINARY",params![id,key]).map_err(invalid)?!=1 {return Err(invalid("consume"));}
    let date = crate::mfa_login::date(now)?;
    if tx.execute("UPDATE main.user SET emailVerified=1,updatedAt=?1 WHERE id=?2 COLLATE BINARY AND email=?3 COLLATE BINARY AND emailVerified=0",params![date,actual.subject,intent.email]).map_err(invalid)?!=1 {return Err(denied());}
    let entity = serde_json::to_string(&(&actual.subject, audit_key_version)).map_err(invalid)?;
    tx.execute("INSERT INTO main.workspace_audit(event_type,entity_type,entity_id,actor,created_at) VALUES('email_verification_completed','native_email_verification_v1',?1,?2,?3)",params![entity,format!("user:{}",actual.subject),date]).map_err(invalid)?;
    tx.execute("INSERT INTO main.rm_workspace_auth_telemetry_events(endpoint,outcome,user_id,identity_key_version,created_at) VALUES('verify-email','accepted',?1,?2,?3)",params![actual.subject,audit_key_version as i64,date]).map_err(invalid)?;
    Ok(())
}
