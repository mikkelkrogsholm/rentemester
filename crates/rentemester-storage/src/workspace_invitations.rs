//! Email-bound invitation consumption; no sessions or verified flags are created here.
use crate::{workspace_context::Catalog, workspace_members as members};
use rentemester_engine::Error;
use rusqlite::{OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
fn invalid(_: impl std::fmt::Display) -> Error {
    Error::new(
        "invitation_refused",
        "invitation is invalid, expired or already used",
    )
}
fn audit(tx: &Transaction<'_>, event: &str, id: &str, actor: &str) -> Result<(), Error> {
    tx.execute("INSERT INTO main.workspace_audit(event_type,entity_type,entity_id,actor) VALUES(?1,'workspace_invitation',?2,?3)",params![event,id,actor]).map_err(invalid)?;
    Ok(())
}
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Invitation {
    pub invitation_id: String,
    pub email: String,
    pub workspace_role: String,
    pub company_slug: String,
    pub company_role: String,
    pub expires_at: String,
    pub status: String,
    pub user_id: Option<String>,
    pub created_at: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Issue {
    pub email: String,
    pub workspace_role: String,
    pub company_slug: String,
    pub company_role: String,
}
pub fn canonical(email: &str) -> Result<String, Error> {
    let email = crate::password_credential::lookup_email(email.trim())?;
    if email.len() > 320 {
        return Err(invalid("email"));
    }
    Ok(email)
}
fn by_id(tx: &Transaction<'_>, id: &str) -> Result<Invitation, Error> {
    tx.query_row("SELECT i.invitation_id,i.canonical_email,i.workspace_role,i.company_slug,i.company_role,i.expires_at,e.event_type,e.user_id,i.created_at FROM main.rm_workspace_invitation_events i JOIN main.rm_workspace_invitation_events e ON e.id=(SELECT MAX(id) FROM main.rm_workspace_invitation_events WHERE invitation_id=i.invitation_id) WHERE i.invitation_id=?1 AND i.event_type='issued'",[id],|r|Ok(Invitation{invitation_id:r.get(0)?,email:r.get(1)?,workspace_role:r.get(2)?,company_slug:r.get(3)?,company_role:r.get(4)?,expires_at:r.get(5)?,status:r.get(6)?,user_id:r.get(7)?,created_at:r.get(8)?})).map_err(invalid)
}
pub fn list(
    tx: &Transaction<'_>,
    catalog: &Catalog,
    admin: &str,
) -> Result<Vec<Invitation>, Error> {
    members::writer(tx)?;
    let mut q=tx.prepare("SELECT invitation_id FROM main.rm_workspace_invitation_events WHERE event_type='issued' ORDER BY id DESC LIMIT 10001").map_err(invalid)?;
    let ids = q
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    if ids.len() > 10000 {
        return Err(invalid("bound"));
    }
    let mut out = vec![];
    for id in ids {
        let i = by_id(tx, &id)?;
        if members::company(catalog, &i.company_slug, false).is_ok()
            && members::membership(tx, admin, &i.company_slug)?.as_deref() == Some("owner")
        {
            out.push(i);
        }
    }
    Ok(out)
}
pub struct Issuance<'a> {
    pub id: &'a str,
    pub token_hash: &'a str,
    pub email_hash: &'a str,
    pub now: i64,
}
pub fn issue(
    tx: &Transaction<'_>,
    catalog: &Catalog,
    input: &Issue,
    generated: Issuance<'_>,
    actor: &str,
) -> Result<Invitation, Error> {
    members::writer(tx)?;
    members::role(&input.workspace_role, false)?;
    members::role(&input.company_role, true)?;
    members::company(catalog, &input.company_slug, false)?;
    let email = canonical(&input.email)?;
    let now = crate::mfa_login::date(generated.now)?;
    let expires = crate::mfa_login::date(
        generated
            .now
            .checked_add(7 * 86400000)
            .ok_or_else(|| invalid("clock"))?,
    )?;
    tx.execute("INSERT INTO main.rm_workspace_invitation_events(invitation_id,event_type,token_hash,token_key_version,canonical_email,email_hash,workspace_role,company_slug,company_role,expires_at,actor,created_at) VALUES(?1,'issued',?2,1,?3,?4,?5,?6,?7,?8,?9,?10)",params![generated.id,generated.token_hash,email,generated.email_hash,input.workspace_role,input.company_slug,input.company_role,expires,actor,now]).map_err(invalid)?;
    audit(tx, "workspace_invitation_issued", generated.id, actor)?;
    by_id(tx, generated.id)
}
pub fn status(
    tx: &Transaction<'_>,
    id: &str,
    status: &str,
    actor: &str,
) -> Result<Invitation, Error> {
    members::writer(tx)?;
    let i = by_id(tx, id)?;
    if i.status == "cancelled" && status == "cancelled" {
        return Ok(i);
    }
    if match status {
        "cancelled" => !["issued", "delivery_confirmed"].contains(&i.status.as_str()),
        "delivery_confirmed" | "delivery_failed" => i.status != "issued",
        _ => true,
    } {
        return Err(invalid("status"));
    }
    tx.execute("INSERT INTO main.rm_workspace_invitation_events(invitation_id,event_type,actor) VALUES(?1,?2,?3)",params![id,status,actor]).map_err(invalid)?;
    audit(tx, &format!("workspace_invitation_{status}"), id, actor)?;
    by_id(tx, id)
}
pub fn inspect(
    tx: &Transaction<'_>,
    hash: &str,
    now: i64,
    catalog: &Catalog,
) -> Result<Invitation, Error> {
    members::writer(tx)?;
    let id:String=tx.query_row("SELECT invitation_id FROM main.rm_workspace_invitation_events WHERE event_type='issued' AND token_hash=?1 AND token_key_version=1",[hash],|r|r.get(0)).map_err(invalid)?;
    let i = by_id(tx, &id)?;
    let expires = crate::cookie_session::timestamp(&i.expires_at)?;
    if i.status != "delivery_confirmed" || now >= expires {
        return Err(invalid("expired"));
    }
    members::company(catalog, &i.company_slug, false)?;
    Ok(i)
}
fn canonical_hash(value: &str) -> bool {
    let b = value.as_bytes();
    b.len() == 161
        && b[32] == b':'
        && b.iter()
            .enumerate()
            .all(|(n, b)| n == 32 || b.is_ascii_digit() || (b'a'..=b'f').contains(b))
}
pub struct Identity<'a> {
    pub user: &'a str,
    pub account: &'a str,
    pub name: &'a str,
    pub hash: &'a str,
}
/// Only trusted runtime-generated IDs and native password hash, under current invitation writer.
/// Existing identities are retained, never reset by possession of an invitation.
pub fn identity(
    tx: &Transaction<'_>,
    i: &Invitation,
    input: Identity<'_>,
    now: i64,
) -> Result<String, Error> {
    members::writer(tx)?;
    let existing: Option<String> = tx
        .query_row(
            "SELECT id FROM main.user WHERE email=?1 COLLATE BINARY",
            [&i.email],
            |r| r.get(0),
        )
        .optional()
        .map_err(invalid)?;
    if let Some(id) = existing {
        let mut query=tx.prepare("SELECT password FROM main.account WHERE userId=?1 COLLATE BINARY AND accountId=?1 COLLATE BINARY AND issuer='local:credential' COLLATE BINARY AND providerId='credential' COLLATE BINARY LIMIT 2").map_err(invalid)?;
        let hashes = query
            .query_map([&id], |r| {
                Ok(zeroize::Zeroizing::new(r.get::<_, String>(0)?))
            })
            .map_err(invalid)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(invalid)?;
        if hashes.len() != 1 || !canonical_hash(&hashes[0]) {
            return Err(invalid("existing credential"));
        }
        return Ok(id);
    }
    if input.name.trim().is_empty()
        || input.name.len() > 4096
        || input.name.chars().any(char::is_control)
        || !canonical_hash(input.hash)
    {
        return Err(invalid("identity"));
    }
    let date = crate::mfa_login::date(now)?;
    tx.execute("INSERT INTO main.user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt) VALUES(?1,?2,?3,0,0,?4,?4)",params![input.user,input.name.trim(),i.email,date]).map_err(invalid)?;
    tx.execute("INSERT INTO main.account(id,userId,accountId,providerId,issuer,password,createdAt,updatedAt) VALUES(?1,?2,?2,'credential','local:credential',?3,?4,?4)",params![input.account,input.user,input.hash,date]).map_err(invalid)?;
    Ok(input.user.into())
}
pub fn accept(tx: &Transaction<'_>, i: &Invitation, user: &str) -> Result<bool, Error> {
    members::writer(tx)?;
    let (email, verified, mfa): (String, i64, Option<i64>) = tx
        .query_row(
            "SELECT email,emailVerified,twoFactorEnabled FROM main.user WHERE id=?1 COLLATE BINARY",
            [user],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(invalid)?;
    if email != i.email || ![0, 1].contains(&verified) || !matches!(mfa, None | Some(0 | 1)) {
        return Err(invalid("identity"));
    }
    let actor = format!("user:{user}");
    let access = members::access(tx, user)?;
    if access.is_none()
        || (access.as_deref() == Some("member") && i.workspace_role == "workspace_owner")
    {
        tx.execute("INSERT INTO main.rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES(?1,'activate',?2,?3)",params![user,i.workspace_role,actor]).map_err(invalid)?;
    }
    let old = members::membership(tx, user, &i.company_slug)?;
    if let Some(role) = old {
        if role != i.company_role {
            return Err(invalid("conflict"));
        }
    } else {
        tx.execute("INSERT INTO main.rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES(?1,?2,'grant',?3,?4)",params![user,i.company_slug,i.company_role,actor]).map_err(invalid)?;
    }
    tx.execute("INSERT INTO main.rm_workspace_invitation_events(invitation_id,event_type,user_id,actor) VALUES(?1,'accepted',?2,?3)",params![i.invitation_id,user,actor]).map_err(invalid)?;
    audit(
        tx,
        "workspace_invitation_accepted",
        &i.invitation_id,
        &actor,
    )?;
    Ok(verified == 1 && mfa == Some(1))
}
