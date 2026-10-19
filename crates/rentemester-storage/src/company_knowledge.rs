//! Append-only advisory knowledge in canonical workspace control, never company ledger.
use rentemester_engine::{
    Error,
    company_knowledge::{self as policy, refused, text},
};
use rusqlite::{OptionalExtension, Transaction, params};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
#[derive(Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub kind: String,
    pub r#ref: String,
}
#[derive(Deserialize, Clone)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Proposal {
    pub assertion_id: Option<String>,
    pub predicate: String,
    pub value: Value,
    pub source: Source,
    pub valid_from: String,
    pub valid_to_exclusive: Option<String>,
    pub certainty: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Review {
    pub assertion_id: String,
    pub decision: String,
    pub reason: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Supersede {
    pub assertion_id: String,
    pub replacement: Proposal,
}
fn sql(_: impl std::fmt::Display) -> Error {
    refused("knowledge control operation refused")
}
fn canonical(v: &Value, depth: usize) -> Result<String, Error> {
    if depth > 64 {
        return Err(refused("knowledge value nesting exceeds64"));
    }
    Ok(match v {
        Value::Object(m) => {
            let mut keys: Vec<_> = m.keys().collect();
            keys.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
            format!(
                "{{{}}}",
                keys.into_iter()
                    .map(|k| Ok(format!("{}:{}", json!(k), canonical(&m[k], depth + 1)?)))
                    .collect::<Result<Vec<_>, Error>>()?
                    .join(",")
            )
        }
        Value::Array(a) => format!(
            "[{}]",
            a.iter()
                .map(|v| canonical(v, depth + 1))
                .collect::<Result<Vec<_>, _>>()?
                .join(",")
        ),
        Value::Number(n) => ryu_js::Buffer::new()
            .format_finite(n.as_f64().ok_or_else(|| refused("invalid JSON number"))?)
            .into(),
        _ => v.to_string(),
    })
}
fn event(
    tx: &Transaction<'_>,
    id: &str,
    kind: &str,
    reason: Option<&str>,
    replacement: Option<&str>,
    subject: &str,
    now: &str,
) -> Result<(), Error> {
    tx.execute("INSERT INTO main.rm_company_knowledge_events(assertion_id,event_type,reason,supersedes_assertion_id,actor,principal_kind,principal_id,created_at) VALUES(?,?,?,?,?,'user',?,?)",params![id,kind,reason.map(policy::trim).filter(|s|!s.is_empty()),replacement,format!("user:{subject}"),subject,now]).map_err(sql)?;
    Ok(())
}
pub fn inspect(tx: &Transaction<'_>, slug: &str, id: &str) -> Result<Value, Error> {
    let mut v=tx.query_row("SELECT assertion_id,company_slug,predicate,value_json,value_hash,source_kind,source_ref,valid_from,valid_to_exclusive,certainty,actor,principal_kind,principal_id FROM main.rm_company_knowledge_assertions WHERE assertion_id=? AND company_slug=?",[id,slug],|r|Ok(json!({"assertionId":r.get::<_,String>(0)?,"companySlug":r.get::<_,String>(1)?,"predicate":r.get::<_,String>(2)?,"value":serde_json::from_str::<Value>(&r.get::<_,String>(3)?).map_err(|_|rusqlite::Error::InvalidQuery)?,"valueHash":r.get::<_,String>(4)?,"source":{"kind":r.get::<_,String>(5)?,"ref":r.get::<_,String>(6)?},"validFrom":r.get::<_,String>(7)?,"validToExclusive":r.get::<_,Option<String>>(8)?,"certainty":r.get::<_,String>(9)?,"actor":r.get::<_,String>(10)?,"principal":{"kind":r.get::<_,String>(11)?,"id":r.get::<_,String>(12)?}}))).optional().map_err(sql)?.ok_or_else(||Error::new("knowledge_not_found","assertion not found in current company"))?;
    let mut q=tx.prepare("SELECT event_type,reason,supersedes_assertion_id,actor,principal_kind,principal_id,created_at FROM main.rm_company_knowledge_events WHERE assertion_id=? ORDER BY id").map_err(sql)?;
    let history=q.query_map([id],|r|Ok(json!({"event_type":r.get::<_,String>(0)?,"reason":r.get::<_,Option<String>>(1)?,"supersedes_assertion_id":r.get::<_,Option<String>>(2)?,"actor":r.get::<_,String>(3)?,"principal_kind":r.get::<_,String>(4)?,"principal_id":r.get::<_,String>(5)?,"created_at":r.get::<_,String>(6)?}))).map_err(sql)?.collect::<Result<Vec<_>,_>>().map_err(sql)?;
    v["reviewState"] = history
        .last()
        .map(|e| e["event_type"].clone())
        .unwrap_or(json!("proposed"));
    v["history"] = json!(history);
    Ok(v)
}
pub fn propose(
    tx: &Transaction<'_>,
    slug: &str,
    i: &Proposal,
    generated_id: &str,
    subject: &str,
    now: &str,
) -> Result<Value, Error> {
    if !policy::PREDICATES.contains(&i.predicate.as_str())
        || !policy::SOURCES.contains(&i.source.kind.as_str())
    {
        return Err(refused("unsupported predicate or source kind"));
    }
    text(&format!("user:{subject}"), 160)?;
    text(subject, 160)?;
    text(slug, 120)?;
    let value = canonical(&i.value, 0)?;
    if value.encode_utf16().count() > 32768 {
        return Err(refused("knowledge value is too large"));
    }
    let hash = format!("{:x}", Sha256::digest(value.as_bytes()));
    let from = policy::iso(&i.valid_from)?;
    let to = i
        .valid_to_exclusive
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(policy::iso)
        .transpose()?;
    if to.as_ref().is_some_and(|t| t <= &from) {
        return Err(refused("validToExclusive must be after validFrom"));
    }
    let source = text(&i.source.r#ref, 512)?;
    let id = text(i.assertion_id.as_deref().unwrap_or(generated_id), 80)?;
    let certainty = i.certainty.as_deref().unwrap_or("confirmed");
    if !["confirmed", "probable", "uncertain"].contains(&certainty) {
        return Err(refused("unsupported certainty"));
    }
    if matches!(i.source.kind.as_str(), "party" | "corporate_record") {
        let statement = if i.source.kind == "party" {
            "SELECT EXISTS(SELECT 1 FROM main.rm_party_events WHERE party_id=? AND event_type='created')"
        } else {
            "SELECT EXISTS(SELECT 1 FROM main.rm_corporate_record_bytes WHERE record_id=?)"
        };
        if !tx
            .query_row(statement, [&source], |r| r.get::<_, bool>(0))
            .map_err(sql)?
        {
            return Err(refused("referenced knowledge source does not exist"));
        }
    }
    let existing:Option<String>=tx.query_row("SELECT assertion_id FROM main.rm_company_knowledge_assertions WHERE company_slug=? AND predicate=? AND value_hash=? AND source_kind=? AND source_ref=? AND valid_from=? AND COALESCE(valid_to_exclusive,'')=COALESCE(?,'')",params![slug,i.predicate,hash,i.source.kind,source,from,to],|r|r.get(0)).optional().map_err(sql)?;
    if let Some(id) = existing {
        return inspect(tx, slug, &id);
    }
    tx.execute("INSERT INTO main.rm_company_knowledge_assertions(assertion_id,company_slug,predicate,value_json,value_hash,source_kind,source_ref,valid_from,valid_to_exclusive,certainty,actor,principal_kind,principal_id,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,'user',?,?)",params![id,slug,i.predicate,value,hash,i.source.kind,source,from,to,certainty,format!("user:{subject}"),subject,now]).map_err(sql)?;
    event(tx, &id, "proposed", None, None, subject, now)?;
    inspect(tx, slug, &id)
}
pub fn review(
    tx: &Transaction<'_>,
    slug: &str,
    i: &Review,
    subject: &str,
    now: &str,
) -> Result<Value, Error> {
    if !["approved", "rejected"].contains(&i.decision.as_str()) {
        return Err(refused("decision must be approved or rejected"));
    }
    let old = inspect(tx, slug, &i.assertion_id)?;
    if old["reviewState"] == i.decision {
        return Ok(old);
    }
    if old["reviewState"] != "proposed" {
        return Err(refused("assertion already reviewed"));
    }
    event(
        tx,
        &i.assertion_id,
        &i.decision,
        i.reason.as_deref(),
        None,
        subject,
        now,
    )?;
    inspect(tx, slug, &i.assertion_id)
}
pub fn supersede(
    tx: &Transaction<'_>,
    slug: &str,
    i: &Supersede,
    id: &str,
    subject: &str,
    now: &str,
) -> Result<Value, Error> {
    let old = inspect(tx, slug, &i.assertion_id)?;
    if old["reviewState"] != "approved" {
        return Err(refused("only approved knowledge may be superseded"));
    }
    let replacement = propose(tx, slug, &i.replacement, id, subject, now)?;
    // Legacy proposal dedup can resolve to the original. Never append a self-link.
    if replacement["assertionId"] == i.assertion_id {
        return Err(refused("an assertion cannot supersede itself"));
    }
    event(
        tx,
        &i.assertion_id,
        "superseded",
        None,
        replacement["assertionId"].as_str(),
        subject,
        now,
    )?;
    Ok(json!({"superseded":inspect(tx,slug,&i.assertion_id)?,"replacement":replacement}))
}
pub fn read(
    tx: &Transaction<'_>,
    slug: &str,
    as_of: &str,
    include_proposed: bool,
) -> Result<Value, Error> {
    let at = policy::iso(as_of)?;
    let mut q=tx.prepare("SELECT assertion_id FROM main.rm_company_knowledge_assertions WHERE company_slug=? ORDER BY id").map_err(sql)?;
    let ids = q
        .query_map([slug], |r| r.get::<_, String>(0))
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    let history = ids
        .iter()
        .map(|id| inspect(tx, slug, id))
        .collect::<Result<Vec<_>, _>>()?;
    let mut active: Vec<_> = history
        .iter()
        .filter(|a| {
            a["validFrom"].as_str().is_some_and(|s| s <= at.as_str())
                && a["validToExclusive"]
                    .as_str()
                    .is_none_or(|s| s > at.as_str())
                && (a["reviewState"] == "approved"
                    || include_proposed && a["reviewState"] == "proposed")
        })
        .cloned()
        .collect();
    active.sort_by(|a, b| {
        a["predicate"]
            .as_str()
            .cmp(&b["predicate"].as_str())
            .then(a["assertionId"].as_str().cmp(&b["assertionId"].as_str()))
    });
    let conflicts: std::collections::BTreeSet<_> = active
        .iter()
        .filter_map(|a| a["predicate"].as_str())
        .filter(|p| {
            policy::singleton(p) && active.iter().filter(|a| a["predicate"] == *p).count() > 1
        })
        .map(str::to_owned)
        .collect();
    for a in &mut active {
        if conflicts.contains(a["predicate"].as_str().unwrap()) {
            a["reviewState"] = json!("conflict");
        }
    }
    Ok(
        json!({"context":{"companySlug":slug,"asOf":at,"assertions":active,"conflicts":conflicts,"safeForProductBehavior":conflicts.is_empty()},"history":history}),
    )
}
