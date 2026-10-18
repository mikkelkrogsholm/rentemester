//! First-user saga state/effects on an admitted current control writer only.
use rentemester_engine::Error;
use rusqlite::{MAIN_DB, OptionalExtension, Transaction, TransactionState, params};
use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Reserved,
    IdentityCreated,
    MailConfirmed,
    AccessCompleted,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reservation {
    pub reservation_hash: String,
    pub company_slug: String,
    pub phase: Phase,
    pub user_id: Option<String>,
}
fn invalid(_: impl std::fmt::Display) -> Error {
    Error::new("invalid_control", "bootstrap control facts refused")
}
pub fn denied() -> Error {
    Error::new(
        "bootstrap_refused",
        "first workspace identity bootstrap refused",
    )
}
fn text(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= 4096 && !s.chars().any(char::is_control)
}
fn hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn writer(tx: &Transaction<'_>) -> Result<(), Error> {
    if tx.transaction_state(Some(MAIN_DB)).map_err(invalid)? != TransactionState::Write {
        return Err(invalid("writer"));
    }
    crate::control_migration::validate_current_control(tx).map_err(invalid)
}
fn count(tx: &Transaction<'_>, sql: &str) -> Result<i64, Error> {
    tx.query_row(sql, [], |r| r.get(0)).map_err(invalid)
}
/// Read the entire singleton history, not a caller-selected slice that can hide a
/// competing reservation. Validate exact sequence and stable subject/company.
fn state(tx: &Transaction<'_>) -> Result<Option<Reservation>, Error> {
    let mut stmt=tx.prepare("SELECT reservation_hash,event_type,user_id,company_slug,actor FROM main.rm_workspace_bootstrap_events ORDER BY id LIMIT 5").map_err(invalid)?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, String>(4)?,
            ))
        })
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    if rows.is_empty() {
        return Ok(None);
    }
    if rows.len() > 4 {
        return Err(invalid("saga length"));
    }
    let (reservation, event, user, company, actor) = &rows[0];
    let Some(company) = company else {
        return Err(invalid("company"));
    };
    if !hash(reservation) || event != "reserved" || user.is_some() || !text(company) || !text(actor)
    {
        return Err(invalid("reservation"));
    }
    let mut result = Reservation {
        reservation_hash: reservation.clone(),
        company_slug: company.clone(),
        phase: Phase::Reserved,
        user_id: None,
    };
    for (index, (current, event, user, slug, actor)) in rows.iter().enumerate().skip(1) {
        let (expected, phase) = match index {
            1 => ("identity_created", Phase::IdentityCreated),
            2 => ("mail_confirmed", Phase::MailConfirmed),
            3 => ("access_completed", Phase::AccessCompleted),
            _ => return Err(invalid("phase")),
        };
        if current != reservation
            || event != expected
            || !text(actor)
            || user.as_deref().is_none_or(|s| !text(s))
            || (index < 3 && slug.is_some())
            || (index == 3 && slug.as_deref() != Some(company))
            || result
                .user_id
                .as_ref()
                .is_some_and(|id| Some(id) != user.as_ref())
        {
            return Err(invalid("saga binding"));
        }
        result.phase = phase;
        result.user_id = user.clone();
    }
    Ok(Some(result))
}
fn event(
    tx: &Transaction<'_>,
    reservation: &Reservation,
    kind: &str,
    actor: &str,
) -> Result<(), Error> {
    if !text(actor) {
        return Err(denied());
    }
    let user = if kind == "reserved" {
        None
    } else {
        reservation.user_id.as_deref()
    };
    let company = if matches!(kind, "reserved" | "access_completed") {
        Some(reservation.company_slug.as_str())
    } else {
        None
    };
    tx.execute("INSERT INTO main.rm_workspace_bootstrap_events(reservation_hash,event_type,user_id,company_slug,actor) VALUES(?1,?2,?3,?4,?5)",params![reservation.reservation_hash,kind,user,company,actor]).map_err(invalid)?;
    let audit = match kind {
        "reserved" => "workspace_identity_bootstrap_reserved",
        "identity_created" => "workspace_identity_bootstrap_identity_created",
        "mail_confirmed" => "workspace_identity_bootstrap_mail_confirmed",
        "access_completed" => "workspace_identity_bootstrap_access_completed",
        _ => return Err(invalid("event")),
    };
    tx.execute("INSERT INTO main.workspace_audit(event_type,entity_type,entity_id,actor) VALUES(?1,'workspace_bootstrap',?2,?3)",params![audit,reservation.reservation_hash,actor]).map_err(invalid)?;
    Ok(())
}
fn selected(
    tx: &Transaction<'_>,
    reservation_hash: &str,
    company: &str,
) -> Result<Reservation, Error> {
    let current = state(tx)?.ok_or_else(denied)?;
    if current.reservation_hash != reservation_hash || current.company_slug != company {
        return Err(denied());
    }
    Ok(current)
}
pub fn reserve(
    tx: &Transaction<'_>,
    reservation_hash: &str,
    company: &str,
    actor: &str,
) -> Result<Reservation, Error> {
    writer(tx)?;
    if !hash(reservation_hash) || !text(company) || !text(actor) {
        return Err(denied());
    }
    if let Some(current) = state(tx)? {
        if current.reservation_hash != reservation_hash || current.company_slug != company {
            return Err(denied());
        }
        return Ok(current);
    }
    if count(tx, "SELECT count(*) FROM main.user")? != 0 {
        return Err(denied());
    }
    let current = Reservation {
        reservation_hash: reservation_hash.to_owned(),
        company_slug: company.to_owned(),
        phase: Phase::Reserved,
        user_id: None,
    };
    event(tx, &current, "reserved", actor)?;
    Ok(current)
}
fn canonical_hash(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 161
        && bytes[32] == b':'
        && bytes
            .iter()
            .enumerate()
            .all(|(i, c)| i == 32 || c.is_ascii_digit() || (b'a'..=b'f').contains(c))
}
fn credential(tx: &Transaction<'_>, subject: &str, email: &str) -> Result<(), Error> {
    let mut query=tx.prepare("SELECT a.password FROM main.user u JOIN main.account a ON a.userId=u.id COLLATE BINARY WHERE u.id=?1 COLLATE BINARY AND u.email=?2 COLLATE BINARY AND u.emailVerified IN(0,1) AND (u.twoFactorEnabled IS NULL OR u.twoFactorEnabled IN(0,1)) AND a.issuer='local:credential' COLLATE BINARY AND a.providerId='credential' COLLATE BINARY AND a.accountId=u.id COLLATE BINARY LIMIT 2").map_err(invalid)?;
    let hashes = query
        .query_map(params![subject, email], |r| {
            Ok(zeroize::Zeroizing::new(r.get::<_, String>(0)?))
        })
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    if hashes.len() != 1 || !canonical_hash(&hashes[0]) {
        return Err(denied());
    }
    Ok(())
}
/// Runtime-generated ids and canonical password hash only. A resumed credential is
/// never overwritten, no session is minted and no address verification is inferred.
pub struct Identity<'a> {
    pub email: &'a str,
    pub name: &'a str,
    pub password_hash: &'a str,
    pub user_id: &'a str,
    pub account_id: &'a str,
    pub now: i64,
}
pub fn identity(
    tx: &Transaction<'_>,
    reservation_hash: &str,
    company: &str,
    actor: &str,
    input: Identity<'_>,
) -> Result<Reservation, Error> {
    let Identity {
        email,
        name,
        password_hash,
        user_id,
        account_id,
        now,
    } = input;
    writer(tx)?;
    let mut current = selected(tx, reservation_hash, company)?;
    if current.phase != Phase::Reserved {
        credential(tx, current.user_id.as_deref().ok_or_else(denied)?, email)?;
        return Ok(current);
    }
    if ![email, name, user_id, account_id, actor]
        .into_iter()
        .all(text)
        || !canonical_hash(password_hash)
    {
        return Err(denied());
    }
    let existing: Option<String> = tx
        .query_row(
            "SELECT id FROM main.user WHERE email=?1 COLLATE BINARY",
            [email],
            |r| r.get(0),
        )
        .optional()
        .map_err(invalid)?;
    let id = if let Some(id) = existing {
        credential(tx, &id, email)?;
        id
    } else {
        if count(tx, "SELECT count(*) FROM main.user")? != 0 {
            return Err(denied());
        }
        crate::mfa_login::check_now(now)?;
        let date = crate::mfa_login::date(now)?;
        tx.execute("INSERT INTO main.user(id,name,email,emailVerified,createdAt,updatedAt,twoFactorEnabled) VALUES(?1,?2,?3,0,?4,?4,0)",params![user_id,name,email,date]).map_err(invalid)?;
        tx.execute("INSERT INTO main.account(id,issuer,accountId,providerId,userId,password,createdAt,updatedAt) VALUES(?1,'local:credential',?2,'credential',?2,?3,?4,?4)",params![account_id,user_id,password_hash,date]).map_err(invalid)?;
        user_id.to_owned()
    };
    current.user_id = Some(id);
    event(tx, &current, "identity_created", actor)?;
    current.phase = Phase::IdentityCreated;
    Ok(current)
}
/// Trusted application calls only after confirmed handoff or observing the already
/// verified reserved address. This function is not an HTTP delivery assertion.
pub fn mail_confirmed(
    tx: &Transaction<'_>,
    reservation_hash: &str,
    company: &str,
    email: &str,
    actor: &str,
) -> Result<Reservation, Error> {
    writer(tx)?;
    let mut current = selected(tx, reservation_hash, company)?;
    credential(tx, current.user_id.as_deref().ok_or_else(denied)?, email)?;
    if current.phase == Phase::IdentityCreated {
        event(tx, &current, "mail_confirmed", actor)?;
        current.phase = Phase::MailConfirmed;
    }
    if !matches!(current.phase, Phase::MailConfirmed | Phase::AccessCompleted) {
        return Err(denied());
    }
    Ok(current)
}
pub fn complete(
    tx: &Transaction<'_>,
    reservation_hash: &str,
    company: &str,
    email: &str,
    actor: &str,
) -> Result<Reservation, Error> {
    writer(tx)?;
    let mut current = selected(tx, reservation_hash, company)?;
    if current.phase == Phase::AccessCompleted {
        return Ok(current);
    }
    if current.phase != Phase::MailConfirmed {
        return Err(denied());
    }
    let user = current.user_id.as_deref().ok_or_else(denied)?;
    credential(tx, user, email)?;
    let access:Option<(String,Option<String>)>=tx.query_row("SELECT event_type,workspace_role FROM main.rm_workspace_user_access_events WHERE user_id=?1 COLLATE BINARY ORDER BY id DESC LIMIT 1",[user],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(invalid)?;
    let membership:Option<(String,Option<String>)>=tx.query_row("SELECT event_type,company_role FROM main.rm_company_membership_events WHERE user_id=?1 COLLATE BINARY AND company_slug=?2 COLLATE BINARY ORDER BY id DESC LIMIT 1",[user,company],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(invalid)?;
    if access.as_ref().is_some_and(|(event, role)| {
        event != "activate" || role.as_deref() != Some("workspace_owner")
    }) || membership
        .as_ref()
        .is_some_and(|(event, role)| event != "grant" || role.as_deref() != Some("owner"))
    {
        return Err(denied());
    }
    if access.is_none() {
        tx.execute("INSERT INTO main.rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES(?1,'activate','workspace_owner',?2)",params![user,actor]).map_err(invalid)?;
    }
    if membership.is_none() {
        tx.execute("INSERT INTO main.rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES(?1,?2,'grant','owner',?3)",params![user,company,actor]).map_err(invalid)?;
    }
    event(tx, &current, "access_completed", actor)?;
    current.phase = Phase::AccessCompleted;
    Ok(current)
}
