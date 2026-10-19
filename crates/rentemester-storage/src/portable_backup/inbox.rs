//! Complete fixed inbox codec. Claims are historical fences, never credentials.
use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use rusqlite::OptionalExtension;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
const FORMAT: &str = "rentemester-native-workspace-inbox/1";
const MAX: usize = 32 * 1024 * 1024;
const TABLES: [&str; 5] = [
    "rm_workspace_inbox_sources",
    "rm_workspace_inbox_events",
    "rm_workspace_inbox_assignments",
    "rm_workspace_inbox_exceptions",
    "rm_workspace_inbox_handoff_claims",
];
const COLS: [&[&str]; 5] = [
    &[
        "source_id",
        "visibility_anchor_slug",
        "idempotency_key",
        "original_bytes",
        "sha256",
        "filename",
        "mime_type",
        "transport",
        "transport_identity",
        "received_at",
        "metadata_json",
        "created_by",
        "created_at",
    ],
    &[
        "id",
        "source_id",
        "event_type",
        "payload_hash",
        "canonical_payload",
        "actor",
        "created_at",
    ],
    &[
        "source_id",
        "company_slug",
        "state",
        "document_id",
        "document_no",
        "assigned_by",
        "assigned_at",
        "completed_at",
    ],
    &[
        "source_id",
        "code",
        "required_action",
        "opened_at",
        "resolved_at",
    ],
    &[
        "source_id",
        "company_slug",
        "source_hash",
        "state",
        "claim_id",
        "lease_expires_at",
        "document_id",
        "document_no",
        "created_at",
        "updated_at",
    ],
];
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", content = "value", deny_unknown_fields)]
enum InboxCell {
    Null,
    Integer(i64),
    Text(String),
    BlobBase64(String),
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct InboxTable {
    columns: Vec<String>,
    rows: Vec<Vec<InboxCell>>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Inbox {
    format: String,
    tables: BTreeMap<String, InboxTable>,
}
fn shape(db: &Connection) -> Result<(), Error> {
    for (table, cols) in TABLES.iter().zip(COLS) {
        if columns(db, table)? != cols {
            return Err(refused("inbox schema differs from codec v1"));
        }
    }
    Ok(())
}
fn dump(db: &Connection) -> Result<Vec<u8>, Error> {
    shape(db)?;
    let mut tables = BTreeMap::new();
    let mut budget = 0usize;
    for (name, cols) in TABLES.iter().zip(COLS) {
        let order = if *name == TABLES[1] {
            "id"
        } else if [TABLES[2], TABLES[4]].contains(name) {
            "source_id,company_slug"
        } else {
            "source_id"
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
                return Err(refused("inbox row bound"));
            }
            let mut row = vec![];
            for (i, col) in cols.iter().enumerate() {
                let cell = match r.get_ref(i).map_err(sql)? {
                    ValueRef::Null => InboxCell::Null,
                    ValueRef::Integer(n) => InboxCell::Integer(n),
                    ValueRef::Text(s) => {
                        budget = budget
                            .checked_add(s.len())
                            .ok_or_else(|| refused("inbox byte bound"))?;
                        InboxCell::Text(std::str::from_utf8(s).map_err(sql)?.into())
                    }
                    ValueRef::Blob(b) if *name == TABLES[0] && *col == "original_bytes" => {
                        if b.is_empty() || b.len() > 25 * 1024 * 1024 {
                            return Err(refused("inbox original byte bound"));
                        }
                        budget = budget
                            .checked_add(b.len().div_ceil(3) * 4)
                            .ok_or_else(|| refused("inbox byte bound"))?;
                        if budget > MAX {
                            return Err(refused("inbox signed section byte bound"));
                        }
                        InboxCell::BlobBase64(STANDARD.encode(b))
                    }
                    _ => return Err(refused("unsupported inbox cell")),
                };
                if budget > MAX {
                    return Err(refused("inbox signed section byte bound"));
                }
                row.push(cell);
            }
            rows.push(row);
        }
        tables.insert(
            (*name).into(),
            InboxTable {
                columns: cols.iter().map(|s| (*s).into()).collect(),
                rows,
            },
        );
    }
    let bytes = serde_json::to_vec(&Inbox {
        format: FORMAT.into(),
        tables,
    })
    .map_err(sql)?;
    if bytes.len() > MAX {
        return Err(refused("inbox signed section byte bound"));
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
        return Err(refused("inbox signed section byte bound"));
    }
    let codec: Inbox = serde_json::from_slice(bytes).map_err(sql)?;
    if codec.format != FORMAT || codec.tables.len() != TABLES.len() {
        return Err(refused("unsupported inbox codec"));
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
            return Err(refused("fresh inbox target required"));
        }
        let table = codec
            .tables
            .get(*name)
            .ok_or_else(|| refused("inbox table missing"))?;
        if table.columns != expected || table.rows.len() > 100000 {
            return Err(refused("inbox table shape differs"));
        }
        let mut q = tx
            .prepare(&format!(
                "INSERT INTO {name} VALUES({})",
                vec!["?"; expected.len()].join(",")
            ))
            .map_err(sql)?;
        for row in &table.rows {
            if row.len() != expected.len() {
                return Err(refused("inbox row shape differs"));
            }
            let values = row
                .iter()
                .zip(expected)
                .map(|(cell, col)| {
                    let nullable = match *name {
                        n if n == TABLES[0] => *col == "transport_identity",
                        n if n == TABLES[2] => {
                            ["document_id", "document_no", "completed_at"].contains(col)
                        }
                        n if n == TABLES[3] => *col == "resolved_at",
                        n if n == TABLES[4] => ["document_id", "document_no"].contains(col),
                        _ => false,
                    };
                    Ok(match (cell, *col) {
                        (InboxCell::Null, _) if nullable => rusqlite::types::Value::Null,
                        (InboxCell::Integer(n), "id" | "document_id") => {
                            rusqlite::types::Value::Integer(*n)
                        }
                        (InboxCell::BlobBase64(s), "original_bytes") if *name == TABLES[0] => {
                            if s.len() > (25 * 1024 * 1024_usize).div_ceil(3) * 4 {
                                return Err(refused("inbox original byte bound"));
                            }
                            let raw = STANDARD.decode(s).map_err(sql)?;
                            if raw.is_empty()
                                || raw.len() > 25 * 1024 * 1024
                                || STANDARD.encode(&raw) != *s
                            {
                                return Err(refused("noncanonical inbox original encoding"));
                            }
                            rusqlite::types::Value::Blob(raw)
                        }
                        (InboxCell::Text(s), _)
                            if !["id", "document_id", "original_bytes"].contains(col) =>
                        {
                            rusqlite::types::Value::Text(s.clone())
                        }
                        _ => return Err(refused("inbox cell type differs")),
                    })
                })
                .collect::<Result<Vec<_>, Error>>()?;
            q.execute(rusqlite::params_from_iter(values)).map_err(sql)?;
        }
    }
    facts(&tx)?;
    validate_companies(&tx, companies)?;
    if dump(&tx)? != bytes {
        return Err(refused("inbox codec order/types changed"));
    }
    crate::control_migration::validate_current_control(&tx)?;
    tx.commit().map_err(sql)
}
fn ensure(ok: bool, why: &str) -> Result<(), Error> {
    if ok { Ok(()) } else { Err(refused(why)) }
}
fn stamp(s: &str) -> Result<(), Error> {
    ensure(
        rentemester_engine::company_knowledge::iso(s)? == s,
        "noncanonical inbox timestamp",
    )
}
fn txt(s: &str, max: usize) -> Result<(), Error> {
    ensure(
        !s.chars().any(char::is_control)
            && rentemester_engine::company_knowledge::text(s, max)? == s,
        "noncanonical inbox text",
    )
}
fn keys(v: &Value, expected: &[&str]) -> Result<(), Error> {
    ensure(
        v.as_object().is_some_and(|o| {
            o.len() == expected.len() && expected.iter().all(|k| o.contains_key(*k))
        }),
        "unsupported inbox event payload",
    )
}
fn string<'a>(v: &'a Value, key: &str) -> Result<&'a str, Error> {
    v[key]
        .as_str()
        .ok_or_else(|| refused("inbox payload field differs"))
}
fn same_time(event: &Value, time: &str, actor: &str) -> Result<(), Error> {
    ensure(
        event["created_at"] == time && event["actor"] == actor,
        "inbox event provenance differs",
    )
}
type ClaimFacts = (
    String,
    String,
    String,
    String,
    String,
    Option<i64>,
    Option<String>,
);
fn facts(tx: &Transaction<'_>) -> Result<(), Error> {
    shape(tx)?;
    let mut q=tx.prepare("SELECT source_id,visibility_anchor_slug,idempotency_key,original_bytes,sha256,filename,mime_type,transport,transport_identity,received_at,metadata_json,created_by,created_at FROM rm_workspace_inbox_sources ORDER BY source_id").map_err(sql)?;
    let mut rows = q.query([]).map_err(sql)?;
    let mut known = BTreeSet::new();
    while let Some(r) = rows.next().map_err(sql)? {
        let id: String = r.get(0).map_err(sql)?;
        let anchor: String = r.get(1).map_err(sql)?;
        let key: String = r.get(2).map_err(sql)?;
        let bytes: Vec<u8> = r.get(3).map_err(sql)?;
        let hash: String = r.get(4).map_err(sql)?;
        let filename: String = r.get(5).map_err(sql)?;
        let mime: String = r.get(6).map_err(sql)?;
        let transport: String = r.get(7).map_err(sql)?;
        let identity: Option<String> = r.get(8).map_err(sql)?;
        let received: String = r.get(9).map_err(sql)?;
        let raw: String = r.get(10).map_err(sql)?;
        let actor: String = r.get(11).map_err(sql)?;
        let at: String = r.get(12).map_err(sql)?;
        txt(&id, 100)?;
        txt(&anchor, 100)?;
        txt(&key, 200)?;
        txt(&filename, 512)?;
        txt(&mime, 160)?;
        txt(&actor, 160)?;
        stamp(&received)?;
        stamp(&at)?;
        if let Some(i) = &identity {
            txt(i, 512)?;
        }
        ensure(
            ["upload", "mail", "connector"].contains(&transport.as_str()),
            "unsupported inbox transport",
        )?;
        ensure(
            !bytes.is_empty()
                && bytes.len() <= 25 * 1024 * 1024
                && format!("{:x}", Sha256::digest(&bytes)) == hash,
            "inbox original hash/size differs",
        )?;
        ensure(raw.len() <= 64 * 1024, "inbox metadata bound")?;
        let metadata: Value = serde_json::from_str(&raw).map_err(sql)?;
        ensure(
            metadata.is_object() && crate::workspace_inbox::canonical(&metadata)? == raw,
            "inbox source metadata differs",
        )?;
        // Use the actual native projection/hash validation with every source-declared
        // company visible. Authorization is deliberately not imported into this codec.
        let mut visible = BTreeSet::from([anchor.clone()]);
        let mut targets=tx.prepare("SELECT company_slug FROM rm_workspace_inbox_assignments WHERE source_id=? UNION SELECT company_slug FROM rm_workspace_inbox_handoff_claims WHERE source_id=?").map_err(sql)?;
        for s in targets
            .query_map([&id, &id], |r| r.get::<_, String>(0))
            .map_err(sql)?
        {
            visible.insert(s.map_err(sql)?);
        }
        let mut events=tx.prepare("SELECT canonical_payload FROM rm_workspace_inbox_events WHERE source_id=? ORDER BY id").map_err(sql)?;
        for raw in events
            .query_map([&id], |r| r.get::<_, String>(0))
            .map_err(sql)?
        {
            let p: Value = serde_json::from_str(&raw.map_err(sql)?).map_err(sql)?;
            for k in ["companySlug", "requestedCompanySlug", "existingCompanySlug"] {
                if let Some(s) = p[k].as_str() {
                    visible.insert(s.into());
                }
            }
            if let Some(c) = p["candidates"].as_array() {
                for c in c {
                    if let Some(s) = c["companySlug"].as_str() {
                        visible.insert(s.into());
                    }
                }
            }
        }
        let source = crate::workspace_inbox::inspect(tx, &anchor, &id, &visible)?;
        let history = source["history"]
            .as_array()
            .ok_or_else(|| refused("inbox history unavailable"))?;
        let mut ingested = vec![];
        let mut candidates = vec![];
        let mut assigned = vec![];
        let mut completed = vec![];
        let mut exceptions = vec![];
        let mut denied = vec![];
        for e in history {
            let kind = string(e, "event_type")?;
            stamp(string(e, "created_at")?)?;
            txt(string(e, "actor")?, 160)?;
            let p = &e["payload"];
            match kind {
                "ingested" => {
                    keys(
                        p,
                        &[
                            "sourceHash",
                            "filename",
                            "mimeType",
                            "transport",
                            "transportIdentity",
                        ],
                    )?;
                    ensure(
                        *p == json!({"sourceHash":hash,"filename":filename,"mimeType":mime,"transport":transport,"transportIdentity":identity}),
                        "ingested source proof differs",
                    )?;
                    same_time(e, &at, &actor)?;
                    ingested.push(e);
                }
                "candidate_resolved" => {
                    keys(p, &["candidates"])?;
                    let claims = p["candidates"]
                        .as_array()
                        .ok_or_else(|| refused("inbox candidates unavailable"))?;
                    ensure(claims.len() <= 100, "inbox candidate count bound")?;
                    let mut seen = BTreeSet::new();
                    for c in claims {
                        keys(c, &["companySlug", "evidenceKind", "evidence"])?;
                        txt(string(c, "companySlug")?, 100)?;
                        txt(string(c, "evidence")?, 1000)?;
                        ensure(
                            [
                                "recipient_alias",
                                "buyer_vat",
                                "buyer_name",
                                "account",
                                "portal",
                            ]
                            .contains(&string(c, "evidenceKind")?),
                            "inbox candidate evidence kind differs",
                        )?;
                        ensure(
                            seen.insert(crate::workspace_inbox::canonical(c)?),
                            "duplicate inbox candidate",
                        )?;
                    }
                    same_time(e, &at, &actor)?;
                    candidates.push(e);
                }
                "exception_opened" => {
                    keys(p, &["code", "candidates", "requiredAction"])?;
                    exceptions.push(e);
                }
                "assigned" => {
                    keys(p, &["companySlug"])?;
                    assigned.push(e);
                }
                "handoff_completed" => {
                    keys(
                        p,
                        &["companySlug", "documentId", "documentNo", "sourceHash"],
                    )?;
                    completed.push(e);
                }
                "reassignment_denied" => {
                    denied.push(e);
                    keys(
                        p,
                        &["requestedCompanySlug", "existingCompanySlug", "reason"],
                    )?;
                    ensure(
                        p["reason"] == "company document already handed off"
                            && p["requestedCompanySlug"] != p["existingCompanySlug"],
                        "invalid denied routing proof",
                    )?;
                }
                _ => return Err(refused("unsupported inbox event kind")),
            }
        }
        ensure(
            ingested.len() == 1
                && candidates.len() == 1
                && history
                    .first()
                    .is_some_and(|h| h["event_type"] == "ingested"),
            "inbox ingress proof missing or duplicated",
        )?;
        let candidate_list = candidates[0]["payload"]["candidates"]
            .as_array()
            .ok_or_else(|| refused("candidate proof missing"))?;
        let candidate_companies = candidate_list
            .iter()
            .map(|c| c["companySlug"].as_str().unwrap())
            .collect::<BTreeSet<_>>();
        let exception = &source["exception"];
        if candidate_companies.len() == 1 {
            ensure(
                exceptions.is_empty() && exception.is_null(),
                "unexpected inbox exception",
            )?;
        } else {
            ensure(
                exceptions.len() == 1 && !exception.is_null(),
                "inbox exception proof missing",
            )?;
            let code = if candidate_companies.is_empty() {
                "INBOX_NO_CANDIDATE"
            } else {
                "INBOX_AMBIGUOUS_CANDIDATES"
            };
            ensure(
                exceptions[0]["payload"]
                    == json!({"code":code,"candidates":candidate_list,"requiredAction":"review_and_assign"})
                    && exception["code"] == code
                    && exception["requiredAction"] == "review_and_assign"
                    && exception["openedAt"] == at,
                "inbox exception facts differ",
            )?;
            same_time(exceptions[0], &at, &actor)?;
        }
        let assignments = source["assignments"].as_array().unwrap();
        let claims = source["handoffs"].as_array().unwrap();
        ensure(
            assignments.len() <= 1 && claims.len() <= 1,
            "multiple inbox destinations/claims",
        )?;
        if let Some(a) = assignments.first() {
            let target = string(a, "companySlug")?;
            let state = string(a, "state")?;
            txt(target, 100)?;
            stamp(string(a, "assignedAt")?)?;
            txt(string(a, "assignedBy")?, 160)?;
            ensure(
                assigned.len() == 1 && assigned[0]["payload"]["companySlug"] == target,
                "assignment proof differs",
            )?;
            same_time(
                assigned[0],
                string(a, "assignedAt")?,
                string(a, "assignedBy")?,
            )?;
            if !exception.is_null() {
                ensure(
                    exception["resolvedAt"] == a["assignedAt"],
                    "exception resolution differs",
                )?;
            }
            let claim:Option<ClaimFacts>=tx.query_row("SELECT source_hash,state,claim_id,lease_expires_at,updated_at,document_id,document_no FROM rm_workspace_inbox_handoff_claims WHERE source_id=? AND company_slug=?",[&id,target],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?))).optional().map_err(sql)?;
            if let Some((claim_hash, claim_state, claim_id, expiry, updated, doc, no)) = claim {
                ensure(claim_hash == hash, "claim original differs")?;
                txt(&claim_id, 160)?;
                stamp(&expiry)?;
                stamp(&updated)?;
                let created:String=tx.query_row("SELECT created_at FROM rm_workspace_inbox_handoff_claims WHERE source_id=?",[&id],|r|r.get(0)).map_err(sql)?;
                stamp(&created)?;
                ensure(
                    expiry > created && (claim_state != "claimed" || expiry > updated),
                    "claim expiry differs",
                )?;
                if state == "completed" {
                    stamp(string(a, "completedAt")?)?;
                    ensure(
                        claim_state == "completed"
                            && doc.is_some()
                            && json!(doc) == a["documentId"]
                            && json!(no) == a["documentNo"]
                            && completed.len() == 1
                            && completed[0]["created_at"] == a["completedAt"]
                            && updated == string(a, "completedAt")?,
                        "completed handoff state differs",
                    )?;
                    ensure(
                        completed[0]["payload"]
                            == json!({"companySlug":target,"documentId":doc,"documentNo":no,"sourceHash":hash}),
                        "completed handoff proof differs",
                    )?;
                } else {
                    ensure(
                        state == "approved"
                            && claim_state == "claimed"
                            && doc.is_none()
                            && no.is_none()
                            && completed.is_empty()
                            && a["documentId"].is_null()
                            && a["documentNo"].is_null()
                            && a["completedAt"].is_null(),
                        "pending claim state differs",
                    )?;
                }
            } else {
                ensure(claims.is_empty(), "claim crosses assignment destination")?;
                ensure(
                    state == "approved"
                        && completed.is_empty()
                        && a["documentId"].is_null()
                        && a["documentNo"].is_null()
                        && a["completedAt"].is_null(),
                    "assignment missing completed claim",
                )?;
            }
        } else {
            ensure(
                assigned.is_empty()
                    && completed.is_empty()
                    && claims.is_empty()
                    && (exception.is_null() || exception["resolvedAt"].is_null()),
                "orphan inbox handoff/assignment proof",
            )?;
        }
        for event in denied {
            ensure(
                completed.len() == 1
                    && event["payload"]["existingCompanySlug"]
                        == completed[0]["payload"]["companySlug"],
                "denied reassignment has no original completed target",
            )?;
        }
        known.insert(id);
    }
    for table in &TABLES[1..] {
        let mut q = tx
            .prepare(&format!("SELECT DISTINCT source_id FROM {table}"))
            .map_err(sql)?;
        for id in q.query_map([], |r| r.get::<_, String>(0)).map_err(sql)? {
            ensure(
                known.contains(&id.map_err(sql)?),
                "orphan inbox source reference",
            )?;
        }
    }
    Ok(())
}
pub fn validate_companies(db: &Connection, companies: &[String]) -> Result<(), Error> {
    let known = companies.iter().collect::<BTreeSet<_>>();
    for table in [TABLES[0], TABLES[2], TABLES[4]] {
        let col = if table == TABLES[0] {
            "visibility_anchor_slug"
        } else {
            "company_slug"
        };
        let mut q = db
            .prepare(&format!("SELECT DISTINCT {col} FROM {table}"))
            .map_err(sql)?;
        for slug in q.query_map([], |r| r.get::<_, String>(0)).map_err(sql)? {
            ensure(
                known.contains(&slug.map_err(sql)?),
                "inbox refers to unregistered company",
            )?;
        }
    }
    let mut q = db
        .prepare("SELECT canonical_payload FROM rm_workspace_inbox_events")
        .map_err(sql)?;
    for raw in q.query_map([], |r| r.get::<_, String>(0)).map_err(sql)? {
        let v: Value = serde_json::from_str(&raw.map_err(sql)?).map_err(sql)?;
        for key in ["companySlug", "requestedCompanySlug", "existingCompanySlug"] {
            if let Some(s) = v[key].as_str() {
                ensure(
                    known.iter().any(|c| c.as_str() == s),
                    "inbox event refers to unregistered company",
                )?;
            }
        }
        if let Some(c) = v["candidates"].as_array() {
            for c in c {
                ensure(
                    c["companySlug"]
                        .as_str()
                        .is_some_and(|s| known.iter().any(|c| c.as_str() == s)),
                    "inbox candidate refers to unregistered company",
                )?;
            }
        }
    }
    Ok(())
}
/// Cross-database evidence under the continuously held owner. Company ledger
/// validity and actual namespace/archive original inventory are checked by host.
pub fn validate_company(
    control: &Connection,
    company: &Connection,
    slug: &str,
) -> Result<(), Error> {
    crate::validate(company)?;
    let mut q=company.prepare("SELECT id,document_no,source,sha256_hash,payload_json,stored_path,original_filename,mime_type FROM documents WHERE substr(source,1,16)='workspace-inbox:' ORDER BY id").map_err(sql)?;
    let mut found = BTreeMap::new();
    for row in q
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, String>(7)?,
            ))
        })
        .map_err(sql)?
    {
        let (id, no, marker, hash, payload, path, filename, mime) = row.map_err(sql)?;
        let source = marker
            .strip_prefix("workspace-inbox:")
            .ok_or_else(|| refused("inbox marker differs"))?;
        ensure(
            !found.contains_key(source),
            "multiple documents claim one inbox source",
        )?;
        let source_row: Option<(String, String, String, String)> = control
            .query_row(
                "SELECT sha256,metadata_json,filename,mime_type FROM rm_workspace_inbox_sources WHERE source_id=?",
                [source],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()
            .map_err(sql)?;
        let Some((original_hash, metadata, original_filename, original_mime)) = source_row else {
            return Err(refused("company document references missing inbox source"));
        };
        ensure(
            hash == original_hash
                && path.contains(&hash)
                && filename == original_filename
                && mime == original_mime,
            "inbox target original differs",
        )?;
        let mut metadata: Value = serde_json::from_str(&metadata).map_err(sql)?;
        metadata["source"] = json!(marker);
        let expected: rentemester_engine::document::Metadata =
            serde_json::from_value(metadata).map_err(sql)?;
        expected.validate()?;
        ensure(
            serde_json::from_str::<Value>(&payload).map_err(sql)?
                == serde_json::to_value(expected).map_err(sql)?,
            "inbox target metadata differs",
        )?;
        let assignment:Option<(String,String,Option<i64>,Option<String>)>=control.query_row("SELECT company_slug,state,document_id,document_no FROM rm_workspace_inbox_assignments WHERE source_id=?",[source],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(sql)?;
        let Some((target, state, doc, number)) = assignment else {
            return Err(refused("inbox document has no approved assignment"));
        };
        ensure(target == slug, "inbox document crosses legal destination")?;
        let claim:Option<String>=control.query_row("SELECT state FROM rm_workspace_inbox_handoff_claims WHERE source_id=? AND company_slug=?",[source,slug],|r|r.get(0)).optional().map_err(sql)?;
        ensure(
            if state == "completed" {
                claim.as_deref() == Some("completed")
                    && doc == Some(id)
                    && number.as_deref() == Some(no.as_str())
            } else {
                state == "approved" && claim.as_deref() == Some("claimed")
            },
            "inbox document claim/state differs",
        )?;
        found.insert(source.to_owned(), id);
    }
    let mut q=control.prepare("SELECT source_id,document_id FROM rm_workspace_inbox_assignments WHERE company_slug=? AND state='completed'").map_err(sql)?;
    for row in q
        .query_map([slug], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })
        .map_err(sql)?
    {
        let (source, id) = row.map_err(sql)?;
        ensure(
            found.get(&source) == Some(&id),
            "completed inbox target document missing",
        )?;
    }
    Ok(())
}
#[cfg(test)]
mod tests;
