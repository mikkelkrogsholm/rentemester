//! Original scoped knowledge pages and relations, pure hash/provenance/render contracts.
use crate::{
    Error,
    company_knowledge::{iso, text},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;
fn bad(message: &str) -> Error {
    Error::new("knowledge_page_refused", message)
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum Scope {
    Workspace,
    Company {
        #[serde(rename = "companySlug")]
        company_slug: String,
    },
}
impl Scope {
    pub fn company(&self) -> Option<&str> {
        match self {
            Self::Workspace => None,
            Self::Company { company_slug } => Some(company_slug),
        }
    }
    pub fn kind(&self) -> &'static str {
        if self.company().is_some() {
            "company"
        } else {
            "workspace"
        }
    }
    pub fn validate(&self) -> Result<(), Error> {
        if self.company().is_some_and(|s| !slug(s)) {
            return Err(bad("Invalid knowledge company scope"));
        }
        Ok(())
    }
    pub fn target(&self) -> Value {
        json!({"kind":self.kind(),"company":self.company()})
    }
}
pub fn slug(v: &str) -> bool {
    v.is_ascii()
        && (1..=120).contains(&v.len())
        && v.as_bytes()[0].is_ascii_alphanumeric()
        && !v.as_bytes()[0].is_ascii_uppercase()
        && v.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
pub fn identifier(v: &str) -> bool {
    v.is_ascii()
        && (3..=80).contains(&v.len())
        && v.as_bytes()[0].is_ascii_lowercase()
        && v.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    pub kind: String,
    pub r#ref: String,
}
impl Provenance {
    pub fn normalized(&self) -> Result<Self, Error> {
        let r = text(&self.r#ref, 512)?;
        if ![
            "user",
            "party",
            "corporate_record",
            "company_knowledge",
            "external_snapshot",
        ]
        .contains(&self.kind.as_str())
            || self.kind == "external_snapshot" && !safe_url(&r)
        {
            return Err(bad("Invalid knowledge provenance"));
        }
        Ok(Self {
            kind: self.kind.clone(),
            r#ref: r,
        })
    }
}
fn decoded_url(input: &str) -> String {
    let mut s = input
        .trim()
        .chars()
        .filter(|c| !matches!(u32::from(*c), 0..=32 | 127))
        .collect::<String>();
    for _ in 0..3 {
        let mut out = vec![];
        let bytes = s.as_bytes();
        let mut i = 0;
        let mut valid = true;
        while i < bytes.len() {
            if bytes[i] == b'%' {
                if i + 2 >= bytes.len() {
                    valid = false;
                    break;
                }
                if let (Some(a), Some(b)) = (
                    char::from(bytes[i + 1]).to_digit(16),
                    char::from(bytes[i + 2]).to_digit(16),
                ) {
                    out.push((a * 16 + b) as u8);
                    i += 3
                } else {
                    valid = false;
                    break;
                }
            } else {
                out.push(bytes[i]);
                i += 1
            }
        }
        if !valid {
            break;
        }
        let Ok(next) = String::from_utf8(out) else {
            break;
        };
        if next == s {
            break;
        }
        s = next;
    }
    s.to_lowercase()
}
fn unsafe_url(s: &str) -> bool {
    let s = decoded_url(s);
    s.starts_with("javascript:") || s.starts_with("data:")
}
pub fn safe_url(s: &str) -> bool {
    !unsafe_url(s)
        && url::Url::parse(s).is_ok_and(|u| {
            matches!(u.scheme(), "http" | "https")
                && u.username().is_empty()
                && u.password().is_none()
        })
}
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Rendered {
    pub text: String,
    pub html: String,
    pub wikilinks: Vec<String>,
    pub external_links: Vec<String>,
}
pub fn render(markdown: &str) -> Result<Rendered, Error> {
    if unsafe_url(markdown) {
        return Err(bad("Unsafe knowledge URL"));
    }
    let external = regex::Regex::new(r"(?i)https?://[^\s)\]]+").expect("literal regex");
    let external_links = external
        .find_iter(markdown)
        .map(|m| m.as_str().to_owned())
        .collect::<BTreeSet<_>>();
    if external_links.iter().any(|u| !safe_url(u)) {
        return Err(bad("Unsafe knowledge URL"));
    }
    let text = escape(markdown);
    let html = external
        .replace_all(&text, |c: &regex::Captures<'_>| {
            let u = escape(c.get(0).unwrap().as_str());
            format!("<a href=\"{u}\" rel=\"noopener noreferrer\">{u}</a>")
        })
        .into_owned();
    let wiki = regex::Regex::new(r"\[\[([a-z0-9][a-z0-9-]{0,119})\]\]").expect("literal regex");
    let wikilinks = wiki
        .captures_iter(markdown)
        .map(|c| c[1].to_owned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    Ok(Rendered {
        text,
        html,
        wikilinks,
        external_links: external_links.into_iter().collect(),
    })
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Page {
    pub page_id: String,
    pub version: u64,
    pub event_type: String,
    pub scope: Scope,
    pub slug: String,
    pub title: String,
    pub body_markdown: String,
    pub provenance: Provenance,
    pub effective_from: String,
    pub effective_to_exclusive: Option<String>,
    pub prior_event_hash: Option<String>,
    pub event_hash: String,
    pub actor: String,
    pub principal: String,
    pub created_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PageInput {
    pub page_id: Option<String>,
    pub scope: Scope,
    pub slug: String,
    pub title: String,
    pub body_markdown: String,
    pub provenance: Provenance,
    pub effective_from: String,
    pub effective_to_exclusive: Option<String>,
}
pub fn interval(from: &str, to: Option<&str>) -> Result<(String, Option<String>), Error> {
    let f = iso(from)?;
    let t = to.map(iso).transpose()?;
    if t.as_ref().is_some_and(|t| t <= &f) {
        return Err(bad("Invalid knowledge interval"));
    }
    Ok((f, t))
}
pub fn visible(from: &str, to: Option<&str>, at: Option<&str>) -> bool {
    at.is_none_or(|at| from <= at && to.is_none_or(|to| at < to))
}
impl Page {
    pub fn hash(&self) -> Result<String, Error> {
        crate::tasks::payload_hash(
            &json!({"pageId":self.page_id,"version":self.version,"eventType":self.event_type,"target":self.scope.target(),"pageSlug":self.slug,"title":self.title,"body":self.body_markdown,"source":self.provenance,"from":self.effective_from,"to":self.effective_to_exclusive,"priorEventHash":self.prior_event_hash,"actor":self.actor,"principal":self.principal,"createdAt":self.created_at}),
        )
    }
    pub fn output(&self, body: bool) -> Result<Value, Error> {
        let mut v = json!({"pageId":self.page_id,"version":self.version,"eventType":self.event_type,"scope":self.scope,"slug":self.slug,"title":self.title,"rendered":render(&self.body_markdown)?,"provenance":self.provenance,"effectiveFrom":self.effective_from,"effectiveToExclusive":self.effective_to_exclusive,"priorEventHash":self.prior_event_hash,"eventHash":self.event_hash});
        if body {
            v["bodyMarkdown"] = json!(self.body_markdown)
        }
        Ok(v)
    }
    pub fn validate(&self) -> Result<(), Error> {
        self.scope.validate()?;
        if !identifier(&self.page_id)
            || !slug(&self.slug)
            || text(&self.title, 240)? != self.title
            || self.body_markdown.encode_utf16().count() > 20000
            || self.provenance.normalized()? != self.provenance
            || interval(&self.effective_from, self.effective_to_exclusive.as_deref())?
                != (
                    self.effective_from.clone(),
                    self.effective_to_exclusive.clone(),
                )
            || text(&self.actor, 160)? != self.actor
            || text(&self.principal, 160)? != self.principal
            || iso(&self.created_at)? != self.created_at
            || self.version == 0
            || !matches!(self.event_type.as_str(), "created" | "superseded")
            || self.event_hash != self.hash()?
        {
            return Err(bad("Knowledge page event differs"));
        }
        render(&self.body_markdown)?;
        Ok(())
    }
}
pub fn page(
    input: &PageInput,
    id: &str,
    prior: Option<&Page>,
    actor: &str,
    principal: &str,
    at: &str,
) -> Result<Page, Error> {
    input.scope.validate()?;
    let (from, to) = interval(
        &input.effective_from,
        input.effective_to_exclusive.as_deref(),
    )?;
    let page_id = input.page_id.as_deref().unwrap_or(id).to_owned();
    if prior.is_some_and(|p| p.page_id != page_id || p.scope != input.scope) {
        return Err(bad("Knowledge page unavailable in scope"));
    }
    let mut p = Page {
        page_id,
        version: prior.map_or(1, |p| p.version + 1),
        event_type: if prior.is_some() {
            "superseded"
        } else {
            "created"
        }
        .into(),
        scope: input.scope.clone(),
        slug: text(&input.slug, 120)?,
        title: text(&input.title, 240)?,
        body_markdown: input.body_markdown.clone(),
        provenance: input.provenance.normalized()?,
        effective_from: from,
        effective_to_exclusive: to,
        prior_event_hash: prior.map(|p| p.event_hash.clone()),
        event_hash: String::new(),
        actor: text(actor, 160)?,
        principal: text(principal, 160)?,
        created_at: iso(at)?,
    };
    p.event_hash = p.hash()?;
    p.validate()?;
    Ok(p)
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Endpoint {
    pub kind: String,
    pub r#ref: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Relation {
    pub relation_id: String,
    pub version: u64,
    pub event_type: String,
    pub scope: Scope,
    pub r#type: String,
    pub subject: Endpoint,
    pub object: Endpoint,
    pub provenance: Provenance,
    pub effective_from: String,
    pub effective_to_exclusive: Option<String>,
    pub prior_event_hash: Option<String>,
    pub event_hash: String,
    pub actor: String,
    pub principal: String,
    pub created_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RelationInput {
    pub relation_id: Option<String>,
    pub scope: Scope,
    pub r#type: String,
    pub subject: Endpoint,
    pub object: Endpoint,
    pub provenance: Provenance,
    pub effective_from: String,
    pub effective_to_exclusive: Option<String>,
}
pub fn labels(kind: &str) -> Result<Value, Error> {
    let (forward, inverse) = match kind {
        "commercial_counterparty" => ("buys from", "sells to"),
        "operational_dependency" => ("depends on", "supports"),
        "reference" => ("references", "referenced by"),
        _ => return Err(bad("Invalid knowledge relation")),
    };
    Ok(json!({"forward":forward,"inverse":inverse}))
}
impl Relation {
    pub fn hash(&self) -> Result<String, Error> {
        crate::tasks::payload_hash(
            &json!({"relationId":self.relation_id,"version":self.version,"eventType":self.event_type,"target":self.scope.target(),"type":self.r#type,"subject":self.subject,"object":self.object,"source":self.provenance,"from":self.effective_from,"to":self.effective_to_exclusive,"priorEventHash":self.prior_event_hash,"actor":self.actor,"principal":self.principal,"createdAt":self.created_at}),
        )
    }
    pub fn output(&self) -> Result<Value, Error> {
        Ok(
            json!({"relationId":self.relation_id,"version":self.version,"eventType":self.event_type,"scope":self.scope,"type":self.r#type,"subject":self.subject,"object":self.object,"labels":labels(&self.r#type)?,"provenance":self.provenance,"effectiveFrom":self.effective_from,"effectiveToExclusive":self.effective_to_exclusive,"priorEventHash":self.prior_event_hash,"eventHash":self.event_hash}),
        )
    }
    pub fn validate(&self) -> Result<(), Error> {
        self.scope.validate()?;
        labels(&self.r#type)?;
        let ends = [&self.subject, &self.object];
        if !identifier(&self.relation_id)
            || self.subject == self.object
            || ends.iter().any(|e| {
                !matches!(e.kind.as_str(), "page" | "party" | "company") || !identifier(&e.r#ref)
            })
            || self.provenance.normalized()? != self.provenance
            || interval(&self.effective_from, self.effective_to_exclusive.as_deref())?
                != (
                    self.effective_from.clone(),
                    self.effective_to_exclusive.clone(),
                )
            || self.version == 0
            || !matches!(self.event_type.as_str(), "created" | "superseded")
            || text(&self.actor, 160)? != self.actor
            || text(&self.principal, 160)? != self.principal
            || iso(&self.created_at)? != self.created_at
            || self.hash()? != self.event_hash
        {
            return Err(bad("Knowledge relation event differs"));
        }
        Ok(())
    }
}
pub fn relation(
    input: &RelationInput,
    id: &str,
    prior: Option<&Relation>,
    actor: &str,
    principal: &str,
    at: &str,
) -> Result<Relation, Error> {
    let (from, to) = interval(
        &input.effective_from,
        input.effective_to_exclusive.as_deref(),
    )?;
    let relation_id = input.relation_id.as_deref().unwrap_or(id).to_owned();
    if prior.is_some_and(|p| p.relation_id != relation_id || p.scope != input.scope) {
        return Err(bad("Knowledge relation unavailable in scope"));
    }
    let mut r = Relation {
        relation_id,
        version: prior.map_or(1, |r| r.version + 1),
        event_type: if prior.is_some() {
            "superseded"
        } else {
            "created"
        }
        .into(),
        scope: input.scope.clone(),
        r#type: input.r#type.clone(),
        subject: input.subject.clone(),
        object: input.object.clone(),
        provenance: input.provenance.normalized()?,
        effective_from: from,
        effective_to_exclusive: to,
        prior_event_hash: prior.map(|r| r.event_hash.clone()),
        event_hash: String::new(),
        actor: text(actor, 160)?,
        principal: text(principal, 160)?,
        created_at: iso(at)?,
    };
    r.event_hash = r.hash()?;
    r.validate()?;
    Ok(r)
}
#[cfg(test)]
mod tests;
