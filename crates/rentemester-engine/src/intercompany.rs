//! Explicit V1 group memberships and reciprocal mappings. Never derives legal ownership.
use crate::{Error, fiscal_year::Date};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use unicode_normalization::UnicodeNormalization;
pub fn refused() -> Error {
    Error::new(
        "intercompany_refused",
        "intercompany source or operation refused",
    )
}
pub fn identifier(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.as_bytes()[0].is_ascii_lowercase()
        && s.bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}
fn normalized(s: &str) -> String {
    crate::company_knowledge::trim(s).nfc().collect()
}
// The source V1 safe-name/evidence contract rejects Unicode Cf as well as Cc.
fn format_control(c: char) -> bool {
    matches!(c as u32, 173 | 1536..=1541 | 1564 | 1757 | 1807 | 2192..=2193 | 2274 | 6158 | 8203..=8207 | 8234..=8238 | 8288..=8292 | 8294..=8303 | 65279 | 65529..=65531 | 69821 | 69837 | 78896..=78911 | 113824..=113827 | 119155..=119162 | 917505 | 917536..=917631)
}
fn text(s: &str, n: usize) -> bool {
    !s.trim().is_empty()
        && s == s.trim()
        && s.encode_utf16().count() <= n
        && !s.chars().any(char::is_control)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Interval {
    pub valid_from: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_to_exclusive: Option<String>,
}
impl Interval {
    fn normalize(&mut self) {
        self.valid_from = normalized(&self.valid_from);
        self.valid_to_exclusive = self.valid_to_exclusive.as_ref().map(|s| normalized(s));
    }
    pub fn valid(&self) -> bool {
        Date::parse(&self.valid_from).is_ok()
            && self
                .valid_to_exclusive
                .as_ref()
                .is_none_or(|d| Date::parse(d).is_ok() && d > &self.valid_from)
    }
    pub fn active(&self, d: &str) -> bool {
        self.valid_from.as_str() <= d
            && self
                .valid_to_exclusive
                .as_ref()
                .is_none_or(|e| d < e.as_str())
    }
    pub fn covers(&self, b: &Self) -> bool {
        self.valid_from <= b.valid_from
            && self
                .valid_to_exclusive
                .as_ref()
                .is_none_or(|e| b.valid_to_exclusive.as_ref().is_some_and(|v| v <= e))
    }
    pub fn overlaps(&self, b: &Self) -> bool {
        self.valid_to_exclusive
            .as_ref()
            .is_none_or(|e| &b.valid_from < e)
            && b.valid_to_exclusive
                .as_ref()
                .is_none_or(|e| &self.valid_from < e)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Membership {
    pub id: String,
    pub company_slug: String,
    #[serde(flatten)]
    pub interval: Interval,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Ownership {
    pub id: String,
    pub parent_company_slug: String,
    pub child_company_slug: String,
    pub basis_points: u32,
    pub evidence_refs: Vec<String>,
    #[serde(flatten)]
    pub interval: Interval,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Group {
    pub id: String,
    pub name: String,
    pub memberships: Vec<Membership>,
    pub ownership: Vec<Ownership>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub version: u8,
    pub groups: Vec<Group>,
}
fn refs(v: &mut [String]) -> bool {
    for item in v.iter_mut() {
        *item = normalized(item);
    }
    v.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
    !v.is_empty()
        && v.len() <= 32
        && v.iter().all(|s| text(s, 256))
        && v.windows(2).all(|a| a[0] != a[1])
}
impl Manifest {
    pub fn companies(&self) -> BTreeSet<String> {
        self.groups
            .iter()
            .flat_map(|g| g.memberships.iter().map(|m| m.company_slug.clone()))
            .collect()
    }
    pub fn normalize(mut self, registered: &BTreeSet<String>) -> Result<Self, Error> {
        if self.version != 1 || self.groups.is_empty() || self.groups.len() > 128 {
            return Err(refused());
        }
        for g in &mut self.groups {
            g.id = normalized(&g.id);
            g.name = normalized(&g.name);
            for m in &mut g.memberships {
                m.id = normalized(&m.id);
                m.company_slug = normalized(&m.company_slug);
                m.interval.normalize();
            }
            for o in &mut g.ownership {
                o.id = normalized(&o.id);
                o.parent_company_slug = normalized(&o.parent_company_slug);
                o.child_company_slug = normalized(&o.child_company_slug);
                o.interval.normalize();
            }
        }
        self.groups.sort_by(|a, b| a.id.cmp(&b.id));
        let mut ids = BTreeSet::new();
        for g in &mut self.groups {
            if !identifier(&g.id)
                || !ids.insert(g.id.clone())
                || !text(&g.name, 160)
                || g.name.chars().any(format_control)
                || g.memberships.is_empty()
                || g.memberships.len() > 1024
                || g.ownership.len() > 1024
            {
                return Err(refused());
            }
            g.memberships.sort_by(|a, b| a.id.cmp(&b.id));
            g.ownership.sort_by(|a, b| a.id.cmp(&b.id));
            let mut mids = BTreeSet::new();
            let mut oids = BTreeSet::new();
            for m in &g.memberships {
                if !identifier(&m.id)
                    || !mids.insert(&m.id)
                    || !registered.contains(&m.company_slug)
                    || !m.interval.valid()
                {
                    return Err(refused());
                }
            }
            for (i, m) in g.memberships.iter().enumerate() {
                if g.memberships[i + 1..]
                    .iter()
                    .any(|b| m.company_slug == b.company_slug && m.interval.overlaps(&b.interval))
                {
                    return Err(refused());
                }
            }
            for o in &mut g.ownership {
                if !identifier(&o.id)
                    || !oids.insert(o.id.clone())
                    || o.parent_company_slug == o.child_company_slug
                    || !(1..=10000).contains(&o.basis_points)
                    || !o.interval.valid()
                    || !refs(&mut o.evidence_refs)
                    || o.evidence_refs
                        .iter()
                        .any(|s| s.chars().any(format_control))
                {
                    return Err(refused());
                }
                for s in [&o.parent_company_slug, &o.child_company_slug] {
                    if !g
                        .memberships
                        .iter()
                        .any(|m| &m.company_slug == s && m.interval.covers(&o.interval))
                    {
                        return Err(refused());
                    }
                }
            }
            for (i, o) in g.ownership.iter().enumerate() {
                if g.ownership[i + 1..].iter().any(|b| {
                    o.parent_company_slug == b.parent_company_slug
                        && o.child_company_slug == b.child_company_slug
                        && o.interval.overlaps(&b.interval)
                }) {
                    return Err(refused());
                }
            }
            let dates: BTreeSet<_> = g
                .ownership
                .iter()
                .flat_map(|o| {
                    std::iter::once(o.interval.valid_from.as_str())
                        .chain(o.interval.valid_to_exclusive.as_deref())
                })
                .collect();
            for d in dates {
                let mut totals: BTreeMap<&str, u32> = BTreeMap::new();
                let mut edges: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
                for o in g.ownership.iter().filter(|o| o.interval.active(d)) {
                    let n = totals.entry(&o.child_company_slug).or_default();
                    *n += o.basis_points;
                    if *n > 10000 {
                        return Err(refused());
                    }
                    edges
                        .entry(&o.parent_company_slug)
                        .or_default()
                        .push(&o.child_company_slug);
                }
                fn visit<'a>(
                    s: &'a str,
                    e: &BTreeMap<&'a str, Vec<&'a str>>,
                    seen: &mut BTreeSet<&'a str>,
                    done: &mut BTreeSet<&'a str>,
                ) -> bool {
                    if done.contains(s) {
                        return true;
                    }
                    if !seen.insert(s) {
                        return false;
                    }
                    if e.get(s)
                        .is_some_and(|v| v.iter().any(|v| !visit(v, e, seen, done)))
                    {
                        return false;
                    }
                    seen.remove(s);
                    done.insert(s);
                    true
                }
                let mut done = BTreeSet::new();
                for s in edges.keys() {
                    if !visit(s, &edges, &mut BTreeSet::new(), &mut done) {
                        return Err(refused());
                    }
                }
            }
        }
        Ok(self)
    }
    pub fn canonical(&self) -> Result<String, Error> {
        serde_json::to_string(self).map_err(|_| refused())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Position {
    Receivable,
    Payable,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Mapping {
    pub id: String,
    pub group_id: String,
    pub left_company_slug: String,
    pub right_company_slug: String,
    pub left_account_nos: Vec<String>,
    pub right_account_nos: Vec<String>,
    pub left_position: Position,
    pub right_position: Position,
    pub evidence_refs: Vec<String>,
    #[serde(flatten)]
    pub interval: Interval,
}
impl Mapping {
    pub fn companies(&self) -> BTreeSet<String> {
        BTreeSet::from([
            self.left_company_slug.clone(),
            self.right_company_slug.clone(),
        ])
    }
    pub fn normalize(mut self, m: &Manifest) -> Result<Self, Error> {
        self.id = normalized(&self.id);
        self.group_id = normalized(&self.group_id);
        self.left_company_slug = normalized(&self.left_company_slug);
        self.right_company_slug = normalized(&self.right_company_slug);
        self.interval.normalize();
        let g = m
            .groups
            .iter()
            .find(|g| g.id == self.group_id)
            .ok_or_else(refused)?;
        if !identifier(&self.id)
            || self.left_company_slug == self.right_company_slug
            || self.left_position == self.right_position
            || !self.interval.valid()
            || !refs(&mut self.evidence_refs)
        {
            return Err(refused());
        }
        for s in [&self.left_company_slug, &self.right_company_slug] {
            if !g
                .memberships
                .iter()
                .any(|m| &m.company_slug == s && m.interval.covers(&self.interval))
            {
                return Err(refused());
            }
        }
        for v in [&mut self.left_account_nos, &mut self.right_account_nos] {
            for item in v.iter_mut() {
                *item = normalized(item);
            }
            v.sort();
            if v.is_empty()
                || v.len() > 128
                || v.windows(2).any(|a| a[0] == a[1])
                || v.iter().any(|s| {
                    s.is_empty()
                        || s.len() > 64
                        || !s.as_bytes()[0].is_ascii_alphanumeric()
                        || !s
                            .bytes()
                            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
                })
            {
                return Err(refused());
            }
        }
        Ok(self)
    }
    pub fn canonical(&self) -> Result<String, Error> {
        serde_json::to_string(self).map_err(|_| refused())
    }
    pub fn overlaps(&self, b: &Self) -> bool {
        self.interval.overlaps(&b.interval)
            && [
                (&self.left_company_slug, &self.left_account_nos),
                (&self.right_company_slug, &self.right_account_nos),
            ]
            .iter()
            .any(|(s, accounts)| {
                let other = if *s == &b.left_company_slug {
                    &b.left_account_nos
                } else if *s == &b.right_company_slug {
                    &b.right_account_nos
                } else {
                    return false;
                };
                accounts.iter().any(|a| other.contains(a))
            })
    }
}
