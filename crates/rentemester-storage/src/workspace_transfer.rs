//! Durable local transfer authority in the existing immutable control audit.
use rentemester_engine::Error;
use rusqlite::{Connection, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
pub fn refused() -> Error {
    Error::new(
        "workspace_transfer_refused",
        "workspace transfer state, authority or reviewed evidence refused",
    )
}
fn sql(_: impl std::fmt::Display) -> Error {
    refused()
}
pub fn digest(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}
pub fn hex(v: &str, n: usize) -> bool {
    v.len() == n
        && v.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub fn text(v: &str, min: usize, max: usize) -> Result<String, Error> {
    let t = rentemester_engine::company_knowledge::trim(v);
    if t.len() < min || t.len() > max || t.chars().any(char::is_control) {
        return Err(refused());
    }
    Ok(t.into())
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Company {
    pub slug: String,
    pub ledger_uuid: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Challenge {
    pub version: u32,
    pub transfer_id: String,
    pub source_instance_id: String,
    pub workspace_id: String,
    pub archive_sha256: String,
    pub destination_instance_id: String,
    pub destination: String,
    pub companies: Vec<Company>,
}
impl Challenge {
    pub fn validate(&self) -> Result<(), Error> {
        if self.version != 1
            || !hex(&self.transfer_id, 32)
            || !hex(&self.source_instance_id, 32)
            || !hex(&self.destination_instance_id, 32)
            || self.source_instance_id == self.destination_instance_id
            || !hex(&self.archive_sha256, 64)
            || text(&self.workspace_id, 1, 1024)? != self.workspace_id
            || text(&self.destination, 10, 300)? != self.destination
        {
            return Err(refused());
        }
        validate_companies(&self.companies)
    }
}
pub fn validate_companies(c: &[Company]) -> Result<(), Error> {
    if c.is_empty() || c.len() > 64 {
        return Err(refused());
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut uuids = std::collections::BTreeSet::new();
    for x in c {
        if (x.slug.is_empty()
            || x.slug.len() > 100
            || !x.slug.as_bytes()[0].is_ascii_alphanumeric()
            || !x
                .slug
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'))
            || !hex(&x.ledger_uuid, 32)
            || !seen.insert(&x.slug)
            || !uuids.insert(&x.ledger_uuid)
        {
            return Err(refused());
        }
    }
    Ok(())
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Freeze {
    pub expected_state_hash: String,
    pub destination: String,
    pub reason: String,
    pub confirm: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Cancel {
    pub expected_state_hash: String,
    pub reason: String,
    pub confirm: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Export {
    pub expected_state_hash: String,
    pub confirm: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Release {
    pub expected_state_hash: String,
    pub challenge: Challenge,
    pub reason: String,
    pub confirm: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Activate {
    pub expected_state_hash: String,
    pub release: String,
    pub reason: String,
    pub confirm: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Fact {
    Frozen {
        transfer_id: String,
        source_instance_id: String,
        workspace_id: String,
        destination: String,
        reason: String,
        companies: Vec<Company>,
    },
    Cancelled {
        reason: String,
    },
    Offered {
        archive_sha256: String,
    },
    Moved {
        challenge: Challenge,
        release: String,
        reason: String,
    },
    Staged {
        challenge: Challenge,
    },
    Activated {
        release: String,
        reason: String,
    },
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Event {
    version: u32,
    previous_state_hash: String,
    actor: String,
    at: String,
    fact: Fact,
}
#[derive(Clone)]
pub struct State {
    pub mode: String,
    pub state_hash: String,
    pub transfer_id: Option<String>,
    pub source_instance_id: Option<String>,
    pub destination: Option<String>,
    pub workspace_id: Option<String>,
    pub companies: Vec<Company>,
    pub challenge: Option<Challenge>,
    pub release: Option<String>,
    pub offered: Vec<String>,
    pub history: Vec<Value>,
}
impl State {
    fn initial() -> Self {
        Self {
            mode: "active".into(),
            state_hash: digest(b"rentemester-workspace-transfer-v1:initial"),
            transfer_id: None,
            source_instance_id: None,
            destination: None,
            workspace_id: None,
            companies: vec![],
            challenge: None,
            release: None,
            offered: vec![],
            history: vec![],
        }
    }
    pub fn redacted(&self) -> Value {
        let mut v = State::initial().value();
        v["stateHash"] = json!(self.state_hash);
        v["mode"] = json!(self.mode);
        v
    }
    pub fn value(&self) -> Value {
        json!({"mode":self.mode,"stateHash":self.state_hash,"transferId":self.transfer_id,"destination":self.destination,"challenge":self.challenge,"release":self.release,"offeredArchiveHashes":self.offered,"pendingEffects":[],"history":self.history})
    }
    pub fn reviewed(&self, hash: &str, confirm: bool) -> Result<(), Error> {
        if !confirm || self.state_hash != hash {
            Err(Error::new(
                "transfer_state_changed",
                "workspace transfer reviewed state changed",
            ))
        } else {
            Ok(())
        }
    }
    pub fn matches(&self, c: &Challenge) -> bool {
        self.transfer_id.as_ref() == Some(&c.transfer_id)
            && self.source_instance_id.as_ref() == Some(&c.source_instance_id)
            && self.workspace_id.as_ref() == Some(&c.workspace_id)
            && self.companies == c.companies
    }
}
fn apply(s: &mut State, f: &Fact) -> Result<(), Error> {
    match f {
        Fact::Frozen {
            transfer_id,
            source_instance_id,
            workspace_id,
            destination,
            reason,
            companies,
        } => {
            if s.mode != "active"
                || !hex(transfer_id, 32)
                || !hex(source_instance_id, 32)
                || text(workspace_id, 1, 1024)? != *workspace_id
                || text(destination, 10, 300)? != *destination
                || text(reason, 10, 2000)? != *reason
            {
                return Err(refused());
            }
            validate_companies(companies)?;
            s.mode = "frozen".into();
            s.transfer_id = Some(transfer_id.clone());
            s.source_instance_id = Some(source_instance_id.clone());
            s.workspace_id = Some(workspace_id.clone());
            s.destination = Some(destination.clone());
            s.companies = companies.clone();
            s.offered.clear();
            s.challenge = None;
            s.release = None;
        }
        Fact::Cancelled { reason } => {
            if s.mode != "frozen" || !s.offered.is_empty() || text(reason, 10, 2000)? != *reason {
                return Err(Error::new(
                    "transfer_cancel_refused",
                    "offered or moved transfers cannot be cancelled",
                ));
            }
            let hash = s.state_hash.clone();
            let history = s.history.clone();
            *s = State::initial();
            s.state_hash = hash;
            s.history = history;
        }
        Fact::Offered { archive_sha256 } => {
            if s.mode != "frozen" || !hex(archive_sha256, 64) {
                return Err(refused());
            }
            if !s.offered.contains(archive_sha256) {
                s.offered.push(archive_sha256.clone());
            }
        }
        Fact::Moved {
            challenge,
            release,
            reason,
        } => {
            challenge.validate()?;
            if s.mode != "frozen"
                || !s.matches(challenge)
                || !s.offered.contains(&challenge.archive_sha256)
                || release.len() > 16384
                || release.is_empty()
                || text(reason, 10, 2000)? != *reason
            {
                return Err(refused());
            }
            s.mode = "moved".into();
            s.challenge = Some(challenge.clone());
            s.release = Some(release.clone());
        }
        Fact::Staged { challenge } => {
            challenge.validate()?;
            if s.mode != "frozen" || !s.matches(challenge) {
                return Err(refused());
            }
            s.mode = "staged".into();
            s.destination = Some(challenge.destination.clone());
            s.challenge = Some(challenge.clone());
            s.offered.clear();
            s.release = None;
        }
        Fact::Activated { release, reason } => {
            if s.mode != "staged"
                || release.is_empty()
                || release.len() > 16384
                || text(reason, 10, 2000)? != *reason
            {
                return Err(refused());
            }
            s.mode = "active".into();
            s.release = Some(release.clone());
        }
    }
    Ok(())
}
pub fn current(db: &Connection) -> Result<State, Error> {
    let mut s = State::initial();
    let mut q=db.prepare("SELECT event_type,entity_type,entity_id,actor,created_at FROM workspace_audit WHERE event_type GLOB 'native_workspace_transfer_*' OR entity_type='native_workspace_transfer_v1' ORDER BY id").map_err(sql)?;
    let rows = q
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
            ))
        })
        .map_err(sql)?;
    for row in rows {
        let (kind, entity, raw, actor, at) = row.map_err(sql)?;
        let e: Event = serde_json::from_str(&raw).map_err(sql)?;
        let name = match &e.fact {
            Fact::Frozen { .. } => "frozen",
            Fact::Cancelled { .. } => "cancelled",
            Fact::Offered { .. } => "offered",
            Fact::Moved { .. } => "moved",
            Fact::Staged { .. } => "staged",
            Fact::Activated { .. } => "activated",
        };
        if e.version != 1
            || e.previous_state_hash != s.state_hash
            || entity != "native_workspace_transfer_v1"
            || kind != format!("native_workspace_transfer_{name}")
            || actor != e.actor
            || at != e.at
            || text(&actor, 1, 4096)? != actor
            || serde_json::to_string(&e).map_err(sql)? != raw
        {
            return Err(refused());
        }
        crate::cookie_session::timestamp(&at)?;
        apply(&mut s, &e.fact)?;
        s.state_hash = digest(raw.as_bytes());
        s.history.push(serde_json::from_str(&raw).map_err(sql)?);
    }
    Ok(s)
}
pub fn require_business_write(db: &Connection) -> Result<(), Error> {
    let s = current(db)?;
    if s.mode != "active" {
        return Err(Error::new(
            "workspace_transfer_fenced",
            "workspace is frozen, moved or staged; ordinary business writes and dispatch are disabled",
        ));
    }
    Ok(())
}
pub fn append(tx: &Transaction<'_>, fact: Fact, actor: &str, at: &str) -> Result<State, Error> {
    crate::workspace_members::writer(tx)?;
    let mut s = current(tx)?;
    apply(&mut s, &fact)?;
    let e = Event {
        version: 1,
        previous_state_hash: s.state_hash,
        actor: actor.into(),
        at: at.into(),
        fact,
    };
    let raw = serde_json::to_string(&e).map_err(sql)?;
    let name = serde_json::to_value(&e.fact).map_err(sql)?["kind"]
        .as_str()
        .ok_or_else(refused)?
        .to_owned();
    tx.execute("INSERT INTO workspace_audit(event_type,entity_type,entity_id,actor,created_at)VALUES(?1,'native_workspace_transfer_v1',?2,?3,?4)",params![format!("native_workspace_transfer_{name}"),raw,actor,at]).map_err(sql)?;
    current(tx)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn typed_control_chain_is_fail_closed_and_cancel_does_not_erase_history() {
        let mut c = Connection::open_in_memory().unwrap();
        crate::control_migration::initialize_empty(&mut c).unwrap();
        let initial = current(&c).unwrap();
        let tx = c
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let s = append(
            &tx,
            Fact::Frozen {
                transfer_id: "a".repeat(32),
                source_instance_id: "b".repeat(32),
                workspace_id: "w".into(),
                destination: "Synthetic destination".into(),
                reason: "Explicit synthetic transfer review".into(),
                companies: vec![Company {
                    slug: "synthetic".into(),
                    ledger_uuid: "c".repeat(32),
                }],
            },
            "user:synthetic",
            "2026-01-01T00:00:00.000Z",
        )
        .unwrap();
        assert!(s.reviewed(&initial.state_hash, true).is_err());
        assert!(require_business_write(&tx).is_err());
        append(
            &tx,
            Fact::Cancelled {
                reason: "Abort before any archive offer".into(),
            },
            "user:synthetic",
            "2026-01-01T00:00:01.000Z",
        )
        .unwrap();
        assert_eq!(current(&tx).unwrap().history.len(), 2);
        assert!(require_business_write(&tx).is_ok());
        tx.commit().unwrap();
        c.execute("INSERT INTO workspace_audit(event_type,entity_type,entity_id,actor,created_at)VALUES('native_workspace_transfer_activated','native_workspace_transfer_v1','{}','user:synthetic','2026-01-01T00:00:01.000Z')",[]).unwrap();
        assert!(current(&c).is_err());
        assert!(require_business_write(&c).is_err());
    }
}
