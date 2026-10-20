//! Owner-only atomic service lifecycle on an admitted canonical control writer.
use rentemester_engine::Error;
use rusqlite::{OptionalExtension, Transaction, params};
use serde::Serialize;
use zeroize::Zeroizing;
const CONFIG: &str = "workspace-service-principal";
const EVENTS: &str = "rm_workspace_service_principal_events";
const OPS: &str = "rm_workspace_service_principal_operation_events";
#[derive(Clone)]
pub enum Operation {
    List,
    Create {
        display_name: String,
        operation_id: Option<String>,
        expires_in_seconds: Option<u64>,
    },
    Rotate {
        subject: String,
        credential: String,
        operation_id: Option<String>,
        expires_in_seconds: Option<u64>,
    },
    Revoke {
        subject: String,
        credential: String,
        operation_id: Option<String>,
    },
    Recover {
        operation_id: String,
    },
}
pub struct Material<'a> {
    pub operation_id: &'a str,
    pub marker: &'a str,
    pub subject: &'a str,
    pub credential: &'a str,
    pub secret: Option<&'a str>,
    pub hash: Option<&'a str>,
}
/// Bounded prepared HTTP receipt. No Debug/Clone/Serialize; issuance is transient sensitive data.
pub struct Receipt {
    json: Zeroizing<String>,
    status: u16,
}
impl Receipt {
    pub fn as_json(&self) -> &str {
        &self.json
    }
    pub fn status(&self) -> u16 {
        self.status
    }
}
fn invalid(_: impl std::fmt::Display) -> Error {
    Error::new("invalid_control", "service lifecycle facts refused")
}
fn bad() -> Error {
    Error::new("invalid_request", "service lifecycle input refused")
}
fn denied() -> Error {
    Error::new("invalid_credentials", "service lifecycle owner refused")
}
fn text(s: &str, max: usize) -> bool {
    !s.trim().is_empty() && s.len() <= max && !s.chars().any(char::is_control)
}
pub fn uuid(value: &str) -> Result<String, Error> {
    let s = value.trim().to_ascii_lowercase();
    let b = s.as_bytes();
    if b.len() != 36
        || !b.iter().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                *b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
        || !matches!(b[14], b'1'..=b'8')
        || !matches!(b[19], b'8' | b'9' | b'a' | b'b')
    {
        return Err(bad());
    }
    Ok(s)
}
fn marker(s: &str) -> bool {
    s.len() == 32
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn stamp(s: &str) -> Result<i64, Error> {
    if !s.is_ascii() {
        return Err(invalid("timestamp"));
    }
    let normalized = if s.len() == 19 && s.as_bytes().get(10) == Some(&b' ') {
        format!("{}T{}.000Z", &s[..10], &s[11..])
    } else {
        s.to_owned()
    };
    crate::cookie_session::timestamp(&normalized).map_err(invalid)
}
fn time(s: &str, now: i64) -> Result<(), Error> {
    if stamp(s)? > now {
        return Err(invalid("future"));
    }
    Ok(())
}
fn receipt<T: Serialize>(v: &T, status: u16) -> Result<Receipt, Error> {
    let s = Zeroizing::new(serde_json::to_string(v).map_err(invalid)?);
    if s.len() > 1_048_576 {
        return Err(invalid("receipt bound"));
    }
    Ok(Receipt { json: s, status })
}
fn stream(tx: &Transaction<'_>, table: &str) -> Result<(), Error> {
    let (n, min, max): (i64, Option<i64>, Option<i64>) = tx
        .query_row(
            &format!("SELECT COUNT(*),MIN(id),MAX(id) FROM main.{table}"),
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(invalid)?;
    if n > 0 && (min.is_none_or(|v| v <= 0) || max == Some(i64::MAX)) {
        return Err(invalid("event order exhausted"));
    }
    Ok(())
}
fn next(tx: &Transaction<'_>, table: &str) -> Result<i64, Error> {
    let n: i64 = tx
        .query_row(
            &format!("SELECT COALESCE(MAX(id),0) FROM main.{table}"),
            [],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    n.checked_add(1)
        .filter(|n| *n > 0)
        .ok_or_else(|| invalid("event order exhausted"))
}
fn event(
    tx: &Transaction<'_>,
    subject: &str,
    credential: Option<&str>,
    kind: &str,
    actor: &str,
    now: &str,
) -> Result<(), Error> {
    let id = next(tx, EVENTS)?;
    tx.execute("INSERT INTO main.rm_workspace_service_principal_events(id,user_id,credential_id,event_type,actor,created_at) VALUES(?1,?2,?3,?4,?5,?6)",params![id,subject,credential,kind,actor,now]).map_err(invalid)?;
    Ok(())
}
struct Record<'a> {
    id: &'a str,
    kind: &'a str,
    subject: &'a str,
    old: Option<&'a str>,
    marker: &'a str,
}
fn append(
    tx: &Transaction<'_>,
    r: &Record<'_>,
    state: &str,
    replacement: Option<&str>,
    actor: &str,
    now: &str,
) -> Result<(), Error> {
    let id = next(tx, OPS)?;
    let old = if r.kind == "create" && state == "completed" {
        replacement
    } else {
        r.old
    };
    tx.execute("INSERT INTO main.rm_workspace_service_principal_operation_events(id,operation_id,operation_type,operation_status,user_id,credential_id,replacement_credential_id,provider_operation_marker,recovery_claim_id,actor,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,NULL,?9,?10)",params![id,r.id,r.kind,state,r.subject,old,replacement,r.marker,actor,now]).map_err(invalid)?;
    Ok(())
}
fn registered(tx: &Transaction<'_>, subject: &str) -> Result<(), Error> {
    if !text(subject,4096)||!tx.query_row("SELECT EXISTS(SELECT 1 FROM main.rm_workspace_service_principals p JOIN main.user u ON u.id=p.user_id WHERE p.user_id=?1 COLLATE BINARY)",[subject],|r|r.get::<_,bool>(0)).map_err(invalid)?{return Err(bad());}
    Ok(())
}
fn key(
    tx: &Transaction<'_>,
    subject: &str,
    credential: &str,
    missing: bool,
) -> Result<bool, Error> {
    if !text(credential, 4096) {
        return Err(bad());
    }
    let row: Option<(String, String, Option<i64>)> = tx
        .query_row(
            "SELECT referenceId,configId,enabled FROM main.apikey WHERE id=?1 COLLATE BINARY",
            [credential],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .map_err(invalid)?;
    match row {
        Some((s, c, e)) if s == subject && c == CONFIG && matches!(e, None | Some(0 | 1)) => {
            Ok(true)
        }
        None if missing => Ok(false),
        _ => Err(bad()),
    }
}
fn disable(tx: &Transaction<'_>, credential: &str, now: &str) -> Result<(), Error> {
    if tx
        .execute(
            "UPDATE main.apikey SET enabled=0,updatedAt=?1 WHERE id=?2 COLLATE BINARY",
            params![now, credential],
        )
        .map_err(invalid)?
        != 1
    {
        return Err(invalid("disable"));
    }
    Ok(())
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Credential {
    credential_id: String,
    enabled: Option<bool>,
    expires_at: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Principal {
    service_account_id: String,
    display_name: String,
    created_at: String,
    active_credential_count: usize,
    credentials: Vec<Credential>,
}
fn inventory(tx: &Transaction<'_>, now: i64) -> Result<Receipt, Error> {
    let mut q=tx.prepare("SELECT user_id,display_name,created_at FROM main.rm_workspace_service_principals ORDER BY created_at,user_id COLLATE BINARY LIMIT 1001").map_err(invalid)?;
    let mut rows = q.query([]).map_err(invalid)?;
    let mut principals = Vec::new();
    let mut count = 0;
    while let Some(r) = rows.next().map_err(invalid)? {
        if principals.len() >= 1000 {
            return Err(invalid("inventory bound"));
        }
        let subject: String = r.get(0).map_err(invalid)?;
        registered(tx, &subject)?;
        let name: String = r.get(1).map_err(invalid)?;
        let created: String = r.get(2).map_err(invalid)?;
        if !text(&subject, 4096) || !text(&name, 1024) || name.chars().count() > 120 {
            return Err(invalid("inventory metadata"));
        }
        time(&created, now)?;
        let mut kq=tx.prepare("SELECT id,enabled,expiresAt FROM main.apikey WHERE referenceId=?1 COLLATE BINARY AND configId=?2 COLLATE BINARY ORDER BY id COLLATE BINARY LIMIT 1001").map_err(invalid)?;
        let mut kr = kq.query(params![subject, CONFIG]).map_err(invalid)?;
        let mut credentials = Vec::new();
        let mut active = 0;
        while let Some(k) = kr.next().map_err(invalid)? {
            count += 1;
            if count > 1000 {
                return Err(invalid("credential bound"));
            }
            let id: String = k.get(0).map_err(invalid)?;
            let enabled: Option<i64> = k.get(1).map_err(invalid)?;
            let expiry: Option<String> = k.get(2).map_err(invalid)?;
            if !text(&id, 4096) || !matches!(enabled, None | Some(0 | 1)) {
                return Err(invalid("credential metadata"));
            }
            if let Some(s) = &expiry {
                crate::cookie_session::timestamp(s).map_err(invalid)?;
            }
            if enabled != Some(0) {
                active += 1;
            }
            credentials.push(Credential {
                credential_id: id,
                enabled: enabled.map(|n| n == 1),
                expires_at: expiry,
            });
        }
        principals.push(Principal {
            service_account_id: subject,
            display_name: name,
            created_at: created,
            active_credential_count: active,
            credentials,
        });
    }
    receipt(&serde_json::json!({"ok":true,"principals":principals}), 200)
}
struct History {
    credential: Option<String>,
    kind: String,
    state: String,
    subject: String,
    old: Option<String>,
    replacement: Option<String>,
    marker: String,
    at: i64,
}
fn history(tx: &Transaction<'_>, id: &str, now: i64) -> Result<Option<History>, Error> {
    let mut q=tx.prepare("SELECT operation_type,operation_status,user_id,credential_id,replacement_credential_id,provider_operation_marker,recovery_claim_id,actor,created_at FROM main.rm_workspace_service_principal_operation_events WHERE operation_id=?1 COLLATE BINARY ORDER BY id LIMIT 1001").map_err(invalid)?;
    let mut rows = q.query([id]).map_err(invalid)?;
    let mut last: Option<History> = None;
    let mut count = 0;
    while let Some(r) = rows.next().map_err(invalid)? {
        count += 1;
        if count > 1000 {
            return Err(invalid("operation history bound"));
        }
        let kind: String = r.get(0).map_err(invalid)?;
        let state: String = r.get(1).map_err(invalid)?;
        let subject: String = r.get(2).map_err(invalid)?;
        let cred: Option<String> = r.get(3).map_err(invalid)?;
        let replacement: Option<String> = r.get(4).map_err(invalid)?;
        let marker_value: String = r.get(5).map_err(invalid)?;
        let claim: Option<String> = r.get(6).map_err(invalid)?;
        let actor: String = r.get(7).map_err(invalid)?;
        let at: String = r.get(8).map_err(invalid)?;
        if !matches!(kind.as_str(), "create" | "rotate" | "revoke")
            || !text(&subject, 4096)
            || !marker(&marker_value)
            || !text(&actor, 160)
            || actor.chars().count() < 3
            || cred.as_ref().is_some_and(|v| !text(v, 4096))
            || replacement.as_ref().is_some_and(|v| !text(v, 4096))
        {
            return Err(invalid("operation fields"));
        }
        let at = stamp(&at)?;
        if at > now {
            return Err(invalid("future operation"));
        }
        if (state == "recovering") != claim.is_some()
            || claim.as_deref().is_some_and(|s| uuid(s).is_err())
        {
            return Err(invalid("claim"));
        }
        if kind == "revoke" && state == "recovered" {
            return Err(invalid("revoke recovery state"));
        }
        let credential = cred.clone();
        let old = if kind == "create" {
            if state == "completed" {
                if cred.is_none() || cred != replacement {
                    return Err(invalid("create receipt"));
                }
            } else if cred.is_some() {
                return Err(invalid("create target"));
            }
            None
        } else {
            if cred.is_none() {
                return Err(invalid("missing target"));
            }
            cred
        };
        if state == "completed" {
            if kind == "rotate" && (replacement.is_none() || replacement == old)
                || kind == "revoke" && replacement.is_some()
            {
                return Err(invalid("replacement"));
            }
        } else if replacement.is_some() {
            return Err(invalid("nonterminal replacement"));
        }
        if let Some(prev) = &last {
            if prev.kind != kind
                || prev.subject != subject
                || prev.old != old
                || prev.marker != marker_value
                || !matches!(
                    (prev.state.as_str(), state.as_str()),
                    ("pending", "completed" | "failed" | "recovering")
                        | ("failed", "recovering" | "recovered")
                        | ("recovering", "recovering" | "completed" | "failed")
                )
            {
                return Err(invalid("operation transition"));
            }
        } else if state != "pending" {
            return Err(invalid("operation initial state"));
        }
        last = Some(History {
            credential,
            kind,
            state,
            subject,
            old,
            replacement,
            marker: marker_value,
            at,
        });
    }
    if let Some(h) = &last
        && tx.query_row("SELECT EXISTS(SELECT 1 FROM main.rm_workspace_service_principal_operation_events WHERE provider_operation_marker=?1 COLLATE BINARY AND operation_id<>?2 COLLATE BINARY)",params![h.marker,id],|r|r.get::<_,bool>(0)).map_err(invalid)?{return Err(invalid("ambiguous marker"));}
    Ok(last)
}
/// Read-only admission after the caller has recognized the complete legacy
/// control schema. Old prefixes without service operations have nothing to drain.
/// A failed provider operation is not proof that a credential was disabled.
pub fn validate_adoption_retirement(db:&rusqlite::Connection,now:i64)->Result<(),Error>{
    let tx=db.unchecked_transaction().map_err(invalid)?;
    let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name=?1)",[OPS],|r|r.get(0)).map_err(invalid)?;
    if !exists {tx.commit().map_err(invalid)?;return Ok(());}
    stream(&tx,OPS)?;
    let ids={let mut q=tx.prepare("SELECT DISTINCT operation_id FROM rm_workspace_service_principal_operation_events ORDER BY operation_id COLLATE BINARY LIMIT 10001").map_err(invalid)?;q.query_map([],|r|r.get::<_,String>(0)).map_err(invalid)?.collect::<Result<Vec<_>,_>>().map_err(invalid)?};
    if ids.len()>10000 {return Err(invalid("operation inventory bound"));}
    for id in ids {
        uuid(&id).map_err(invalid)?;
        let h=history(&tx,&id,now)?.ok_or_else(||invalid("operation history missing"))?;
        if !matches!(h.state.as_str(),"completed"|"recovered") || h.kind=="revoke"&&h.state=="recovered" {
            return Err(Error::new("legacy_retirement_refused","unresolved service provider operation requires recovery before adoption"));
        }
    }
    tx.commit().map_err(invalid)
}
fn recovered(id: &str, h: &History, recovered: bool) -> Result<Receipt, Error> {
    receipt(
        &serde_json::json!({"ok":true,"operationId":id,"status":h.state,"recovered":recovered,"serviceAccountId":h.subject,"credentialId":h.credential,"replacementCredentialId":h.replacement}),
        200,
    )
}
fn recover(
    tx: &Transaction<'_>,
    id: &str,
    actor: &str,
    now: i64,
    when: &str,
) -> Result<Receipt, Error> {
    let id = uuid(id)?;
    let mut h = history(tx, &id, now)?.ok_or_else(bad)?;
    registered(tx, &h.subject)?;
    match h.state.as_str() {
        "completed" => return recovered(&id, &h, false),
        "recovered" => return recovered(&id, &h, true),
        "recovering" if now - h.at < 30000 => return recovered(&id, &h, false),
        _ => {}
    }
    let record = Record {
        id: &id,
        kind: &h.kind,
        subject: &h.subject,
        old: h.old.as_deref(),
        marker: &h.marker,
    };
    if h.kind == "revoke" {
        let credential = h.old.as_deref().ok_or_else(|| invalid("revoke target"))?;
        if key(tx, &h.subject, credential, true)? {
            disable(tx, credential, when)?;
            event(
                tx,
                &h.subject,
                Some(credential),
                "credential_revoked",
                actor,
                when,
            )?;
        }
        append(tx, &record, "completed", None, actor, when)?;
        h.state = "completed".to_owned();
    } else {
        let mut q=tx.prepare("SELECT id,enabled FROM main.apikey WHERE referenceId=?1 COLLATE BINARY AND configId=?2 COLLATE BINARY AND name=?3 COLLATE BINARY LIMIT 1001").map_err(invalid)?;
        let rows = q
            .query_map(params![h.subject, CONFIG, h.marker], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, Option<i64>>(1)?))
            })
            .map_err(invalid)?;
        let keys = rows.collect::<Result<Vec<_>, _>>().map_err(invalid)?;
        if keys.len() > 1000 {
            return Err(invalid("recovery key bound"));
        }
        for (id, enabled) in keys {
            if !text(&id, 4096) || !matches!(enabled, None | Some(0 | 1)) {
                return Err(invalid("recovery key"));
            }
            if enabled != Some(0) {
                disable(tx, &id, when)?;
            }
        }
        if h.state != "failed" {
            append(tx, &record, "failed", None, actor, when)?;
        }
        append(tx, &record, "recovered", None, actor, when)?;
        h.state = "recovered".to_owned();
    }
    recovered(&id, &h, true)
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Issued<'a> {
    ok: bool,
    operation_id: &'a str,
    service_account_id: &'a str,
    credential_id: &'a str,
    secret: &'a str,
}
/// Material is trusted runtime output; caller must bind its hash to the generated secret.
pub fn execute(
    tx: &Transaction<'_>,
    current: &crate::cookie_session::SessionIdentity,
    op: &Operation,
    material: Option<Material<'_>>,
    now: i64,
) -> Result<Receipt, Error> {
    crate::session_control::authorize(tx, current)?;
    let facts =
        crate::control_access::observe_human(tx, &current.subject, &current.session_id, "")?;
    if facts.workspace_role.as_deref() != Some("workspace_owner") {
        return Err(denied());
    }
    crate::session_control::check_now(now)?;
    stream(tx, EVENTS)?;
    stream(tx, OPS)?;
    if matches!(op, Operation::List) {
        return inventory(tx, now);
    }
    let actor = format!("user:{}", current.subject);
    if !text(&actor, 160) {
        return Err(invalid("actor"));
    }
    let when = crate::mfa_login::date(now)?;
    if let Operation::Recover { operation_id } = op {
        return recover(tx, operation_id, &actor, now, &when);
    }
    let m = material.ok_or_else(|| invalid("runtime material"))?;
    let (kind, target, old, requested, expiry, name) = match op {
        Operation::Create {
            display_name,
            operation_id,
            expires_in_seconds,
        } => (
            "create",
            m.subject,
            None,
            operation_id,
            expires_in_seconds,
            Some(display_name.trim()),
        ),
        Operation::Rotate {
            subject,
            credential,
            operation_id,
            expires_in_seconds,
        } => (
            "rotate",
            subject.as_str(),
            Some(credential.as_str()),
            operation_id,
            expires_in_seconds,
            None,
        ),
        Operation::Revoke {
            subject,
            credential,
            operation_id,
        } => (
            "revoke",
            subject.as_str(),
            Some(credential.as_str()),
            operation_id,
            &None,
            None,
        ),
        _ => return Err(bad()),
    };
    let id = uuid(requested.as_deref().unwrap_or(m.operation_id))?;
    if history(tx, &id, now)?.is_some() {
        return Err(Error::new(
            "operation_reused",
            "service operation already exists; no secret redisclosure",
        ));
    }
    if !marker(m.marker) || !text(target, 4096) || !text(m.credential, 4096) {
        return Err(invalid("runtime identifiers"));
    }
    if tx.query_row("SELECT EXISTS(SELECT 1 FROM main.rm_workspace_service_principal_operation_events WHERE provider_operation_marker=?1)",[m.marker],|r|r.get::<_,bool>(0)).map_err(invalid)?{return Err(invalid("runtime marker collision"));}
    if kind == "create" {
        let name = name.ok_or_else(bad)?;
        if !text(name, 1024) || name.chars().count() > 120 {
            return Err(bad());
        }
        tx.execute("INSERT INTO main.user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt) VALUES(?1,?2,?3,0,0,?4,?4)",params![target,name,format!("{target}@service.invalid"),when]).map_err(invalid)?;
        tx.execute("INSERT INTO main.rm_workspace_service_principals(user_id,display_name,created_at) VALUES(?1,?2,?3)",params![target,name,when]).map_err(invalid)?;
        event(tx, target, None, "created", &actor, &when)?;
    } else {
        registered(tx, target)?;
        key(tx, target, old.ok_or_else(bad)?, false)?;
    }
    let record = Record {
        id: &id,
        kind,
        subject: target,
        old,
        marker: m.marker,
    };
    append(tx, &record, "pending", None, &actor, &when)?;
    if kind == "revoke" {
        let old = old.ok_or_else(bad)?;
        disable(tx, old, &when)?;
        event(tx, target, Some(old), "credential_revoked", &actor, &when)?;
        append(tx, &record, "completed", None, &actor, &when)?;
        return receipt(
            &serde_json::json!({"ok":true,"operationId":id,"revoked":true}),
            200,
        );
    }
    let seconds = expiry.unwrap_or(7776000);
    if !(86400..=31536000).contains(&seconds) {
        return Err(bad());
    }
    let expires = now
        .checked_add(i64::try_from(seconds * 1000).map_err(invalid)?)
        .filter(|n| *n <= 253402300799999)
        .ok_or_else(|| invalid("expiry"))?;
    let expires = crate::mfa_login::date(expires)?;
    let secret = m.secret.ok_or_else(|| invalid("runtime key"))?;
    let hash = m.hash.ok_or_else(|| invalid("runtime hash"))?;
    if secret.len() != 68
        || !secret.starts_with("rms_")
        || !secret.as_bytes()[4..].iter().all(u8::is_ascii_alphabetic)
        || hash.len() != 43
        || !hash
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(invalid("runtime key"));
    }
    if tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM main.apikey WHERE key=?1 COLLATE BINARY)",
            [hash],
            |r| r.get::<_, bool>(0),
        )
        .map_err(invalid)?
    {
        return Err(invalid("key collision"));
    }
    tx.execute("INSERT INTO main.apikey(id,configId,name,start,referenceId,prefix,key,enabled,rateLimitEnabled,rateLimitTimeWindow,rateLimitMax,requestCount,createdAt,updatedAt,expiresAt) VALUES(?1,?2,?3,?4,?5,'rms_',?6,1,1,60000,600,0,?7,?7,?8)",params![m.credential,CONFIG,m.marker,&secret[..6],target,hash,when,expires]).map_err(invalid)?;
    event(
        tx,
        target,
        Some(m.credential),
        if kind == "create" {
            "credential_issued"
        } else {
            "credential_rotated"
        },
        &actor,
        &when,
    )?;
    if let Some(old) = old {
        disable(tx, old, &when)?;
        event(tx, target, Some(old), "credential_revoked", &actor, &when)?;
    }
    append(tx, &record, "completed", Some(m.credential), &actor, &when)?;
    receipt(
        &Issued {
            ok: true,
            operation_id: &id,
            service_account_id: target,
            credential_id: m.credential,
            secret,
        },
        if kind == "create" { 201 } else { 200 },
    )
}

