//! Immutable two-company evidence lifecycle. Caller retains current authority and Owner admission.
mod history;
mod journal;
#[cfg(test)]
mod tests;
pub use history::{HistoricalLink, validate_history};
pub use rentemester_engine::intercompany_dispositions::{Disposition, Side};
use rentemester_engine::intercompany_dispositions::{refs, refused, text};
use rentemester_engine::{Error, fiscal_year::Date, money::Money};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
fn sql(_: impl std::fmt::Display) -> Error {
    refused("disposition storage unavailable or inconsistent")
}
pub fn canonical(v: &Value) -> String {
    match v {
        Value::Object(o) => {
            let mut keys = o.keys().collect::<Vec<_>>();
            keys.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
            format!(
                "{{{}}}",
                keys.into_iter()
                    .map(|k| format!("{}:{}", json!(k), canonical(&o[k])))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
        Value::Array(a) => format!(
            "[{}]",
            a.iter().map(canonical).collect::<Vec<_>>().join(",")
        ),
        Value::Number(n) => ryu_js::Buffer::new().format(n.as_f64().unwrap()).into(),
        _ => v.to_string(),
    }
}
pub fn hash(v: &Value) -> String {
    crate::intercompany::sha(&canonical(v))
}
pub fn hex(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
#[derive(Clone, Debug)]
pub struct Principal {
    pub kind: String,
    pub id: String,
}
#[derive(Clone, Debug)]
pub struct JournalLine {
    pub debit: Money,
    pub credit: Money,
}
#[derive(Clone, Debug)]
pub struct JournalEntry {
    pub id: i64,
    pub entry_no: String,
    pub transaction_date: String,
    pub entry_hash: String,
    pub status: String,
    pub reversal_of_entry_id: Option<i64>,
    pub reversed: bool,
    pub lines: Vec<JournalLine>,
}
#[derive(Clone, Debug)]
pub struct JournalSnapshot {
    pub company_slug: String,
    pub ledger_uuid: String,
    pub currency: String,
    pub ledger_head_hash: Option<String>,
    pub entries: Vec<JournalEntry>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Plan {
    pub disposition: Disposition,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Propose {
    pub disposition: Disposition,
    pub confirm: bool,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Exact {
    pub disposition_id: String,
    pub payload_hash: String,
    pub confirm: bool,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Link {
    pub disposition_id: String,
    pub payload_hash: String,
    pub confirm: bool,
    pub side: String,
    pub journal_entry_id: i64,
    pub ledger_head_hash: Option<String>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Settle {
    pub disposition_id: String,
    pub payload_hash: String,
    pub confirm: bool,
    pub settlement_evidence_record_ids: Vec<String>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Supersede {
    pub disposition_id: String,
    pub payload_hash: String,
    pub confirm: bool,
    pub replacement_disposition_id: String,
    pub reason: String,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Reopen {
    pub disposition_id: String,
    pub payload_hash: String,
    pub confirm: bool,
    pub reason: String,
}
#[derive(Clone, Debug)]
pub enum Action {
    Plan(Plan),
    Propose(Propose),
    Approve(Exact),
    Link(Link),
    Settle(Settle),
    Supersede(Supersede),
    Reopen(Reopen),
    Status {
        disposition_id: String,
        as_of: String,
    },
}
#[derive(Clone, Debug)]
pub struct Requirements {
    pub company_slugs: Vec<String>,
    pub party_ids: Vec<String>,
    pub evidence_record_ids: Vec<String>,
    pub dispositions: Vec<Disposition>,
    pub linked_journal_ids: BTreeMap<String, Vec<i64>>,
}
impl Action {
    pub fn mutates(&self) -> bool {
        !matches!(self, Self::Plan(_) | Self::Status { .. })
    }
}
fn normalized(db: &Connection, mut d: Disposition) -> Result<Disposition, Error> {
    if d.disposition_id.is_empty() {
        let id: String = db
            .query_row("SELECT lower(hex(randomblob(16)))", [], |r| r.get(0))
            .map_err(sql)?;
        d.disposition_id = format!(
            "icd-{}-{}-4{}-a{}-{}",
            &id[..8],
            &id[8..12],
            &id[13..16],
            &id[17..20],
            &id[20..]
        );
    }
    d.normalize()
}
pub fn prepare(tx: &Transaction<'_>, action: &mut Action) -> Result<Requirements, Error> {
    let mut states = history::states(tx)?;
    let mut dispositions = vec![];
    let mut already_settled = false;
    match action {
        Action::Plan(i) => {
            i.disposition = normalized(tx, i.disposition.clone())?;
            dispositions.push(i.disposition.clone())
        }
        Action::Propose(i) => {
            i.disposition = normalized(tx, i.disposition.clone())?;
            dispositions.push(i.disposition.clone())
        }
        _ => {
            let id = match action {
                Action::Approve(i) => &mut i.disposition_id,
                Action::Link(i) => &mut i.disposition_id,
                Action::Settle(i) => &mut i.disposition_id,
                Action::Supersede(i) => &mut i.disposition_id,
                Action::Reopen(i) => &mut i.disposition_id,
                Action::Status { disposition_id, .. } => disposition_id,
                _ => unreachable!(),
            };
            *id = text(id, 80)?;
            let s = states
                .remove(id)
                .ok_or_else(|| Error::new("disposition_not_found", "disposition not found"))?;
            already_settled = s.status == "settled";
            dispositions.push(s.disposition);
        }
    }
    let mut records = BTreeSet::new();
    if let Action::Supersede(i) = action {
        i.replacement_disposition_id = text(&i.replacement_disposition_id, 80)?;
        dispositions.push(
            states
                .remove(&i.replacement_disposition_id)
                .ok_or_else(|| refused("replacement disposition not found"))?
                .disposition,
        );
    }
    if let Action::Settle(i) = action
        && !already_settled
    {
        if i.settlement_evidence_record_ids.is_empty()
            || i.settlement_evidence_record_ids.len() > 64
        {
            return Err(refused("1 through 64 settlement references required"));
        }
        i.settlement_evidence_record_ids = i
            .settlement_evidence_record_ids
            .iter()
            .map(|v| text(v, 160))
            .collect::<Result<_, _>>()?;
        records.extend(i.settlement_evidence_record_ids.clone());
    }
    let mut companies = BTreeSet::new();
    let mut parties = BTreeSet::new();
    let mut linked: BTreeMap<String, Vec<i64>> = BTreeMap::new();
    for d in &dispositions {
        companies.extend([d.left.company_slug.clone(), d.right.company_slug.clone()]);
        parties.extend(d.party_ids.clone());
        records.extend(d.evidence_record_ids.clone());
        let mut history=tx.prepare("SELECT canonical_payload FROM rm_intercompany_disposition_lifecycle_events WHERE disposition_id=?1 AND event_type='settled' ORDER BY id").map_err(sql)?;
        for payload in history
            .query_map([&d.disposition_id], |r| r.get::<_, String>(0))
            .map_err(sql)?
        {
            let payload: Value = serde_json::from_str(&payload.map_err(sql)?).map_err(sql)?;
            let ids: Vec<String> =
                serde_json::from_value(payload["settlementEvidenceRecordIds"].clone())
                    .map_err(sql)?;
            records.extend(ids);
        }
        let mut q=tx.prepare("SELECT company_slug,journal_entry_id FROM rm_intercompany_disposition_journal_links WHERE disposition_id=?1 ORDER BY id").map_err(sql)?;
        for row in q
            .query_map([&d.disposition_id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
            })
            .map_err(sql)?
        {
            let (c, id) = row.map_err(sql)?;
            linked.entry(c).or_default().push(id);
        }
    }
    for ids in linked.values_mut() {
        ids.sort();
        ids.dedup();
    }
    Ok(Requirements {
        company_slugs: companies.into_iter().collect(),
        party_ids: parties.into_iter().collect(),
        evidence_record_ids: records.into_iter().collect(),
        dispositions,
        linked_journal_ids: linked,
    })
}
fn references(tx: &Connection, d: &Disposition) -> Result<(), Error> {
    for id in &d.party_ids {
        if !tx.query_row("SELECT EXISTS(SELECT 1 FROM rm_party_events WHERE party_id=?1 AND event_type='created')",[id],|r|r.get::<_,bool>(0)).map_err(sql)?{return Err(refused("disposition party missing"));}
    }
    records(tx, &d.evidence_record_ids)
}
fn records(tx: &Connection, ids: &[String]) -> Result<(), Error> {
    for id in ids {
        if !tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM rm_corporate_record_bytes WHERE record_id=?1)",
                [id],
                |r| r.get::<_, bool>(0),
            )
            .map_err(sql)?
        {
            return Err(refused("disposition corporate evidence missing"));
        }
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
fn append(
    tx: &Transaction<'_>,
    table: &str,
    id: &str,
    kind: &str,
    payload: &Value,
    actor: &str,
    principal: &Principal,
    now: &str,
) -> Result<(), Error> {
    let previous: Option<String> = tx
        .query_row(
            &format!("SELECT event_hash FROM {table} ORDER BY id DESC LIMIT 1"),
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(sql)?;
    let body = canonical(payload);
    let digest = crate::intercompany::sha(&body);
    let event_hash = hash(
        &json!({"previousHash":previous,"dispositionId":id,"type":kind,"payloadHash":digest,"actor":actor,"principal":{"kind":principal.kind,"id":principal.id},"at":now}),
    );
    tx.execute(&format!("INSERT INTO {table}(disposition_id,event_type,payload_hash,canonical_payload,actor,principal_kind,principal_id,previous_hash,event_hash,created_at)VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)"),params![id,kind,digest,body,actor,principal.kind,principal.id,previous,event_hash,now]).map_err(sql)?;
    tx.execute("INSERT INTO workspace_audit(event_type,entity_type,entity_id,actor,created_at)VALUES(?1,'intercompany_disposition',?2,?3,?4)",params![format!("intercompany_disposition_{kind}"),id,actor,now]).map_err(sql)?;
    Ok(())
}
const EVENTS: &str = "rm_intercompany_disposition_events";
const LIFECYCLE: &str = "rm_intercompany_disposition_lifecycle_events";
fn exact<'a>(
    states: &'a BTreeMap<String, history::State>,
    id: &str,
    h: &str,
    confirm: bool,
) -> Result<&'a history::State, Error> {
    if !confirm || !hex(h) {
        return Err(refused("confirmation and exact disposition hash required"));
    }
    let s = states
        .get(id)
        .ok_or_else(|| refused("exact disposition not found"))?;
    if s.payload_hash != h {
        return Err(refused("exact disposition hash differs"));
    }
    Ok(s)
}
pub fn execute(
    tx: &Transaction<'_>,
    action: &Action,
    snapshots: &[JournalSnapshot],
    actor: &str,
    principal: &Principal,
    now: &str,
) -> Result<Value, Error> {
    let actor = text(actor, 160)?;
    text(&principal.id, 160)?;
    if !["user", "service"].contains(&principal.kind.as_str()) {
        return Err(refused("unsupported stable principal"));
    }
    crate::cookie_session::timestamp(now)?;
    let states = history::states(tx)?;
    if let Action::Plan(i) = action {
        let d = i.disposition.clone().normalize()?;
        references(tx, &d)?;
        let value = serde_json::to_value(&d).map_err(sql)?;
        return Ok(
            json!({"ok":true,"mode":"dry-run","disposition":d,"payloadHash":hash(&value),"ledgerEffects":[],"proposedSides":["left","right"],"warnings":["Each legal company must post separately through its normal policy, document, VAT and period controls."],"journalCandidates":journal::candidates(&d,snapshots)?}),
        );
    }
    if let Action::Status {
        disposition_id,
        as_of,
    } = action
    {
        Date::parse(as_of)?;
        let s = states
            .get(disposition_id)
            .ok_or_else(|| Error::new("disposition_not_found", "disposition not found"))?;
        return journal::status(s, snapshots, as_of);
    }
    crate::workspace_members::writer(tx)?;
    let id = match action {
        Action::Propose(i) => {
            if !i.confirm {
                return Err(refused("confirmation required"));
            }
            let d = i.disposition.clone().normalize()?;
            references(tx, &d)?;
            let body = serde_json::to_value(&d).map_err(sql)?;
            let digest = hash(&body);
            if let Some(s) = states.get(&d.disposition_id) {
                if s.payload_hash == digest && s.status == "proposed" {
                    return Ok(s.value());
                }
                return Err(refused(
                    "disposition ID already has different or completed history",
                ));
            }
            tx.execute("INSERT INTO rm_intercompany_dispositions(disposition_id,canonical_payload,payload_hash,created_at)VALUES(?1,?2,?3,?4)",params![d.disposition_id,canonical(&body),digest,now]).map_err(sql)?;
            append(
                tx,
                EVENTS,
                &d.disposition_id,
                "proposed",
                &body,
                &actor,
                principal,
                now,
            )?;
            d.disposition_id
        }
        Action::Approve(i) => {
            let s = exact(&states, &i.disposition_id, &i.payload_hash, i.confirm)?;
            if ["approved", "partly_posted", "posted"].contains(&s.status.as_str()) {
                return Ok(s.value());
            }
            if s.status != "proposed" {
                return Err(refused("only a proposed disposition can be approved"));
            }
            let first = &s.events[0];
            if first["principal_kind"] == principal.kind && first["principal_id"] == principal.id {
                return Err(refused("distinct stable principal required for approval"));
            }
            append(
                tx,
                EVENTS,
                &i.disposition_id,
                "approved",
                &json!({"payloadHash":i.payload_hash}),
                &actor,
                principal,
                now,
            )?;
            i.disposition_id.clone()
        }
        Action::Link(i) => {
            let s = exact(&states, &i.disposition_id, &i.payload_hash, i.confirm)?;
            if !["approved", "partly_posted"].contains(&s.status.as_str()) {
                return Err(refused(
                    "only approved or partly posted dispositions can link",
                ));
            }
            if !["left", "right"].contains(&i.side.as_str())
                || i.journal_entry_id <= 0
                || i.journal_entry_id > 9_007_199_254_740_991
                || i.ledger_head_hash.as_ref().is_some_and(|h| !hex(h))
            {
                return Err(refused("exact side, positive journal ID and head required"));
            }
            journal::current_links(s, snapshots)?;
            if let Some(link) = s.links.iter().find(|l| l["side"] == i.side) {
                if link["journal_entry_id"] == i.journal_entry_id {
                    return Ok(s.value());
                }
                return Err(refused("side already has immutable journal link"));
            }
            let side = if i.side == "left" {
                &s.disposition.left
            } else {
                &s.disposition.right
            };
            let snap = journal::snapshot(snapshots, &side.company_slug)?;
            let entry = snap
                .entries
                .iter()
                .find(|e| e.id == i.journal_entry_id)
                .ok_or_else(|| refused("journal not found in admitted snapshot"))?;
            journal::matching(&s.disposition, side, snap, entry, true)?;
            if snap.ledger_head_hash != i.ledger_head_hash {
                return Err(refused("ledger head changed; refresh before linking"));
            }
            tx.execute("INSERT INTO rm_intercompany_disposition_journal_links(disposition_id,side,company_slug,journal_entry_id,journal_entry_no,journal_entry_hash,ledger_head_hash,linked_at,actor,principal_kind,principal_id)VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",params![i.disposition_id,i.side,side.company_slug,entry.id,entry.entry_no,entry.entry_hash,snap.ledger_head_hash,now,actor,principal.kind,principal.id]).map_err(sql)?;
            append(
                tx,
                LIFECYCLE,
                &i.disposition_id,
                "exception",
                &json!({"kind":"one_sided_posting","side":i.side,"payloadHash":i.payload_hash}),
                &actor,
                principal,
                now,
            )?;
            if s.links.len() == 1 {
                append(
                    tx,
                    LIFECYCLE,
                    &i.disposition_id,
                    "exception_resolved",
                    &json!({"kind":"one_sided_posting","payloadHash":i.payload_hash}),
                    &actor,
                    principal,
                    now,
                )?;
            }
            i.disposition_id.clone()
        }
        Action::Settle(i) => {
            let s = exact(&states, &i.disposition_id, &i.payload_hash, i.confirm)?;
            if s.status == "settled" {
                return Ok(s.value());
            }
            if s.status != "posted" {
                return Err(refused(
                    "fully posted disposition required before settlement",
                ));
            }
            let mut ids = i
                .settlement_evidence_record_ids
                .iter()
                .map(|v| text(v, 160))
                .collect::<Result<Vec<_>, _>>()?;
            if ids.is_empty() || ids.len() > 64 {
                return Err(refused("1 through 64 settlement references required"));
            }
            ids.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
            ids.dedup();
            records(tx, &ids)?;
            journal::current_links(s, snapshots)?;
            append(
                tx,
                LIFECYCLE,
                &i.disposition_id,
                "settled",
                &json!({"payloadHash":i.payload_hash,"settlementEvidenceRecordIds":ids}),
                &actor,
                principal,
                now,
            )?;
            i.disposition_id.clone()
        }
        Action::Supersede(i) => {
            let s = exact(&states, &i.disposition_id, &i.payload_hash, i.confirm)?;
            if ["settled", "superseded"].contains(&s.status.as_str())
                || i.replacement_disposition_id == i.disposition_id
                || !states.contains_key(&i.replacement_disposition_id)
            {
                return Err(refused(
                    "active disposition and distinct existing replacement required",
                ));
            }
            let reason = text(&i.reason, 1000)?;
            append(
                tx,
                LIFECYCLE,
                &i.disposition_id,
                "superseded",
                &json!({"payloadHash":i.payload_hash,"replacementDispositionId":i.replacement_disposition_id,"reason":reason}),
                &actor,
                principal,
                now,
            )?;
            i.disposition_id.clone()
        }
        Action::Reopen(i) => {
            let s = exact(&states, &i.disposition_id, &i.payload_hash, i.confirm)?;
            if !["settled", "superseded"].contains(&s.status.as_str()) {
                return Err(refused("only terminal dispositions can be reopened"));
            }
            journal::current_links(s, snapshots)?;
            append(
                tx,
                LIFECYCLE,
                &i.disposition_id,
                "reopened",
                &json!({"payloadHash":i.payload_hash,"reason":text(&i.reason,1000)?}),
                &actor,
                principal,
                now,
            )?;
            i.disposition_id.clone()
        }
        _ => unreachable!(),
    };
    Ok(history::states(tx)?
        .remove(&id)
        .ok_or_else(|| refused("written disposition missing"))?
        .value())
}
