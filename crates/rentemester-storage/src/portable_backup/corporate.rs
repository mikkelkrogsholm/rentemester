//! Fixed corporate originals/history codec. Historical provenance never becomes authority.
use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::collections::BTreeSet;
const FORMAT: &str = "rentemester-native-corporate-records/1";
const MAX: usize = 32 * 1024 * 1024;
const TABLES: [&str; 4] = [
    "rm_corporate_record_bytes",
    "rm_corporate_record_events",
    "rm_corporate_record_links",
    "rm_corporate_record_scope_assertions",
];
const COLS: [&[&str]; 4] = [
    &[
        "record_id",
        "original_bytes",
        "sha256",
        "filename",
        "source",
        "received_at",
        "record_date",
        "retention_until",
        "uploader",
        "sensitivity",
    ],
    &[
        "id",
        "record_id",
        "event_type",
        "record_type",
        "payload_hash",
        "canonical_payload",
        "actor",
        "created_at",
    ],
    &["record_id", "link_type", "link_id"],
    &[
        "id",
        "record_id",
        "scope_kind",
        "scope_id",
        "actor",
        "created_at",
    ],
];
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", content = "value", deny_unknown_fields)]
enum CorporateCell {
    Null,
    Integer(i64),
    Text(String),
    BlobBase64(String),
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CorporateTable {
    columns: Vec<String>,
    rows: Vec<Vec<CorporateCell>>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Corporate {
    format: String,
    tables: BTreeMap<String, CorporateTable>,
}
fn shape(db: &Connection) -> Result<(), Error> {
    for (table, cols) in TABLES.iter().zip(COLS) {
        if columns(db, table)? != cols {
            return Err(refused("corporate schema differs from codec v1"));
        }
    }
    Ok(())
}
fn dump(db: &Connection) -> Result<Vec<u8>, Error> {
    shape(db)?;
    let mut tables = BTreeMap::new();
    let mut budget = 0usize;
    for (name, cols) in TABLES.iter().zip(COLS) {
        let order = match *name {
            n if n == TABLES[1] || n == TABLES[3] => "id",
            n if n == TABLES[2] => "record_id,link_type,link_id",
            _ => "record_id",
        };
        let mut q = db
            .prepare(&format!(
                "SELECT * FROM {name} ORDER BY {order} LIMIT 100001"
            ))
            .map_err(sql)?;
        let mut rs = q.query([]).map_err(sql)?;
        let mut rows = vec![];
        while let Some(r) = rs.next().map_err(sql)? {
            if rows.len() >= 100000 {
                return Err(refused("corporate row bound"));
            }
            let mut row = vec![];
            for (i, col) in cols.iter().enumerate() {
                let cell = match r.get_ref(i).map_err(sql)? {
                    ValueRef::Null => CorporateCell::Null,
                    ValueRef::Integer(n) => CorporateCell::Integer(n),
                    ValueRef::Text(s) => {
                        budget = budget
                            .checked_add(s.len())
                            .ok_or_else(|| refused("corporate byte bound"))?;
                        CorporateCell::Text(std::str::from_utf8(s).map_err(sql)?.into())
                    }
                    ValueRef::Blob(b) if *name == TABLES[0] && *col == "original_bytes" => {
                        if b.is_empty() || b.len() > 16 * 1024 * 1024 {
                            return Err(refused("corporate original byte bound"));
                        }
                        budget = budget
                            .checked_add(b.len().div_ceil(3) * 4)
                            .ok_or_else(|| refused("corporate byte bound"))?;
                        if budget > MAX {
                            return Err(refused("corporate signed section byte bound"));
                        }
                        CorporateCell::BlobBase64(STANDARD.encode(b))
                    }
                    _ => return Err(refused("unsupported corporate cell")),
                };
                if budget > MAX {
                    return Err(refused("corporate signed section byte bound"));
                }
                row.push(cell);
            }
            rows.push(row);
        }
        tables.insert(
            (*name).into(),
            CorporateTable {
                columns: cols.iter().map(|s| (*s).into()).collect(),
                rows,
            },
        );
    }
    let bytes = serde_json::to_vec(&Corporate {
        format: FORMAT.into(),
        tables,
    })
    .map_err(sql)?;
    if bytes.len() > MAX {
        return Err(refused("corporate signed section byte bound"));
    }
    Ok(bytes)
}
pub(super) fn export(tx: &Transaction<'_>) -> Result<Vec<u8>, Error> {
    let bytes = dump(tx)?;
    facts(tx)?;
    Ok(bytes)
}
pub fn initialize(db: &mut Connection, bytes: &[u8], companies: &[String]) -> Result<(), Error> {
    crate::control_migration::validate_current_control(db)?;
    shape(db)?;
    if bytes.len() > MAX {
        return Err(refused("corporate signed section byte bound"));
    }
    let codec: Corporate = serde_json::from_slice(bytes).map_err(sql)?;
    if codec.format != FORMAT || codec.tables.len() != TABLES.len() {
        return Err(refused("unsupported corporate codec"));
    }
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(sql)?;
    for (name, expected) in TABLES.iter().zip(COLS) {
        if tx
            .query_row(&format!("SELECT COUNT(*) FROM {name}"), [], |r| {
                r.get::<_, i64>(0)
            })
            .map_err(sql)?
            != 0
        {
            return Err(refused("fresh corporate target required"));
        }
        let table = codec
            .tables
            .get(*name)
            .ok_or_else(|| refused("corporate table missing"))?;
        if table.columns != expected || table.rows.len() > 100000 {
            return Err(refused("corporate table shape differs"));
        }
        let mut q = tx
            .prepare(&format!(
                "INSERT INTO {name} VALUES({})",
                vec!["?"; expected.len()].join(",")
            ))
            .map_err(sql)?;
        for row in &table.rows {
            if row.len() != expected.len() {
                return Err(refused("corporate row shape differs"));
            }
            let values = row
                .iter()
                .zip(expected)
                .map(|(cell, col)| {
                    let nullable =
                        *name == TABLES[0] && ["record_date", "retention_until"].contains(col);
                    Ok(match (cell, *col) {
                        (CorporateCell::Null, _) if nullable => rusqlite::types::Value::Null,
                        (CorporateCell::Integer(n), "id")
                            if [TABLES[1], TABLES[3]].contains(name) =>
                        {
                            rusqlite::types::Value::Integer(*n)
                        }
                        (CorporateCell::BlobBase64(s), "original_bytes") if *name == TABLES[0] => {
                            if s.len() > (16 * 1024 * 1024_usize).div_ceil(3) * 4 {
                                return Err(refused("corporate original byte bound"));
                            }
                            let raw = STANDARD.decode(s).map_err(sql)?;
                            if raw.is_empty()
                                || raw.len() > 16 * 1024 * 1024
                                || STANDARD.encode(&raw) != *s
                            {
                                return Err(refused("noncanonical corporate original encoding"));
                            }
                            rusqlite::types::Value::Blob(raw)
                        }
                        (CorporateCell::Text(s), _) if !["id", "original_bytes"].contains(col) => {
                            rusqlite::types::Value::Text(s.clone())
                        }
                        _ => return Err(refused("corporate cell type differs")),
                    })
                })
                .collect::<Result<Vec<_>, Error>>()?;
            q.execute(rusqlite::params_from_iter(values)).map_err(sql)?;
        }
    }
    facts(&tx)?;
    validate_companies(&tx, companies)?;
    if dump(&tx)? != bytes {
        return Err(refused("corporate codec order/types changed"));
    }
    crate::control_migration::validate_current_control(&tx)?;
    tx.commit().map_err(sql)
}
fn ensure(ok: bool, why: &str) -> Result<(), Error> {
    if ok { Ok(()) } else { Err(refused(why)) }
}
fn txt(s: &str, max: usize) -> Result<(), Error> {
    ensure(
        rentemester_engine::company_knowledge::text(s, max)? == s,
        "noncanonical corporate text",
    )
}
fn stamp(s: &str) -> Result<(), Error> {
    ensure(
        rentemester_engine::company_knowledge::iso(s)? == s,
        "noncanonical corporate timestamp",
    )
}
fn keys(v: &Value, expected: &[&str]) -> Result<(), Error> {
    ensure(
        v.as_object().is_some_and(|o| {
            o.len() == expected.len() && expected.iter().all(|k| o.contains_key(*k))
        }),
        "unsupported corporate payload",
    )
}
fn string<'a>(v: &'a Value, k: &str) -> Result<&'a str, Error> {
    v[k].as_str()
        .ok_or_else(|| refused("corporate payload field differs"))
}
const TYPES: &[&str] = &[
    "incorporation",
    "articles",
    "registration",
    "shareholder_register",
    "shareholder_agreement",
    "board_resolution",
    "general_meeting_minutes",
    "capital_or_dividend_resolution",
    "annual_report",
    "auditor_deliverable",
    "intercompany_agreement",
    "ownership_register",
    "share_transfer",
    "authority_letter",
    "bank_mandate",
    "tax_registration",
    "other",
];
fn stored_type(kind: &str) -> &str {
    if [
        "articles",
        "registration",
        "board_resolution",
        "ownership_register",
        "share_transfer",
        "authority_letter",
        "bank_mandate",
        "tax_registration",
        "other",
    ]
    .contains(&kind)
    {
        kind
    } else {
        "other"
    }
}
fn link_type(kind: &str) -> Result<(), Error> {
    ensure(
        [
            "company",
            "party",
            "ownership",
            "disposition",
            "legacy_evidence_ref",
        ]
        .contains(&kind),
        "unsupported corporate link type",
    )
}
type LinkKey = (String, String, String);
/// Only company/party have existence semantics in the source. Other link IDs stay opaque.
pub fn validate_companies(db: &Connection, companies: &[String]) -> Result<(), Error> {
    let known: BTreeSet<_> = companies.iter().collect();
    for (table, kind, id) in [
        (TABLES[2], "link_type", "link_id"),
        (TABLES[3], "scope_kind", "scope_id"),
    ] {
        let mut q = db
            .prepare(&format!(
                "SELECT DISTINCT {id} FROM {table} WHERE {kind}='company'"
            ))
            .map_err(sql)?;
        for s in q.query_map([], |r| r.get::<_, String>(0)).map_err(sql)? {
            ensure(
                known.contains(&s.map_err(sql)?),
                "corporate record refers to an unregistered company",
            )?;
        }
    }
    Ok(())
}
fn facts(tx: &Transaction<'_>) -> Result<(), Error> {
    shape(tx)?;
    let mut originals = BTreeMap::new();
    let mut q=tx.prepare("SELECT record_id,original_bytes,sha256,filename,source,received_at,record_date,retention_until,uploader,sensitivity FROM rm_corporate_record_bytes ORDER BY record_id").map_err(sql)?;
    let mut rows = q.query([]).map_err(sql)?;
    while let Some(r) = rows.next().map_err(sql)? {
        let id: String = r.get(0).map_err(sql)?;
        txt(&id, 64)?;
        let original: Vec<u8> = r.get(1).map_err(sql)?;
        let digest: String = r.get(2).map_err(sql)?;
        ensure(
            !original.is_empty()
                && original.len() <= crate::corporate_records::MAX_BYTES
                && crate::corporate_records::hash(&original) == digest,
            "corporate original/hash differs",
        )?;
        let filename: String = r.get(3).map_err(sql)?;
        txt(&filename, 320)?;
        let source: String = r.get(4).map_err(sql)?;
        txt(&source, 320)?;
        let received: String = r.get(5).map_err(sql)?;
        stamp(&received)?;
        let record_date: Option<String> = r.get(6).map_err(sql)?;
        let retention: Option<String> = r.get(7).map_err(sql)?;
        for date in [&record_date, &retention].into_iter().flatten() {
            stamp(date)?;
        }
        let uploader: String = r.get(8).map_err(sql)?;
        txt(&uploader, 160)?;
        let sensitivity: String = r.get(9).map_err(sql)?;
        ensure(
            ["normal", "confidential", "restricted"].contains(&sensitivity.as_str()),
            "corporate sensitivity differs",
        )?;
        originals.insert(id,json!({"sha256":digest,"filename":filename,"source":source,"receivedAt":received,"recordDate":record_date,"retentionUntil":retention,"uploader":uploader,"sensitivity":sensitivity}));
    }
    let mut kinds = BTreeMap::<String, String>::new();
    let mut linked = BTreeSet::<LinkKey>::new();
    let mut q=tx.prepare("SELECT id,record_id,event_type,record_type,payload_hash,canonical_payload,actor,created_at FROM rm_corporate_record_events ORDER BY id").map_err(sql)?;
    let mut rows = q.query([]).map_err(sql)?;
    while let Some(r) = rows.next().map_err(sql)? {
        ensure(
            r.get::<_, i64>(0).map_err(sql)? > 0,
            "corporate event id differs",
        )?;
        let id: String = r.get(1).map_err(sql)?;
        let event: String = r.get(2).map_err(sql)?;
        let kind: String = r.get(3).map_err(sql)?;
        let hash: String = r.get(4).map_err(sql)?;
        let raw: String = r.get(5).map_err(sql)?;
        let actor: String = r.get(6).map_err(sql)?;
        let at: String = r.get(7).map_err(sql)?;
        txt(&actor, 160)?;
        stamp(&at)?;
        let p: Value = serde_json::from_str(&raw).map_err(sql)?;
        ensure(
            serde_json::to_string(&p).map_err(sql)? == raw
                && crate::corporate_records::hash(raw.as_bytes()) == hash,
            "corporate event canonical/hash differs",
        )?;
        let original = originals
            .get(&id)
            .ok_or_else(|| refused("corporate event original missing"))?;
        if event == "ingested" {
            keys(
                &p,
                &[
                    "type",
                    "sha256",
                    "filename",
                    "source",
                    "receivedAt",
                    "recordDate",
                    "retentionUntil",
                    "uploader",
                    "sensitivity",
                ],
            )?;
            let full = string(&p, "type")?;
            ensure(
                TYPES.contains(&full) && stored_type(full) == kind && !kinds.contains_key(&id),
                "contradictory corporate ingested type/original",
            )?;
            let mut mirror = p.clone();
            mirror.as_object_mut().unwrap().remove("type");
            ensure(
                &mirror == original && p["receivedAt"] == at,
                "corporate ingested original mirror differs",
            )?;
            kinds.insert(id, full.to_owned());
            continue;
        }
        let full = kinds
            .get(&id)
            .ok_or_else(|| refused("corporate history precedes original ingestion"))?;
        ensure(
            stored_type(full) == kind,
            "corporate event record type differs",
        )?;
        match event.as_str() {
            "linked" => {
                keys(&p, &["type", "id"])?;
                let k = string(&p, "type")?;
                let target = string(&p, "id")?;
                link_type(k)?;
                txt(target, 256)?;
                linked.insert((id, k.into(), target.into()));
            }
            "enriched" => {
                keys(&p, &["assertion"])?;
                txt(string(&p, "assertion")?, 2000)?;
            }
            "superseded" => {
                keys(&p, &["replacementRecordId", "reason"])?;
                ensure(
                    kinds.contains_key(string(&p, "replacementRecordId")?),
                    "corporate replacement original missing",
                )?;
                txt(string(&p, "reason")?, 1000)?;
            }
            _ => return Err(refused("unsupported corporate event")),
        }
    }
    ensure(
        kinds.len() == originals.len(),
        "corporate original lacks unique ingestion event",
    )?;
    let mut mirrors = BTreeSet::<LinkKey>::new();
    let mut q=tx.prepare("SELECT record_id,link_type,link_id FROM rm_corporate_record_links ORDER BY record_id,link_type,link_id").map_err(sql)?;
    for row in q
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(sql)?
    {
        let row = row.map_err(sql)?;
        link_type(&row.1)?;
        txt(&row.2, 256)?;
        ensure(
            originals.contains_key(&row.0),
            "corporate link original missing",
        )?;
        if row.1 == "party" {
            ensure(
                crate::parties::exists(tx, &row.2)?,
                "corporate party target missing",
            )?;
        }
        mirrors.insert(row);
    }
    ensure(mirrors == linked, "corporate links/event mirrors differ")?;
    let mut q=tx.prepare("SELECT id,record_id,scope_kind,scope_id,actor,created_at FROM rm_corporate_record_scope_assertions ORDER BY id").map_err(sql)?;
    let mut rows = q.query([]).map_err(sql)?;
    while let Some(r) = rows.next().map_err(sql)? {
        ensure(
            r.get::<_, i64>(0).map_err(sql)? > 0,
            "corporate scope id differs",
        )?;
        let key = (
            r.get::<_, String>(1).map_err(sql)?,
            r.get::<_, String>(2).map_err(sql)?,
            r.get::<_, String>(3).map_err(sql)?,
        );
        ensure(
            mirrors.contains(&key),
            "corporate scope/link/event mirror differs",
        )?;
        txt(&r.get::<_, String>(4).map_err(sql)?, 160)?;
        stamp(&r.get::<_, String>(5).map_err(sql)?)?;
    }
    // Migration20 deliberately did not backfill pre-existing links. Preserve those rows
    // without synthesizing scope/authority; ordinary native reads keep them invisible.
    // Assertion provenance may differ from an older deduplicated linked event.
    Ok(())
}
#[cfg(test)]
mod tests;