#[cfg(test)]
mod adoption_retirement_tests {
    use super::*;
    fn fixture()->rusqlite::Connection{
        let db=rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE rm_workspace_service_principal_operation_events(id INTEGER PRIMARY KEY,operation_id TEXT,operation_type TEXT,operation_status TEXT,user_id TEXT,credential_id TEXT,replacement_credential_id TEXT,provider_operation_marker TEXT,recovery_claim_id TEXT,actor TEXT,created_at TEXT)").unwrap();db
    }
    fn append(db:&rusqlite::Connection,state:&str){
        let credential=if state=="completed"{Some("synthetic-credential")}else{None::<&str>};
        db.execute("INSERT INTO rm_workspace_service_principal_operation_events(operation_id,operation_type,operation_status,user_id,credential_id,replacement_credential_id,provider_operation_marker,actor,created_at) VALUES('00000000-0000-4000-8000-000000000001','create',?1,'synthetic-service',?2,?2,'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa','user:synthetic','2026-01-01 00:00:00')",params![state,credential]).unwrap();
    }
    #[test]
    fn failed_provider_effect_blocks_until_valid_terminal_recovery(){
        let db=fixture();let now=stamp("2026-01-02 00:00:00").unwrap();append(&db,"pending");assert!(validate_adoption_retirement(&db,now).is_err());append(&db,"failed");assert!(validate_adoption_retirement(&db,now).is_err());append(&db,"recovered");validate_adoption_retirement(&db,now).unwrap();
    }
    #[test]
    fn completed_requires_valid_history_and_missing_legacy_extension_is_empty(){
        let db=fixture();let now=stamp("2026-01-02 00:00:00").unwrap();append(&db,"completed");assert!(validate_adoption_retirement(&db,now).is_err());let db=fixture();append(&db,"pending");append(&db,"completed");validate_adoption_retirement(&db,now).unwrap();let empty=rusqlite::Connection::open_in_memory().unwrap();validate_adoption_retirement(&empty,now).unwrap();
    }
}
