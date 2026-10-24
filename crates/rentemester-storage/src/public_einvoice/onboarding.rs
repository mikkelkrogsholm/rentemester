//! Additive direction attestations; legacy outbound proof bytes remain unchanged.
use super::*;
const KIND: &str = "native_public_einvoice_direction_v1";
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Proof {
    version: u8,
    identity: Identity,
    ledger_uuid: String,
    environment: String,
    company_key: String,
    network: String,
    direction: String,
    participant_id: String,
    registered_at: String,
    actor: String,
    at: String,
}
fn valid_route(network: &str, direction: &str) -> bool {
    matches!(network, "nemhandel" | "peppol") && matches!(direction, "outbound" | "inbound")
}
fn proofs(db: &Connection) -> Result<Vec<Proof>, Error> {
    let mut q = db.prepare("SELECT event_type,entity_id,message,actor,created_at FROM audit_log WHERE entity_type=? ORDER BY id").map_err(sql)?;
    q.query_map([KIND], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
        ))
    })
    .map_err(sql)?
    .map(|row| {
        let (kind, id, raw, actor, at) = row.map_err(sql)?;
        let p: Proof = serde_json::from_str(&raw).map_err(|_| refused())?;
        ensure(
            p.version == 1
                && kind == KIND
                && id == "registration"
                && p.actor == actor
                && p.at == at
                && canonical(&p)? == raw
                && valid_route(&p.network, &p.direction)
                && text(&actor, 512)
                && text(&at, 64)
                && text(&p.registered_at, 64),
        )?;
        environment(&p.environment)?;
        Ok(p)
    })
    .collect()
}
pub(super) fn validate(db: &Connection) -> Result<(), Error> {
    let uuid: String = db
        .query_row(
            "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
            [],
            |r| r.get(0),
        )
        .map_err(sql)?;
    for p in proofs(db)? {
        ensure(
            p.ledger_uuid == uuid
                && (p.participant_id == p.identity.cvr
                    || p.participant_id == format!("DK{}", p.identity.cvr)),
        )?;
        // Existing typed company proof is mandatory; direction evidence cannot bind a key.
        ensure(registration::bound_identity(
            db,
            &p.environment,
            &p.company_key,
            &p.identity,
        )?)?;
        let row:(String,String,Option<String>,i64,i64,String)=db.query_row("SELECT participant_type,participant_id,webhook_url,registered_on_network,webhook_registered,registered_at FROM digisense_participants WHERE company_key=? AND network=? AND direction=?",params![p.company_key,p.network,p.direction],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).map_err(sql)?;
        ensure(
            row == (
                "DK:CVR".into(),
                p.participant_id,
                None,
                1,
                0,
                p.registered_at,
            ),
        )?;
    }
    Ok(())
}
pub fn direction_registered(
    db: &Connection,
    env: &str,
    key: &str,
    network: &str,
    direction: &str,
) -> Result<bool, Error> {
    ensure(valid_route(network, direction))?;
    ensure(load_bound(db, env)?.as_deref() == Some(key))?;
    validate(db)?;
    let identity = identity(db)?;
    if network == "nemhandel" && direction == "outbound" && outbound_registered(db, env, key)? {
        return Ok(true);
    }
    Ok(proofs(db)?.iter().any(|p| {
        p.environment == env
            && p.company_key == key
            && p.network == network
            && p.direction == direction
            && p.identity == identity
    }))
}
#[allow(clippy::too_many_arguments)]
pub fn save_direction(
    tx: &Transaction<'_>,
    i: &Identity,
    env: &str,
    key: &str,
    network: &str,
    direction: &str,
    actor: &str,
    now: &str,
) -> Result<(), Error> {
    ensure(
        identity(tx)? == *i && valid_route(network, direction) && text(actor, 512) && text(now, 64),
    )?;
    if network == "nemhandel" && direction == "outbound" {
        return save_outbound(tx, i, env, key, true, false, actor, now);
    }
    if direction_registered(tx, env, key, network, direction)? {
        return Ok(());
    }
    let old:Option<(String,String,Option<String>,i64,i64,String)>=tx.query_row("SELECT participant_type,participant_id,webhook_url,registered_on_network,webhook_registered,registered_at FROM digisense_participants WHERE company_key=? AND network=? AND direction=?",params![key,network,direction],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).optional().map_err(sql)?;
    let (participant_id, registered_at) = if let Some((
        kind,
        id,
        webhook,
        registered,
        webhook_registered,
        at,
    )) = old
    {
        ensure(
            kind == "DK:CVR"
                && (id == i.cvr || id == format!("DK{}", i.cvr))
                && webhook.is_none()
                && registered == 1
                && webhook_registered == 0,
        )?;
        (id, at)
    } else {
        tx.execute("INSERT INTO digisense_participants(company_key,network,direction,participant_type,participant_id,webhook_url,registered_on_network,webhook_registered,registered_at)VALUES(?,?,?,'DK:CVR',?,NULL,1,0,?)",params![key,network,direction,i.cvr,now]).map_err(sql)?;
        (i.cvr.clone(), now.into())
    };
    let ledger_uuid = tx
        .query_row(
            "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
            [],
            |r| r.get(0),
        )
        .map_err(sql)?;
    let p = Proof {
        version: 1,
        identity: i.clone(),
        ledger_uuid,
        environment: env.into(),
        company_key: key.into(),
        network: network.into(),
        direction: direction.into(),
        participant_id,
        registered_at,
        actor: actor.into(),
        at: now.into(),
    };
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor,created_at)VALUES(?,?,'registration',?,?,?)",params![KIND,KIND,canonical(&p)?,actor,now]).map_err(sql)?;
    validate(tx)
}
pub fn onboarding_status(db: &Connection, env: &str, key: &str) -> Result<Value, Error> {
    let mut networks = json!({});
    let mut ready = false;
    let mut inbound = false;
    for network in ["nemhandel", "peppol"] {
        let out = direction_registered(db, env, key, network, "outbound")?;
        let inc = direction_registered(db, env, key, network, "inbound")?;
        ready |= out && inc;
        inbound |= inc;
        networks[network] = json!({"outbound":out,"inbound":inc,"technicalReady":out && inc});
    }
    Ok(
        json!({"inbound":inbound,"networks":networks,"technicalReady":ready,"externallyApproved":false}),
    )
}
