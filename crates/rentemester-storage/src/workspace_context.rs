//! Own display context on a held control writer. Never a later company action grant.
use rentemester_engine::Error;
use rusqlite::Transaction;
use serde::Serialize;
use std::collections::{HashMap, HashSet};

/// Caller-observed registered company metadata; HTTP never supplies this catalog.
pub struct RegisteredCompany {
    pub slug: String,
    pub name: String,
    pub archived: bool,
    pub purpose: rentemester_engine::workspace_manifest::Purpose,
}
impl RegisteredCompany {
    pub fn active(&self) -> bool { !self.archived && self.purpose == rentemester_engine::workspace_manifest::Purpose::Live }
}
/// Trusted runtime observation only, not physical manifest/lifetime admission.
pub struct Catalog {
    pub workspace: String,
    pub companies: Vec<RegisteredCompany>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct User {
    id: String,
    email: String,
    email_verified: bool,
    two_factor_enabled: bool,
}
#[derive(Serialize)]
struct Company<'a> {
    slug: &'a str,
    name: &'a str,
    role: &'a str,
    archived: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Context<'a> {
    ok: bool,
    user: User,
    workspace_role: String,
    companies: Vec<Company<'a>>,
}
/// Bounded serialized display result; cannot be constructed from HTTP or used as authority.
pub struct WorkspaceContext(String);
impl WorkspaceContext {
    pub fn as_json(&self) -> &str {
        &self.0
    }
}
fn invalid(_: impl std::fmt::Display) -> Error {
    Error::new("invalid_control", "workspace context refused")
}
fn text(s: &str, max: usize) -> bool {
    !s.trim().is_empty() && s.len() <= max && !s.chars().any(char::is_control)
}
pub fn validate_catalog(catalog: &Catalog) -> Result<(), Error> {
    let mut seen = HashSet::new();
    if !text(&catalog.workspace, 1024) || catalog.companies.len() > 1000 {
        return Err(invalid("catalog bound"));
    }
    for c in &catalog.companies {
        if !text(&c.slug, 1024)
            || !c.slug.as_bytes()[0].is_ascii_alphanumeric()
            || !c
                .slug
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            || !text(&c.name, 1024)
            || !seen.insert(&c.slug)
        {
            return Err(invalid("catalog entry"));
        }
    }
    Ok(())
}
/// Requires current same-workspace cookie verification by caller and held canonical control writer.
/// Catalog must remain admitted for this snapshot. No filesystem, refresh, audit or SQL writes.
pub fn current(
    tx: &Transaction<'_>,
    token: &str,
    now: i64,
    catalog: &Catalog,
) -> Result<WorkspaceContext, Error> {
    validate_catalog(catalog)?;
    let current = crate::cookie_session::resolve(
        tx,
        token,
        now,
        crate::cookie_session::SessionRequirement::Current,
    )?;
    crate::session_control::authorize(tx, &current)?;
    let facts =
        crate::control_access::observe_human(tx, &current.subject, &current.session_id, "")?;
    let workspace_role = facts.workspace_role.ok_or_else(|| invalid("access"))?;
    let email: String = tx
        .query_row(
            "SELECT email FROM main.user WHERE id=?1 COLLATE BINARY",
            [&current.subject],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    project(
        tx,
        current.subject,
        workspace_role,
        email,
        true,
        true,
        catalog,
    )
}
/// Internal shared projection: caller must hold canonical writer and authenticate current human/service.
pub(crate) fn project(
    tx: &Transaction<'_>,
    subject: String,
    workspace_role: String,
    email: String,
    email_verified: bool,
    two_factor_enabled: bool,
    catalog: &Catalog,
) -> Result<WorkspaceContext, Error> {
    if !text(&subject, 4096) || !text(&email, 4096) {
        return Err(invalid("user"));
    }
    let mut q=tx.prepare("SELECT e.company_slug,e.event_type,e.company_role FROM main.rm_company_membership_events e JOIN (SELECT company_slug,MAX(id) latest_id FROM main.rm_company_membership_events WHERE user_id=?1 COLLATE BINARY GROUP BY company_slug) latest ON latest.latest_id=e.id WHERE e.user_id=?1 COLLATE BINARY").map_err(invalid)?;
    let mut rows = q.query([&subject]).map_err(invalid)?;
    let mut roles = HashMap::new();
    while let Some(row) = rows.next().map_err(invalid)? {
        // Bound history cardinality before building the result, including dangling grants.
        if roles.len() >= 1000 {
            return Err(invalid("membership bound"));
        }
        let slug: String = row.get(0).map_err(invalid)?;
        let event: String = row.get(1).map_err(invalid)?;
        let role: Option<String> = row.get(2).map_err(invalid)?;
        if !text(&slug, 1024) {
            return Err(invalid("membership slug"));
        }
        match (event.as_str(), role.as_deref()) {
            ("grant", Some("owner" | "bookkeeper" | "reviewer" | "reader")) => {}
            ("revoke", None) => {}
            _ => return Err(invalid("membership role")),
        }
        if roles.insert(slug, role).is_some() {
            return Err(invalid("duplicate membership"));
        }
    }
    let companies = catalog
        .companies
        .iter()
        .filter(|c| c.purpose == rentemester_engine::workspace_manifest::Purpose::Live)
        .filter_map(|c| {
            roles
                .get(&c.slug)
                .and_then(Option::as_deref)
                .map(|role| Company {
                    slug: &c.slug,
                    name: &c.name,
                    role,
                    archived: c.archived,
                })
        })
        .collect();
    let result = Context {
        ok: true,
        user: User {
            id: subject,
            email,
            email_verified,
            two_factor_enabled,
        },
        workspace_role,
        companies,
    };
    let json = serde_json::to_string(&result).map_err(invalid)?;
    if json.len() > 1_048_576 {
        return Err(invalid("result bound"));
    }
    Ok(WorkspaceContext(json))
}
