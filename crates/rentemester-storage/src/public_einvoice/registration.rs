use super::*;
const KIND: &str = "native_public_einvoice_registration_v1";
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Identity {
    pub cvr: String,
    pub company_name: String,
}
pub fn identity(db: &Connection) -> Result<Identity, Error> {
    let (cvr, name, country): (Option<String>, String, String) = db
        .query_row(
            "SELECT cvr,name,country FROM companies WHERE id=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(sql)?;
    let cvr = cvr.ok_or_else(refused)?;
    let digits = cvr.strip_prefix("DK").unwrap_or(&cvr);
    ensure(
        country == "DK"
            && digits.len() == 8
            && digits.bytes().all(|b| b.is_ascii_digit())
            && text(name.trim(), 256)
            && name.trim() != "Rentemester company",
    )?;
    Ok(Identity {
        cvr: digits.into(),
        company_name: name.trim().into(),
    })
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Registration {
    Missing,
    Unverified { company_key: String },
    Bound { company_key: String },
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LocalRow {
    participant_id: String,
    registered_at: String,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Proof {
    version: u8,
    kind: String,
    identity: Identity,
    ledger_uuid: String,
    environment: String,
    company_key: String,
    actor: String,
    at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    local_row: Option<LocalRow>,
}
fn proofs(db: &Connection) -> Result<Vec<Proof>, Error> {
    let mut q=db.prepare("SELECT event_type,message,actor,created_at FROM audit_log WHERE entity_type=? ORDER BY id").map_err(sql)?;
    q.query_map([KIND], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
        ))
    })
    .map_err(sql)?
    .map(|r| {
        let (kind, raw, actor, at) = r.map_err(sql)?;
        let p: Proof = serde_json::from_str(&raw).map_err(|_| refused())?;
        environment(&p.environment)?;
        ensure(
            ((p.version == 1 && p.local_row.is_none())
                || (p.version == 2
                    && p.local_row.as_ref().is_some_and(|r| {
                        text(&r.registered_at, 64) && text(&r.participant_id, 32)
                    })))
                && [
                    "native_public_einvoice_company",
                    "native_public_einvoice_outbound",
                ]
                .contains(&kind.as_str())
                && p.kind == kind
                && p.actor == actor
                && p.at == at
                && canonical(&p)? == raw
                && text(&p.actor, 512)
                && text(&p.at, 64)
                && text(&p.company_key, 512)
                && p.identity.cvr.len() == 8
                && p.identity.cvr.bytes().all(|b| b.is_ascii_digit())
                && text(&p.identity.company_name, 256),
        )?;
        Ok(p)
    })
    .collect()
}
fn uuid(db: &Connection) -> Result<String, Error> {
    db.query_row(
        "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
        [],
        |r| r.get(0),
    )
    .map_err(sql)
}
fn record(
    tx: &Transaction<'_>,
    i: &Identity,
    env: &str,
    key: &str,
    kind: &str,
    actor: &str,
    now: &str,
    local_row: Option<LocalRow>,
) -> Result<(), Error> {
    ensure(identity(tx)? == *i && text(key, 512) && text(actor, 512) && text(now, 64))?;
    environment(env)?;
    let p = Proof {
        version: if local_row.is_some() { 2 } else { 1 },
        kind: kind.into(),
        identity: i.clone(),
        ledger_uuid: uuid(tx)?,
        environment: env.into(),
        company_key: key.into(),
        actor: actor.into(),
        at: now.into(),
        local_row,
    };
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor,created_at)VALUES(?,?,'registration',?,?,?)",params![kind,KIND,canonical(&p)?,actor,now]).map_err(sql)?;
    Ok(())
}
/// A legacy row is only an untrusted hint until the current operator license
/// lists this exact legal entity and confirms its key.
pub fn registration(db: &Connection, env: &str) -> Result<Registration, Error> {
    environment(env)?;
    validate(db)?;
    let i = identity(db)?;
    let all = proofs(db)?;
    if let Some(p) = all
        .iter()
        .rev()
        .find(|p| p.kind == "native_public_einvoice_company")
    {
        ensure(p.identity == i && p.environment == env && p.ledger_uuid == uuid(db)?)?;
        return Ok(Registration::Bound {
            company_key: p.company_key.clone(),
        });
    }
    let mut q=db.prepare("SELECT company_key,company_type,participant_id,company_name FROM digisense_companies WHERE participant_id=? OR participant_id=?").map_err(sql)?;
    let rows = q
        .query_map(params![i.cvr, format!("DK{}", i.cvr)], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    ensure(rows.len() <= 1)?;
    match rows.first() {
        None => Ok(Registration::Missing),
        Some((key, kind, _, name)) => {
            ensure(kind == "DK:CVR" && name == &i.company_name && text(key, 512))?;
            Ok(Registration::Unverified {
                company_key: key.clone(),
            })
        }
    }
}
pub fn load_bound(db: &Connection, env: &str) -> Result<Option<String>, Error> {
    match registration(db, env)? {
        Registration::Missing => Ok(None),
        Registration::Bound { company_key } => Ok(Some(company_key)),
        Registration::Unverified { .. } => Err(refused()),
    }
}
pub fn save_key(
    tx: &Transaction<'_>,
    i: &Identity,
    env: &str,
    key: &str,
    actor: &str,
    now: &str,
) -> Result<(), Error> {
    ensure(identity(tx)? == *i)?;
    environment(env)?;
    let local_row = match registration(tx, env)? {
        Registration::Bound { company_key } => return ensure(company_key == key),
        Registration::Missing => {
            ensure(text(key, 512))?;
            tx.execute("INSERT INTO digisense_companies(company_key,company_type,participant_id,company_name,registered_at)VALUES(?,'DK:CVR',?,?,?)",params![key,i.cvr,i.company_name,now]).map_err(sql)?;
            None
        }
        Registration::Unverified { company_key } => {
            ensure(company_key == key)?;
            let row=tx.query_row("SELECT participant_id,registered_at FROM digisense_companies WHERE company_key=? AND company_type='DK:CVR' AND company_name=? AND (participant_id=? OR participant_id=?)",params![key,i.company_name,i.cvr,format!("DK{}",i.cvr)],|r|Ok(LocalRow{participant_id:r.get(0)?,registered_at:r.get(1)?})).map_err(sql)?;
            Some(row)
        }
    };
    record(
        tx,
        i,
        env,
        key,
        "native_public_einvoice_company",
        actor,
        now,
        local_row,
    )
}

pub fn outbound_registered(db: &Connection, env: &str, key: &str) -> Result<bool, Error> {
    ensure(load_bound(db, env)?.as_deref() == Some(key))?;
    Ok(proofs(db)?.iter().any(|p| {
        p.kind == "native_public_einvoice_outbound" && p.environment == env && p.company_key == key
    }))
}
#[allow(clippy::too_many_arguments)]
pub fn save_outbound(
    tx: &Transaction<'_>,
    i: &Identity,
    env: &str,
    key: &str,
    registered_on_network: bool,
    webhook_registered: bool,
    actor: &str,
    now: &str,
) -> Result<(), Error> {
    ensure(identity(tx)? == *i && registered_on_network && !webhook_registered)?;
    if outbound_registered(tx, env, key)? {
        return Ok(());
    }
    // This function is called only after current provider confirmation. Existing
    // legacy facts are attested as-is, never updated or accepted merely on 409.
    let old:Option<(String,String,Option<String>,i64,i64,String)>=tx.query_row("SELECT participant_type,participant_id,webhook_url,registered_on_network,webhook_registered,registered_at FROM digisense_participants WHERE company_key=? AND network='nemhandel' AND direction='outbound'",[key],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).optional().map_err(sql)?;
    let local_row = if let Some((kind, participant, webhook, registered, webhook_registered, at)) =
        old
    {
        ensure(
            kind == "DK:CVR"
                && (participant == i.cvr || participant == format!("DK{}", i.cvr))
                && webhook.is_none()
                && registered == 1
                && webhook_registered == 0,
        )?;
        Some(LocalRow {
            participant_id: participant,
            registered_at: at,
        })
    } else {
        tx.execute("INSERT INTO digisense_participants(company_key,network,direction,participant_type,participant_id,webhook_url,registered_on_network,webhook_registered,registered_at)VALUES(?,'nemhandel','outbound','DK:CVR',?,NULL,1,0,?)",params![key,i.cvr,now]).map_err(sql)?;
        None
    };
    record(
        tx,
        i,
        env,
        key,
        "native_public_einvoice_outbound",
        actor,
        now,
        local_row,
    )
}

pub(super) fn historical_binding(db: &Connection, s: &Source) -> Result<(), Error> {
    let p = proofs(db)?;
    let own = p.iter().any(|p| {
        p.kind == "native_public_einvoice_company"
            && p.ledger_uuid == s.ledger_uuid
            && p.environment == s.environment
            && p.company_key == s.company_key
    });
    let out = p.iter().any(|p| {
        p.kind == "native_public_einvoice_outbound"
            && p.ledger_uuid == s.ledger_uuid
            && p.environment == s.environment
            && p.company_key == s.company_key
    });
    ensure(own && out)
}
pub(super) fn validate(db: &Connection) -> Result<(), Error> {
    let all = proofs(db)?;
    let uuid = uuid(db)?;
    // Old rows remain untouched when no native proof claims them.
    for (index, p) in all.iter().enumerate() {
        ensure(p.ledger_uuid == uuid)?;
        let participant = p
            .local_row
            .as_ref()
            .map(|r| r.participant_id.as_str())
            .unwrap_or(&p.identity.cvr);
        let recorded = p
            .local_row
            .as_ref()
            .map(|r| r.registered_at.as_str())
            .unwrap_or(&p.at);
        ensure(participant == p.identity.cvr || participant == format!("DK{}", p.identity.cvr))?;
        if p.kind == "native_public_einvoice_company" {
            let row:(String,String,String,String)=db.query_row("SELECT company_key,company_type,company_name,registered_at FROM digisense_companies WHERE participant_id=?",[participant],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).map_err(sql)?;
            ensure(
                row == (
                    p.company_key.clone(),
                    "DK:CVR".into(),
                    p.identity.company_name.clone(),
                    recorded.to_owned(),
                ),
            )?;
        } else {
            ensure(all[..index].iter().any(|q| {
                q.kind == "native_public_einvoice_company"
                    && q.identity == p.identity
                    && q.company_key == p.company_key
                    && q.environment == p.environment
            }))?;
            let row:(String,String,Option<String>,i64,i64,String)=db.query_row("SELECT participant_type,participant_id,webhook_url,registered_on_network,webhook_registered,registered_at FROM digisense_participants WHERE company_key=? AND network='nemhandel' AND direction='outbound'",[&p.company_key],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).map_err(sql)?;
            ensure(
                row == (
                    "DK:CVR".into(),
                    participant.to_owned(),
                    None,
                    1,
                    0,
                    recorded.to_owned(),
                ),
            )?;
        }
    }
    Ok(())
}
