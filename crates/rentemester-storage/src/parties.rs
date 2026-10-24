//! Workspace canonical parties; caller must retain current control writer and visibility fence.
use rentemester_engine::{
    Error,
    company_knowledge::{iso, trim},
    parties::{self as policy, Identifier, bounded, refused},
};
use rusqlite::{OptionalExtension, Transaction, params};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Create {
    pub party_id: Option<String>,
    pub kind: String,
    pub name: String,
    pub aliases: Option<Vec<String>>,
    pub identifiers: Option<Vec<Identifier>>,
    pub source: String,
    pub observed_at: String,
    pub review_assertion: String,
    pub role: String,
    pub defaults: Option<serde_json::Map<String, Value>>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CreateIdentity {
    pub party_id: Option<String>,
    pub kind: String,
    pub name: String,
    pub aliases: Option<Vec<String>>,
    pub identifiers: Option<Vec<Identifier>>,
    pub source: String,
    pub observed_at: String,
    pub review_assertion: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Role {
    pub role: String,
    pub defaults: Option<serde_json::Map<String, Value>>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Assertion {
    pub field: String,
    pub value: String,
    pub source: String,
    pub observed_at: String,
    pub review_state: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Alias {
    pub alias: String,
    pub source: String,
    pub observed_at: String,
    pub review_state: String,
}
fn sql(_: impl std::fmt::Display) -> Error {
    refused("party control operation refused")
}
pub(crate) fn canonical(v: &Value) -> String {
    match v {
        Value::Object(m) => {
            let mut keys: Vec<_> = m.keys().collect();
            keys.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
            format!(
                "{{{}}}",
                keys.into_iter()
                    .map(|k| format!("{}:{}", json!(k), canonical(&m[k])))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
        Value::Array(a) => format!(
            "[{}]",
            a.iter().map(canonical).collect::<Vec<_>>().join(",")
        ),
        Value::Number(n) => ryu_js::Buffer::new()
            .format_finite(n.as_f64().unwrap())
            .into(),
        _ => v.to_string(),
    }
}
pub(crate) fn hash(v: &Value) -> String {
    format!("{:x}", Sha256::digest(canonical(v).as_bytes()))
}
pub fn exists(tx: &Transaction<'_>, id: &str) -> Result<bool, Error> {
    tx.query_row("SELECT EXISTS(SELECT 1 FROM main.rm_party_events WHERE party_id=? AND event_type='created')",[id],|r|r.get(0)).map_err(sql)
}
fn event(
    tx: &Transaction<'_>,
    id: &str,
    kind: &str,
    event: &str,
    payload: &Value,
    actor: &str,
    at: &str,
) -> Result<(), Error> {
    bounded(actor, 160)?;
    tx.execute("INSERT OR IGNORE INTO main.rm_party_events(party_id,event_type,canonical_kind,payload_hash,canonical_payload,actor,created_at) VALUES(?,?,?,?,?,?,?)",params![id,event,kind,hash(payload),canonical(payload),actor,at]).map_err(sql)?;
    Ok(())
}
/// Complete associated scopes stay private to authorization; never return hidden names/counts.
pub fn scopes(tx: &Transaction<'_>, id: &str) -> Result<BTreeSet<String>, Error> {
    let mut q = tx
        .prepare("SELECT DISTINCT company_slug FROM main.rm_party_company_roles WHERE party_id=?")
        .map_err(sql)?;
    let mut scopes = q
        .query_map([id], |r| r.get(0))
        .map_err(sql)?
        .collect::<Result<BTreeSet<_>, _>>()
        .map_err(sql)?;
    let mut q = tx.prepare("SELECT e.canonical_payload,e.payload_hash,e.canonical_kind,c.canonical_kind,e.id,c.id FROM main.rm_party_events e JOIN main.rm_party_events c ON c.party_id=e.party_id AND c.event_type='created' WHERE e.party_id=? AND e.event_type='linked'").map_err(sql)?;
    for row in q
        .query_map([id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
            ))
        })
        .map_err(sql)?
    {
        let (raw, h, kind, created_kind, event_id, created_id) = row.map_err(sql)?;
        let v: Value = serde_json::from_str(&raw).map_err(sql)?;
        if v.get("scopeKind").is_some() {
            if kind != created_kind
                || event_id <= created_id
                || !policy::KINDS.contains(&kind.as_str())
            {
                return Err(refused("invalid identity registration reference"));
            }
            scopes.insert(registration(&raw, &h)?);
        }
    }
    Ok(scopes)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Registration {
    company_slug: String,
    scope_kind: RegistrationKind,
}
#[derive(Deserialize)]
enum RegistrationKind {
    #[serde(rename = "identity_registration")]
    IdentityRegistration,
}
/// Exact typed private payload, including canonical text and hash, not a JSON authority hint.
pub(crate) fn registration(raw: &str, h: &str) -> Result<String, Error> {
    let r: Registration = serde_json::from_str(raw).map_err(sql)?;
    let _ = r.scope_kind;
    if bounded(&r.company_slug, 120)? != r.company_slug {
        return Err(refused("noncanonical identity scope"));
    }
    let v = json!({"companySlug":r.company_slug,"scopeKind":"identity_registration"});
    if canonical(&v) != raw || hash(&v) != h {
        return Err(refused("invalid identity scope hash"));
    }
    Ok(r.company_slug)
}
pub fn visible(
    tx: &Transaction<'_>,
    id: &str,
    companies: &BTreeSet<String>,
) -> Result<bool, Error> {
    Ok(scopes(tx, id)?.iter().any(|s| companies.contains(s)))
}
pub fn inspect(
    tx: &Transaction<'_>,
    id: &str,
    companies: &BTreeSet<String>,
) -> Result<Value, Error> {
    if !visible(tx, id, companies)? {
        return Err(Error::new("party_not_found", "party not found"));
    }
    let(kind,payload):(String,String)=tx.query_row("SELECT canonical_kind,canonical_payload FROM main.rm_party_events WHERE party_id=? AND event_type='created' ORDER BY id LIMIT 1",[id],|r|Ok((r.get(0)?,r.get(1)?))).map_err(sql)?;
    let mut p: Value = serde_json::from_str(&payload).map_err(sql)?;
    if !p.is_object() {
        return Err(sql("invalid party payload"));
    }
    p["partyId"] = json!(id);
    p["kind"] = json!(kind);
    let mut q=tx.prepare("SELECT company_slug,role,defaults_json FROM main.rm_party_company_roles WHERE party_id=? ORDER BY company_slug,role").map_err(sql)?;
    let roles=q.query_map([id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?))).map_err(sql)?.collect::<Result<Vec<_>,_>>().map_err(sql)?.into_iter().filter(|r|companies.contains(&r.0)).map(|r|Ok(json!({"companySlug":r.0,"role":r.1,"defaults":serde_json::from_str::<Value>(&r.2).map_err(sql)?}))).collect::<Result<Vec<_>,Error>>()?;
    p["roles"] = json!(roles);
    let mut q=tx.prepare("SELECT event_type,payload_hash,actor,created_at FROM main.rm_party_events WHERE party_id=? AND event_type!='linked' ORDER BY id").map_err(sql)?;
    p["history"]=json!(q.query_map([id],|r|Ok(json!({"event_type":r.get::<_,String>(0)?,"payload_hash":r.get::<_,String>(1)?,"actor":r.get::<_,String>(2)?,"created_at":r.get::<_,String>(3)?}))).map_err(sql)?.collect::<Result<Vec<_>,_>>().map_err(sql)?);
    for (alias, table) in [
        (true, "rm_party_alias_assertions"),
        (false, "rm_party_field_assertions"),
    ] {
        let columns = if alias {
            "alias,source,observed_at,review_state,payload_hash"
        } else {
            "field,value,source,observed_at,review_state,payload_hash"
        };
        let keys = if alias {
            vec![
                "alias",
                "source",
                "observed_at",
                "review_state",
                "payload_hash",
            ]
        } else {
            vec![
                "field",
                "value",
                "source",
                "observed_at",
                "review_state",
                "payload_hash",
            ]
        };
        let mut q = tx
            .prepare(&format!(
                "SELECT {columns} FROM main.{table} WHERE party_id=? ORDER BY id"
            ))
            .map_err(sql)?;
        let rows = q
            .query_map([id], |r| {
                let mut o = serde_json::Map::new();
                for (i, k) in keys.iter().enumerate() {
                    o.insert((*k).into(), json!(r.get::<_, String>(i)?));
                }
                Ok(Value::Object(o))
            })
            .map_err(sql)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql)?;
        if alias && !rows.is_empty() {
            p["aliases"] = json!(rows);
        } else if !alias {
            p["assertions"] = json!(rows);
        } else {
            p["aliases"]=json!(p["aliases"].as_array().ok_or_else(||sql("aliases"))?.iter().map(|a|json!({"alias":a,"source":"legacy-created","observed_at":null,"review_state":"approved","payload_hash":null})).collect::<Vec<_>>());
        }
    }
    Ok(p)
}
pub fn list(
    tx: &Transaction<'_>,
    companies: &BTreeSet<String>,
    query: Option<&str>,
    cursor: usize,
    limit: usize,
) -> Result<Value, Error> {
    let mut q=tx.prepare("SELECT party_id FROM main.rm_party_events WHERE event_type='created' ORDER BY party_id").map_err(sql)?;
    let ids = q
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    let query = query.map(|s| trim(s).to_lowercase());
    let mut rows = Vec::new();
    for id in ids {
        let mut roles = tx
            .prepare("SELECT company_slug FROM main.rm_party_company_roles WHERE party_id=?")
            .map_err(sql)?;
        let linked = roles
            .query_map([&id], |r| r.get::<_, String>(0))
            .map_err(sql)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql)?;
        if linked.iter().any(|slug| companies.contains(slug)) {
            let p = inspect(tx, &id, companies)?;
            if query.as_ref().is_none_or(|s| {
                p["name"]
                    .as_str()
                    .is_some_and(|n| n.to_lowercase().contains(s))
                    || p["aliases"].as_array().is_some_and(|a| {
                        a.iter().any(|a| {
                            a["alias"]
                                .as_str()
                                .is_some_and(|n| n.to_lowercase().contains(s))
                        })
                    })
            }) {
                rows.push(p);
            }
        }
    }
    let count = rows.len();
    let limit = limit.clamp(1, 100);
    let next = cursor.saturating_add(limit);
    Ok(
        json!({"rows":rows.into_iter().skip(cursor).take(limit).collect::<Vec<_>>(),"count":count,"nextCursor":if next<count{Some(next)}else{None}}),
    )
}
pub fn role(
    tx: &Transaction<'_>,
    id: &str,
    slug: &str,
    i: &Role,
    actor: &str,
    now: &str,
) -> Result<(), Error> {
    bounded(slug, 120)?;
    if !policy::ROLES.contains(&i.role.as_str()) {
        return Err(refused("unsupported party role"));
    }
    let defaults = i.defaults.clone().unwrap_or_default();
    if defaults
        .keys()
        .any(|k| !["account", "vat", "currency", "paymentTermsDays"].contains(&k.as_str()))
    {
        return Err(refused("unsupported company-scoped default"));
    }
    let encoded = canonical(&json!(defaults));
    let old:Option<String>=tx.query_row("SELECT defaults_json FROM main.rm_party_company_roles WHERE party_id=? AND company_slug=? AND role=?",[id,slug,&i.role],|r|r.get(0)).optional().map_err(sql)?;
    if old.is_some_and(|v| v != encoded) {
        return Err(refused(
            "conflicting company-scoped defaults require supersession",
        ));
    }
    let kind:String=tx.query_row("SELECT canonical_kind FROM main.rm_party_events WHERE party_id=? AND event_type='created'",[id],|r|r.get(0)).map_err(sql)?;
    tx.execute("INSERT OR IGNORE INTO main.rm_party_company_roles(party_id,company_slug,role,defaults_json) VALUES(?,?,?,?)",params![id,slug,i.role,encoded]).map_err(sql)?;
    event(
        tx,
        id,
        &kind,
        "linked",
        &json!({"companySlug":slug,"role":i.role,"defaults":defaults}),
        actor,
        now,
    )
}
fn review(s: &str) -> Result<(), Error> {
    if !["proposed", "approved", "rejected"].contains(&s) {
        return Err(refused("unsupported review state"));
    }
    Ok(())
}
pub fn assertion(
    tx: &Transaction<'_>,
    id: &str,
    i: &Assertion,
    actor: &str,
    now: &str,
) -> Result<(), Error> {
    bounded(actor, 160)?;
    review(&i.review_state)?;
    let field = bounded(&i.field, 64)?;
    let value = bounded(&i.value, 512)?;
    let source = bounded(&i.source, 160)?;
    let observed = iso(&i.observed_at)?;
    let payload = json!({"field":field,"value":value,"source":source,"observedAt":observed,"reviewState":i.review_state});
    tx.execute("INSERT OR IGNORE INTO main.rm_party_field_assertions(party_id,field,value,source,observed_at,review_state,payload_hash,actor,created_at) VALUES(?,?,?,?,?,?,?,?,?)",params![id,field,value,source,observed,i.review_state,hash(&payload),actor,now]).map_err(sql)?;
    Ok(())
}
pub fn alias(
    tx: &Transaction<'_>,
    id: &str,
    i: &Alias,
    actor: &str,
    now: &str,
) -> Result<(), Error> {
    bounded(actor, 160)?;
    review(&i.review_state)?;
    let alias = bounded(&i.alias, 160)?.to_lowercase();
    let source = bounded(&i.source, 160)?;
    let observed = iso(&i.observed_at)?;
    let payload =
        json!({"alias":alias,"source":source,"observedAt":observed,"reviewState":i.review_state});
    tx.execute("INSERT OR IGNORE INTO main.rm_party_alias_assertions(party_id,alias,source,observed_at,review_state,payload_hash,actor,created_at) VALUES(?,?,?,?,?,?,?,?)",params![id,alias,source,observed,i.review_state,hash(&payload),actor,now]).map_err(sql)?;
    Ok(())
}
pub fn create(
    tx: &Transaction<'_>,
    i: &Create,
    generated: &str,
    actor: &str,
    now: &str,
) -> Result<String, Error> {
    create_fields(
        tx,
        &CreateIdentity {
            party_id: i.party_id.clone(),
            kind: i.kind.clone(),
            name: i.name.clone(),
            aliases: i.aliases.clone(),
            identifiers: i.identifiers.clone(),
            source: i.source.clone(),
            observed_at: i.observed_at.clone(),
            review_assertion: i.review_assertion.clone(),
        },
        generated,
        actor,
        now,
    )
}
/// Caller supplies an authorized catalog selection, never a client-supplied scope.
pub fn create_identity(
    tx: &Transaction<'_>,
    i: &CreateIdentity,
    slug: &str,
    generated: &str,
    actor: &str,
    now: &str,
) -> Result<String, Error> {
    let slug = bounded(slug, 120)?;
    let candidate = bounded(
        i.party_id
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or(generated),
        64,
    )?;
    if exists(tx, &candidate)? {
        return Err(refused("partyId already exists"));
    }
    let id = create_fields(tx, i, generated, actor, now)?;
    event(
        tx,
        &id,
        &i.kind,
        "linked",
        &json!({"companySlug":slug,"scopeKind":"identity_registration"}),
        actor,
        now,
    )?;
    Ok(id)
}
fn create_fields(
    tx: &Transaction<'_>,
    i: &CreateIdentity,
    generated: &str,
    actor: &str,
    now: &str,
) -> Result<String, Error> {
    if !policy::KINDS.contains(&i.kind.as_str()) {
        return Err(refused("unsupported canonical party kind"));
    }
    let id = bounded(
        i.party_id
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or(generated),
        64,
    )?;
    let mut aliases = Vec::new();
    for a in i.aliases.as_deref().unwrap_or_default() {
        let a = bounded(a, 160)?;
        if !aliases.contains(&a) {
            aliases.push(a);
        }
    }
    if aliases.len() > 16 {
        return Err(refused("aliases bounded to16"));
    }
    let identifiers = i
        .identifiers
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(policy::identity)
        .collect::<Result<Vec<_>, _>>()?;
    let source = bounded(&i.source, 160)?;
    let observed = iso(&i.observed_at)?;
    let payload = json!({"name":bounded(&i.name,320)?,"aliases":aliases,"identifiers":identifiers,"source":source,"observedAt":observed,"reviewAssertion":bounded(&i.review_assertion,500)?});
    let existing:Option<String>=tx.query_row("SELECT canonical_payload FROM main.rm_party_events WHERE party_id=? AND event_type='created'",[&id],|r|r.get(0)).optional().map_err(sql)?;
    if let Some(old) = existing {
        if old != canonical(&payload) {
            return Err(refused(
                "partyId already exists with different immutable payload",
            ));
        }
        return Ok(id);
    }
    for item in &identifiers {
        let conflict:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM main.rm_party_identifiers WHERE jurisdiction=? AND identifier_kind=? AND identifier=? AND party_id!=?)",params![item.country,item.identifier_kind,item.identifier,id],|r|r.get(0)).map_err(sql)?;
        if conflict {
            return Err(refused("identifier conflicts with another canonical party"));
        }
    }
    event(tx, &id, &i.kind, "created", &payload, actor, &observed)?;
    for item in &identifiers {
        tx.execute("INSERT INTO main.rm_party_identifiers(party_id,jurisdiction,identifier_kind,identifier) VALUES(?,?,?,?)",params![id,item.country,item.identifier_kind,item.identifier]).map_err(sql)?;
    }
    for a in aliases {
        alias(
            tx,
            &id,
            &Alias {
                alias: a,
                source: source.clone(),
                observed_at: observed.clone(),
                review_state: "approved".into(),
            },
            actor,
            now,
        )?;
    }
    assertion(
        tx,
        &id,
        &Assertion {
            field: "name".into(),
            value: payload["name"].as_str().unwrap().into(),
            source: source.clone(),
            observed_at: observed.clone(),
            review_state: "approved".into(),
        },
        actor,
        now,
    )?;
    for item in identifiers {
        assertion(
            tx,
            &id,
            &Assertion {
                field: "identifier".into(),
                value: format!(
                    "{}:{}:{}",
                    item.country, item.identifier_kind, item.identifier
                ),
                source: source.clone(),
                observed_at: observed.clone(),
                review_state: "approved".into(),
            },
            actor,
            now,
        )?;
    }
    Ok(id)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProposeMerge {
    pub into_party_id: String,
    pub review_assertion: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ApproveMerge {
    pub proposal_hash: String,
}
fn merge_payload(i: &ProposeMerge) -> Result<Value, Error> {
    Ok(
        json!({"intoPartyId":bounded(&i.into_party_id,64)?,"reviewAssertion":bounded(&i.review_assertion,500)?}),
    )
}
/// Exact source-bound retained proposal, checked before using its target as authority.
pub fn merge_proposal(
    tx: &Transaction<'_>,
    id: &str,
    i: &ApproveMerge,
) -> Result<ProposeMerge, Error> {
    let h = &i.proposal_hash;
    if h.len() != 64
        || !h
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(refused("exact merge proposal not found"));
    }
    let raw: Option<String> = tx.query_row("SELECT canonical_payload FROM main.rm_party_events WHERE party_id=? AND event_type='proposed_merge' AND payload_hash=?", params![id,h], |r|r.get(0)).optional().map_err(sql)?;
    let raw = raw.ok_or_else(|| refused("exact merge proposal not found"))?;
    let input: ProposeMerge = serde_json::from_str(&raw).map_err(sql)?;
    let payload = merge_payload(&input)?;
    if canonical(&payload) != raw || hash(&payload) != *h {
        return Err(refused("invalid merge proposal"));
    }
    Ok(input)
}
fn merge_kind(tx: &Transaction<'_>, id: &str, into: &str) -> Result<String, Error> {
    if id == into {
        return Err(refused("cannot merge a party into itself"));
    }
    if !exists(tx, id)? || !exists(tx, into)? {
        return Err(Error::new("party_not_found", "party not found"));
    }
    tx.query_row(
        "SELECT canonical_kind FROM main.rm_party_events WHERE party_id=? AND event_type='created'",
        [id],
        |r| r.get(0),
    )
    .map_err(sql)
}
pub fn propose_merge(
    tx: &Transaction<'_>,
    id: &str,
    i: &ProposeMerge,
    actor: &str,
    now: &str,
) -> Result<String, Error> {
    let payload = merge_payload(i)?;
    let kind = merge_kind(tx, id, payload["intoPartyId"].as_str().unwrap())?;
    event(tx, id, &kind, "proposed_merge", &payload, actor, now)?;
    Ok(hash(&payload))
}
pub fn approve_merge(
    tx: &Transaction<'_>,
    id: &str,
    i: &ApproveMerge,
    actor: &str,
    now: &str,
) -> Result<(), Error> {
    let input = merge_proposal(tx, id, i)?;
    let payload = merge_payload(&input)?;
    let kind = merge_kind(tx, id, &input.into_party_id)?;
    event(tx, id, &kind, "approved_merge", &payload, actor, now)?;
    event(tx, id, &kind, "superseded", &payload, actor, now)
}
