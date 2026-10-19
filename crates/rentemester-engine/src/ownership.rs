//! Effective direct facts only: no inferred ultimate owners or journal effects.
use crate::Error;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub fn refused(message: impl Into<String>) -> Error {
    Error::new("ownership_refused", message)
}
fn bounded(s: &str, max: usize) -> Result<String, Error> {
    crate::company_knowledge::text(s, max).map_err(|_| refused("bounded nonblank text required"))
}
pub fn date(s: &str) -> Result<String, Error> {
    let s = bounded(s, 10)?;
    if !crate::calendar::valid_iso_date(&s) {
        return Err(refused("ISO date required"));
    }
    Ok(s)
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum Endpoint {
    #[serde(rename = "company")]
    Company {
        #[serde(rename = "companySlug")]
        company_slug: String,
    },
    #[serde(rename = "party")]
    Party {
        #[serde(rename = "partyId")]
        party_id: String,
    },
}
impl Endpoint {
    pub fn key(&self) -> String {
        match self {
            Self::Company { company_slug } => format!("company:{company_slug}"),
            Self::Party { party_id } => format!("party:{party_id}"),
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Interval {
    pub min: u16,
    pub max: u16,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Fact {
    pub owner: Endpoint,
    pub owned_company_slug: String,
    pub valid_from: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_to_exclusive: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub economic_basis_points: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub economic_interval_basis_points: Option<Interval>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voting_basis_points: Option<u16>,
    pub control_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub share_class: Option<String>,
    pub jurisdiction: String,
    pub evidence_refs: Vec<String>,
}
impl Fact {
    pub fn normalize(mut self) -> Result<Self, Error> {
        self.owner = match self.owner {
            Endpoint::Company { company_slug } => Endpoint::Company {
                company_slug: bounded(&company_slug, 120)?,
            },
            Endpoint::Party { party_id } => Endpoint::Party {
                party_id: bounded(&party_id, 64)?,
            },
        };
        self.owned_company_slug = bounded(&self.owned_company_slug, 120)?;
        if matches!(&self.owner,Endpoint::Company{company_slug} if company_slug==&self.owned_company_slug)
        {
            return Err(refused("ownership cannot be self-referential"));
        }
        self.valid_from = date(&self.valid_from)?;
        self.valid_to_exclusive = self.valid_to_exclusive.as_deref().map(date).transpose()?;
        if self
            .valid_to_exclusive
            .as_ref()
            .is_some_and(|d| d <= &self.valid_from)
        {
            return Err(refused("nonempty effective interval required"));
        }
        match (
            self.economic_basis_points,
            &self.economic_interval_basis_points,
        ) {
            (Some(n), None) if n <= 10000 => {}
            (None, Some(i)) if i.min <= i.max && i.max <= 10000 => {}
            _ => {
                return Err(refused(
                    "exactly one bounded economic value or interval required",
                ));
            }
        }
        if self.voting_basis_points.is_some_and(|n| n > 10000)
            || !["equity", "voting", "board", "agreement", "other"]
                .contains(&self.control_type.as_str())
        {
            return Err(refused(
                "bounded voting value and supported control type required",
            ));
        }
        self.share_class = self
            .share_class
            .filter(|s| !s.is_empty())
            .as_deref()
            .map(|s| bounded(s, 80))
            .transpose()?;
        self.jurisdiction = bounded(&self.jurisdiction, 2)?.to_uppercase();
        if self.jurisdiction.len() != 2
            || !self.jurisdiction.bytes().all(|b| b.is_ascii_uppercase())
        {
            return Err(refused("two letter jurisdiction required"));
        }
        self.evidence_refs = self
            .evidence_refs
            .iter()
            .map(|s| bounded(s, 256))
            .collect::<Result<BTreeSet<_>, _>>()?
            .into_iter()
            .collect();
        // JavaScript's source canonicalization sorts references by UTF-16 code units.
        self.evidence_refs
            .sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
        if self.evidence_refs.is_empty() || self.evidence_refs.len() > 32 {
            return Err(refused("1..32 evidence references required"));
        }
        Ok(self)
    }
    pub fn active(&self, at: &str) -> bool {
        self.valid_from.as_str() <= at
            && self
                .valid_to_exclusive
                .as_deref()
                .is_none_or(|end| at < end)
    }
    pub fn with_end(mut self, end: Option<&str>) -> Self {
        if let Some(end) = end
            && self
                .valid_to_exclusive
                .as_deref()
                .is_none_or(|old| end < old)
        {
            self.valid_to_exclusive = Some(end.into());
        }
        self
    }
    pub fn companies(&self) -> BTreeSet<String> {
        let mut s = BTreeSet::from([self.owned_company_slug.clone()]);
        if let Endpoint::Company { company_slug } = &self.owner {
            s.insert(company_slug.clone());
        }
        s
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Ending {
    pub fact_hash: String,
    pub effective_to_exclusive: String,
    pub successor_fact_hash: String,
}
fn overlaps(a: &Fact, b: &Fact) -> bool {
    a.valid_from.as_str() < b.valid_to_exclusive.as_deref().unwrap_or("9999-12-31")
        && b.valid_from.as_str() < a.valid_to_exclusive.as_deref().unwrap_or("9999-12-31")
}
/// Approved rows already carry their prior append-only endings; identities retain original hashes.
pub fn integrity(
    approved: &[(String, Fact)],
    incoming: &[(String, Fact)],
) -> Result<Vec<Ending>, Error> {
    if incoming
        .iter()
        .map(|(h, _)| h)
        .collect::<BTreeSet<_>>()
        .len()
        != incoming.len()
    {
        return Err(refused("duplicate canonical facts"));
    }
    let mut endings = vec![];
    for (h, old) in approved {
        for (next_hash, next) in incoming {
            if old.owner == next.owner
                && old.owned_company_slug == next.owned_company_slug
                && overlaps(old, next)
            {
                if next.valid_from <= old.valid_from {
                    return Err(refused("direct ownership facts must not overlap"));
                }
                endings.push(Ending {
                    fact_hash: h.clone(),
                    effective_to_exclusive: next.valid_from.clone(),
                    successor_fact_hash: next_hash.clone(),
                });
            }
        }
    }
    endings.sort_by(|a, b| {
        a.fact_hash
            .cmp(&b.fact_hash)
            .then(a.effective_to_exclusive.cmp(&b.effective_to_exclusive))
    });
    // Source takes the last planned ending for a hash; contradictory same-endpoint
    // incoming rows are rejected below before the plan can be saved.
    let end_by_hash: BTreeMap<_, _> = endings
        .iter()
        .map(|e| (e.fact_hash.as_str(), e.effective_to_exclusive.as_str()))
        .collect();
    let mut all: Vec<Fact> = approved
        .iter()
        .map(|(h, f)| f.clone().with_end(end_by_hash.get(h.as_str()).copied()))
        .collect();
    for (_, f) in incoming {
        if !all.contains(f) {
            all.push(f.clone());
        }
    }
    for (i, a) in all.iter().enumerate() {
        for b in &all[i + 1..] {
            if a.owner == b.owner && a.owned_company_slug == b.owned_company_slug && overlaps(a, b)
            {
                return Err(refused("direct ownership facts must not overlap"));
            }
        }
    }
    let boundaries: BTreeSet<_> = all
        .iter()
        .flat_map(|f| std::iter::once(f.valid_from.as_str()).chain(f.valid_to_exclusive.as_deref()))
        .collect();
    for at in boundaries {
        let active: Vec<_> = all.iter().filter(|f| f.active(at)).collect();
        let mut totals: BTreeMap<&str, u64> = BTreeMap::new();
        let mut edges: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for f in active {
            *totals.entry(&f.owned_company_slug).or_default() += u64::from(
                f.economic_basis_points
                    .unwrap_or_else(|| f.economic_interval_basis_points.as_ref().unwrap().min),
            );
            if let Endpoint::Company { company_slug } = &f.owner {
                edges
                    .entry(company_slug)
                    .or_default()
                    .push(&f.owned_company_slug);
            }
        }
        if totals.values().any(|n| *n > 10000) {
            return Err(refused(
                "combined direct economic floors exceed 10000 basis points",
            ));
        }
        let mut done = BTreeSet::new();
        for node in edges.keys() {
            if cycle(node, &edges, &mut BTreeSet::new(), &mut done) {
                return Err(refused("effective company cycle"));
            }
        }
    }
    Ok(endings)
}
fn cycle<'a>(
    node: &'a str,
    edges: &BTreeMap<&'a str, Vec<&'a str>>,
    visiting: &mut BTreeSet<&'a str>,
    done: &mut BTreeSet<&'a str>,
) -> bool {
    if visiting.contains(node) {
        return true;
    }
    if done.contains(node) {
        return false;
    }
    visiting.insert(node);
    if edges.get(node).is_some_and(|children| {
        children
            .iter()
            .any(|child| cycle(child, edges, visiting, done))
    }) {
        return true;
    }
    visiting.remove(node);
    done.insert(node);
    false
}
#[cfg(test)]
mod tests;
