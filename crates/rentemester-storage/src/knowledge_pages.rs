//! Programmatic knowledge core on the admitted control writer. No public management routes/grants.
use domain::{Endpoint, Page, PageInput, Relation, RelationInput, Scope};
use rentemester_engine::{Error, knowledge_pages as domain};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde_json::{Value, json};
fn sql(_: impl std::fmt::Display) -> Error {
    Error::new("knowledge_page_refused", "Knowledge operation refused")
}
fn conflict() -> Error {
    Error::new("version_conflict", "Knowledge version/hash differs")
}
const PAGE: &str = "page_id,version,event_type,scope_kind,company_slug,slug,title,body_markdown,provenance_kind,provenance_ref,effective_from,effective_to_exclusive,prior_event_hash,event_hash,actor,principal,created_at";
const RELATION: &str = "relation_id,version,event_type,scope_kind,company_slug,relation_type,subject_kind,subject_ref,object_kind,object_ref,provenance_kind,provenance_ref,effective_from,effective_to_exclusive,prior_event_hash,event_hash,actor,principal,created_at";
fn scope(kind: String, slug: Option<String>) -> rusqlite::Result<Scope> {
    match (kind.as_str(), slug) {
        ("workspace", None) => Ok(Scope::Workspace),
        ("company", Some(company_slug)) => Ok(Scope::Company { company_slug }),
        _ => Err(rusqlite::Error::InvalidQuery),
    }
}
fn page_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Page> {
    Ok(Page {
        page_id: r.get(0)?,
        version: u64::try_from(r.get::<_, i64>(1)?).map_err(|_| rusqlite::Error::InvalidQuery)?,
        event_type: r.get(2)?,
        scope: scope(r.get(3)?, r.get(4)?)?,
        slug: r.get(5)?,
        title: r.get(6)?,
        body_markdown: r.get(7)?,
        provenance: domain::Provenance {
            kind: r.get(8)?,
            r#ref: r.get(9)?,
        },
        effective_from: r.get(10)?,
        effective_to_exclusive: r.get(11)?,
        prior_event_hash: r.get(12)?,
        event_hash: r.get(13)?,
        actor: r.get(14)?,
        principal: r.get(15)?,
        created_at: r.get(16)?,
    })
}
fn relation_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Relation> {
    Ok(Relation {
        relation_id: r.get(0)?,
        version: u64::try_from(r.get::<_, i64>(1)?).map_err(|_| rusqlite::Error::InvalidQuery)?,
        event_type: r.get(2)?,
        scope: scope(r.get(3)?, r.get(4)?)?,
        r#type: r.get(5)?,
        subject: Endpoint {
            kind: r.get(6)?,
            r#ref: r.get(7)?,
        },
        object: Endpoint {
            kind: r.get(8)?,
            r#ref: r.get(9)?,
        },
        provenance: domain::Provenance {
            kind: r.get(10)?,
            r#ref: r.get(11)?,
        },
        effective_from: r.get(12)?,
        effective_to_exclusive: r.get(13)?,
        prior_event_hash: r.get(14)?,
        event_hash: r.get(15)?,
        actor: r.get(16)?,
        principal: r.get(17)?,
        created_at: r.get(18)?,
    })
}
/// Explicit internal attribution; never parsed from an HTTP/CLI/MCP payload.
pub struct Context<'a> {
    pub actor: &'a str,
    pub principal: &'a str,
    pub now: &'a str,
}
fn writer(tx: &Transaction<'_>) -> Result<(), Error> {
    crate::workspace_members::writer(tx)?;
    crate::workspace_transfer::require_business_write(tx)?;
    crate::control_migration::validate_current_control(tx)
}
pub fn page_history(db: &Connection, target: &Scope, id: &str) -> Result<Vec<Page>, Error> {
    target.validate()?;
    if !domain::identifier(id) {
        return Err(sql("id"));
    }
    let mut q=db.prepare(&format!("SELECT {PAGE} FROM rm_knowledge_page_events WHERE page_id=?1 AND scope_kind=?2 AND company_slug IS ?3 ORDER BY version")).map_err(sql)?;
    let rows = q
        .query_map(params![id, target.kind(), target.company()], page_row)
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    let mut prior: Option<&Page> = None;
    for p in &rows {
        p.validate()?;
        if p.version != prior.map_or(1, |p| p.version + 1)
            || p.prior_event_hash.as_deref() != prior.map(|p| p.event_hash.as_str())
            || p.event_type
                != if prior.is_none() {
                    "created"
                } else {
                    "superseded"
                }
        {
            return Err(sql("page chain"));
        }
        prior = Some(p);
    }
    Ok(rows)
}
pub fn pages(db: &Connection, target: &Scope, at: Option<&str>) -> Result<Vec<Value>, Error> {
    target.validate()?;
    let at = at.map(domain_time).transpose()?;
    let mut q=db.prepare(&format!("SELECT {PAGE} FROM rm_current_knowledge_pages WHERE scope_kind=?1 AND company_slug IS ?2 ORDER BY slug,page_id")).map_err(sql)?;
    let rows = q
        .query_map(params![target.kind(), target.company()], page_row)
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    rows.iter()
        .filter(|p| {
            domain::visible(
                &p.effective_from,
                p.effective_to_exclusive.as_deref(),
                at.as_deref(),
            )
        })
        .map(|p| {
            p.validate()?;
            p.output(false)
        })
        .collect()
}
fn domain_time(at: &str) -> Result<String, Error> {
    rentemester_engine::company_knowledge::iso(at)
}
/// Called only after current URL-scope authority; it never opens another scope's content.
pub fn read_page(
    db: &Connection,
    target: &Scope,
    reference: &str,
    at: &str,
) -> Result<Option<Value>, Error> {
    target.validate()?;
    if !domain::slug(reference) {
        return Ok(None);
    }
    let at = domain_time(at)?;
    let p=db.query_row(&format!("SELECT {PAGE} FROM rm_current_knowledge_pages WHERE scope_kind=?1 AND company_slug IS ?2 AND (page_id=?3 OR slug=?3) AND effective_from<=?4 AND (effective_to_exclusive IS NULL OR ?4<effective_to_exclusive) ORDER BY slug,page_id LIMIT 1"),params![target.kind(),target.company(),reference,at],page_row).optional().map_err(sql)?;
    p.map(|p| {
        p.validate()?;
        p.output(true)
    })
    .transpose()
}
pub fn append_page(
    tx: &Transaction<'_>,
    input: &PageInput,
    generated_id: &str,
    expected: Option<(u64, &str)>,
    ctx: &Context<'_>,
) -> Result<Value, Error> {
    writer(tx)?;
    let id = input.page_id.as_deref().unwrap_or(generated_id);
    let history = page_history(tx, &input.scope, id)?;
    let prior = history.last();
    match (expected, prior) {
        (None, None) => {}
        (Some((version, hash)), Some(p)) if p.version == version && p.event_hash == hash => {}
        _ => return Err(conflict()),
    }
    let p = domain::page(input, id, prior, ctx.actor, ctx.principal, ctx.now)?;
    tx.execute("INSERT INTO rm_knowledge_page_events(page_id,version,event_type,scope_kind,company_slug,slug,title,body_markdown,provenance_kind,provenance_ref,effective_from,effective_to_exclusive,prior_event_hash,event_hash,actor,principal,created_at)VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",params![p.page_id,i64::try_from(p.version).map_err(sql)?,p.event_type,p.scope.kind(),p.scope.company(),p.slug,p.title,p.body_markdown,p.provenance.kind,p.provenance.r#ref,p.effective_from,p.effective_to_exclusive,p.prior_event_hash,p.event_hash,p.actor,p.principal,p.created_at]).map_err(sql)?;
    p.output(true)
}
fn endpoint(db: &Connection, target: &Scope, e: &Endpoint) -> Result<(), Error> {
    if !domain::identifier(&e.r#ref) {
        return Err(sql("endpoint"));
    }
    let valid=match e.kind.as_str(){"page"=>db.query_row("SELECT EXISTS(SELECT 1 FROM rm_current_knowledge_pages WHERE page_id=?1 AND scope_kind=?2 AND company_slug IS ?3)",params![e.r#ref,target.kind(),target.company()],|r|r.get(0)).map_err(sql)?,"party"=>db.query_row("SELECT EXISTS(SELECT 1 FROM rm_party_events WHERE party_id=?1)",[&e.r#ref],|r|r.get(0)).map_err(sql)?,"company"=>target.company()==Some(e.r#ref.as_str()),_=>false};
    if valid {
        Ok(())
    } else {
        Err(sql("endpoint missing in scope"))
    }
}
pub fn relation_history(db: &Connection, target: &Scope, id: &str) -> Result<Vec<Relation>, Error> {
    target.validate()?;
    if !domain::identifier(id) {
        return Err(sql("id"));
    }
    let mut q=db.prepare(&format!("SELECT {RELATION} FROM rm_knowledge_relation_events WHERE relation_id=?1 AND scope_kind=?2 AND company_slug IS ?3 ORDER BY version")).map_err(sql)?;
    let rows = q
        .query_map(params![id, target.kind(), target.company()], relation_row)
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    let mut prior: Option<&Relation> = None;
    for r in &rows {
        r.validate()?;
        if r.version != prior.map_or(1, |r| r.version + 1)
            || r.prior_event_hash.as_deref() != prior.map(|p| p.event_hash.as_str())
            || r.event_type
                != if prior.is_none() {
                    "created"
                } else {
                    "superseded"
                }
        {
            return Err(sql("relation chain"));
        }
        prior = Some(r);
    }
    Ok(rows)
}
pub fn append_relation(
    tx: &Transaction<'_>,
    input: &RelationInput,
    generated_id: &str,
    expected: Option<(u64, &str)>,
    ctx: &Context<'_>,
) -> Result<Value, Error> {
    writer(tx)?;
    let id = input.relation_id.as_deref().unwrap_or(generated_id);
    let rows = relation_history(tx, &input.scope, id)?;
    let prior = rows.last();
    match (expected, prior) {
        (None, None) => {}
        (Some((version, hash)), Some(p)) if p.version == version && p.event_hash == hash => {}
        _ => return Err(conflict()),
    };
    endpoint(tx, &input.scope, &input.subject)?;
    endpoint(tx, &input.scope, &input.object)?;
    let r = domain::relation(input, id, prior, ctx.actor, ctx.principal, ctx.now)?;
    tx.execute("INSERT INTO rm_knowledge_relation_events(relation_id,version,event_type,scope_kind,company_slug,relation_type,subject_kind,subject_ref,object_kind,object_ref,provenance_kind,provenance_ref,effective_from,effective_to_exclusive,prior_event_hash,event_hash,actor,principal,created_at)VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19)",params![r.relation_id,i64::try_from(r.version).map_err(sql)?,r.event_type,r.scope.kind(),r.scope.company(),r.r#type,r.subject.kind,r.subject.r#ref,r.object.kind,r.object.r#ref,r.provenance.kind,r.provenance.r#ref,r.effective_from,r.effective_to_exclusive,r.prior_event_hash,r.event_hash,r.actor,r.principal,r.created_at]).map_err(sql)?;
    r.output()
}
pub fn relations(db: &Connection, target: &Scope, at: Option<&str>) -> Result<Vec<Value>, Error> {
    target.validate()?;
    let at = at.map(domain_time).transpose()?;
    let mut q=db.prepare(&format!("SELECT {RELATION} FROM rm_current_knowledge_relations WHERE scope_kind=?1 AND company_slug IS ?2 ORDER BY relation_id")).map_err(sql)?;
    q.query_map(params![target.kind(), target.company()], relation_row)
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?
        .iter()
        .filter(|r| {
            domain::visible(
                &r.effective_from,
                r.effective_to_exclusive.as_deref(),
                at.as_deref(),
            )
        })
        .map(|r| {
            r.validate()?;
            r.output()
        })
        .collect()
}
pub fn backlinks(
    db: &Connection,
    target: &Scope,
    page_id: &str,
    at: Option<&str>,
) -> Result<Vec<Value>, Error> {
    if !domain::identifier(page_id) {
        return Err(sql("page id"));
    }
    let mut out = vec![];
    for mut r in relations(db, target, at)? {
        let direction = if r["subject"]["kind"] == "page" && r["subject"]["ref"] == page_id {
            Some(("outgoing", "forward", "object"))
        } else if r["object"]["kind"] == "page" && r["object"]["ref"] == page_id {
            Some(("incoming", "inverse", "subject"))
        } else {
            None
        };
        if let Some((d, l, o)) = direction {
            r["direction"] = json!(d);
            r["label"] = r["labels"][l].clone();
            r["other"] = r[o].clone();
            out.push(r);
        }
    }
    Ok(out)
}
pub fn metric(
    db: &Connection,
    id: &str,
    target: &Scope,
    at: Option<&str>,
    limit: usize,
) -> Result<Value, Error> {
    if !(1..=100).contains(&limit)
        || !matches!(id, "knowledge-page-count" | "knowledge-relation-count")
    {
        return Err(sql("metric limit"));
    }
    let pages = pages(db, target, at)?;
    let relations = relations(db, target, at)?;
    let rows = if id == "knowledge-page-count" {
        pages
    } else {
        relations
    };
    let drilldown = rows
        .iter()
        .take(limit)
        .map(|r| {
            if id == "knowledge-page-count" {
                json!({"pageId":r["pageId"],"slug":r["slug"],"provenance":r["provenance"]})
            } else {
                json!({"relationId":r["relationId"],"type":r["type"],"provenance":r["provenance"]})
            }
        })
        .collect::<Vec<_>>();
    Ok(
        json!({"id":id,"catalogueVersion":1,"scope":target,"asOf":at.map(domain_time).transpose()?,"value":if rows.is_empty(){Value::Null}else{json!(rows.len())},"dataState":if rows.is_empty(){"missing"}else{"available"},"drilldown":drilldown}),
    )
}
/// Full history verifier for source-free recovery; historical attribution never restores grants.
pub fn validate(db: &Connection, companies: Option<&[String]>) -> Result<(), Error> {
    for (table, key) in [
        ("rm_knowledge_page_events", "page_id"),
        ("rm_knowledge_relation_events", "relation_id"),
    ] {
        let mut q = db
            .prepare(&format!(
                "SELECT DISTINCT {key},scope_kind,company_slug FROM {table}"
            ))
            .map_err(sql)?;
        let ids = q
            .query_map([], |r| {
                Ok((r.get::<_, String>(0)?, scope(r.get(1)?, r.get(2)?)?))
            })
            .map_err(sql)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql)?;
        for (id, target) in ids {
            if companies
                .is_some_and(|c| target.company().is_some_and(|s| !c.iter().any(|c| c == s)))
            {
                return Err(sql("unregistered knowledge company"));
            }
            if key == "page_id" {
                page_history(db, &target, &id)?;
            } else {
                for r in relation_history(db, &target, &id)? {
                    endpoint(db, &target, &r.subject)?;
                    endpoint(db, &target, &r.object)?;
                }
            }
        }
    }
    Ok(())
}
#[cfg(test)]
pub(crate) mod tests;
