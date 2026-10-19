//! Append-only workspace access, on the admitted control writer only.
use crate::workspace_context::Catalog;
use rentemester_engine::Error;
use rusqlite::{OptionalExtension, Transaction, params};
use serde_json::{Value, json};
fn invalid(_: impl std::fmt::Display) -> Error {
    Error::new(
        "workspace_access_refused",
        "workspace access change refused",
    )
}
pub fn writer(tx: &Transaction<'_>) -> Result<(), Error> {
    if tx
        .transaction_state(Some(rusqlite::MAIN_DB))
        .map_err(invalid)?
        != rusqlite::TransactionState::Write
    {
        return Err(invalid("writer"));
    }
    crate::control_migration::validate_current_control(tx).map_err(invalid)
}
pub fn role(value: &str, company: bool) -> Result<(), Error> {
    if if company {
        ["owner", "bookkeeper", "reviewer", "reader"].contains(&value)
    } else {
        ["workspace_owner", "member"].contains(&value)
    } {
        Ok(())
    } else {
        Err(invalid("role"))
    }
}
pub fn access(tx: &Transaction<'_>, user: &str) -> Result<Option<String>, Error> {
    let r:Option<(String,Option<String>)>=tx.query_row("SELECT event_type,workspace_role FROM main.rm_workspace_user_access_events WHERE user_id=?1 COLLATE BINARY ORDER BY id DESC LIMIT 1",[user],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(invalid)?;
    match r {
        None => Ok(None),
        Some((e, None)) if e == "disable" => Ok(None),
        Some((e, Some(r))) if e == "activate" => {
            role(&r, false)?;
            Ok(Some(r))
        }
        _ => Err(invalid("access")),
    }
}
pub fn membership(
    tx: &Transaction<'_>,
    user: &str,
    company: &str,
) -> Result<Option<String>, Error> {
    crate::control_access::observe_company_role(tx, user, company)
}
pub fn company<'a>(
    catalog: &'a Catalog,
    slug: &str,
    archived: bool,
) -> Result<&'a crate::workspace_context::RegisteredCompany, Error> {
    catalog
        .companies
        .iter()
        .find(|c| c.slug == slug && (archived || !c.archived))
        .ok_or_else(|| invalid("company"))
}
fn ready(tx: &Transaction<'_>, user: &str) -> Result<bool, Error> {
    tx.query_row(
        "SELECT emailVerified=1 AND twoFactorEnabled=1 FROM main.user WHERE id=?1 COLLATE BINARY",
        [user],
        |r| r.get(0),
    )
    .map_err(invalid)
}
fn owner(tx: &Transaction<'_>, user: &str, slug: Option<&str>) -> Result<bool, Error> {
    Ok(ready(tx, user)?
        && access(tx, user)?.as_deref() == Some("workspace_owner")
        && match slug {
            Some(c) => membership(tx, user, c)?.as_deref() == Some("owner"),
            None => true,
        })
}
fn protect(tx: &Transaction<'_>, user: &str, slug: Option<&str>) -> Result<(), Error> {
    if !owner(tx, user, slug)? {
        return Ok(());
    }
    let mut q = tx
        .prepare("SELECT id FROM main.user LIMIT 10001")
        .map_err(invalid)?;
    let ids = q
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    if ids.len() > 10000 {
        return Err(invalid("bound"));
    }
    for id in ids {
        if id != user && owner(tx, &id, slug)? {
            return Ok(());
        }
    }
    Err(Error::new(
        "last_owner",
        "the final effective owner cannot be removed or demoted",
    ))
}
pub fn audit(tx: &Transaction<'_>, event: &str, id: &str, actor: &str) -> Result<(), Error> {
    tx.execute("INSERT INTO main.workspace_audit(event_type,entity_type,entity_id,actor) VALUES(?1,'workspace_access',?2,?3)",params![event,id,actor]).map_err(invalid)?;
    Ok(())
}
pub fn change(
    tx: &Transaction<'_>,
    catalog: &Catalog,
    user: &str,
    slug: Option<&str>,
    new_role: Option<&str>,
    actor: &str,
) -> Result<Value, Error> {
    writer(tx)?;
    let exists: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM main.user WHERE id=?1 COLLATE BINARY)",
            [user],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    if !exists {
        return Err(invalid("user"));
    }
    if let Some(c) = slug {
        company(catalog, c, new_role.is_none())?;
        if let Some(r) = new_role {
            role(r, true)?;
        }
        let old = membership(tx, user, c)?;
        if old.as_deref() != new_role {
            if old.as_deref() == Some("owner") {
                protect(tx, user, Some(c))?;
            }
            tx.execute("INSERT INTO main.rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES(?1,?2,?3,?4,?5)",params![user,c,if new_role.is_some(){"grant"}else{"revoke"},new_role,actor]).map_err(invalid)?;
            audit(tx, "company_membership_changed", user, actor)?;
        }
        Ok(json!({"membership":{"active":new_role.is_some(),"role":new_role}}))
    } else {
        if let Some(r) = new_role {
            role(r, false)?;
        }
        let old = access(tx, user)?;
        if old.as_deref() != new_role {
            if old.as_deref() == Some("workspace_owner") {
                protect(tx, user, None)?;
                for c in &catalog.companies {
                    protect(tx, user, Some(&c.slug))?;
                }
            }
            tx.execute("INSERT INTO main.rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES(?1,?2,?3,?4)",params![user,if new_role.is_some(){"activate"}else{"disable"},new_role,actor]).map_err(invalid)?;
            if new_role.is_none() {
                tx.execute("INSERT INTO main.rm_workspace_security_events(scope,user_id,event_type,actor) VALUES('user',?1,'session_invalidate',?2)",params![user,actor]).map_err(invalid)?;
                tx.execute(
                    "DELETE FROM main.session WHERE userId=?1 COLLATE BINARY",
                    [user],
                )
                .map_err(invalid)?;
            }
            audit(tx, "workspace_user_access_changed", user, actor)?;
        }
        Ok(json!({"access":{"active":new_role.is_some(),"workspaceRole":new_role}}))
    }
}
pub fn list(tx: &Transaction<'_>, catalog: &Catalog, admin: &str) -> Result<Value, Error> {
    writer(tx)?;
    let mut q=tx.prepare("SELECT id,name,email,emailVerified,twoFactorEnabled FROM main.user ORDER BY lower(email),id LIMIT 10001").map_err(invalid)?;
    let rows = q
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, Option<i64>>(4)?,
            ))
        })
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    if rows.len() > 10000 {
        return Err(invalid("bound"));
    }
    let mut members = vec![];
    for (id, name, email, verified, mfa) in rows {
        if ![0, 1].contains(&verified) || !matches!(mfa, None | Some(0 | 1)) {
            return Err(invalid("flags"));
        }
        let Some(role) = access(tx, &id)? else {
            continue;
        };
        let mut memberships = vec![];
        for c in &catalog.companies {
            if !c.archived
                && membership(tx, admin, &c.slug)?.as_deref() == Some("owner")
                && let Some(role) = membership(tx, &id, &c.slug)?
            {
                memberships.push(json!({"companySlug":c.slug,"companyName":c.name,"role":role,"archived":c.archived}));
            }
        }
        members.push(json!({"userId":id,"name":name,"email":email,"emailVerified":verified==1,"twoFactorEnabled":mfa==Some(1),"accessReady":verified==1&&mfa==Some(1),"workspaceRole":role,"memberships":memberships}));
    }
    Ok(json!({"members":members}))
}
