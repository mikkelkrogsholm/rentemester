//! Explicit reviewed reporting charts over authoritative V1 groups. No ledger writes.
use crate::{
    Error,
    intercompany::{Interval, Manifest},
    money::Money,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use unicode_normalization::UnicodeNormalization;
pub fn refused() -> Error {
    Error::new(
        "consolidation_refused",
        "consolidation source or operation refused",
    )
}
fn text(s: &str, max: usize) -> Result<String, Error> {
    let s: String = crate::company_knowledge::trim(s).nfc().collect();
    if s.is_empty() || s.encode_utf16().count() > max {
        return Err(refused());
    }
    Ok(s)
}
fn id(s: &str) -> Result<String, Error> {
    let s = text(s, 64)?;
    if !crate::intercompany::identifier(&s) {
        return Err(refused());
    }
    Ok(s)
}
fn section(s: &str) -> bool {
    ["asset", "liability", "equity", "income", "expense"].contains(&s)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Line {
    pub id: String,
    pub label: String,
    pub section: String,
    pub display_order: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Mapping {
    pub id: String,
    pub company_slug: String,
    pub account_no: String,
    pub reporting_line_id: String,
    #[serde(flatten)]
    pub interval: Interval,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Profile {
    pub version: u32,
    pub id: String,
    pub group_id: String,
    pub currency: String,
    pub reporting_lines: Vec<Line>,
    pub account_mappings: Vec<Mapping>,
    pub evidence_refs: Vec<String>,
    #[serde(flatten)]
    pub interval: Interval,
}
impl Profile {
    pub fn normalize(mut self, manifest: &Manifest) -> Result<Self, Error> {
        if self.version != 1 {
            return Err(refused());
        }
        self.id = id(&self.id)?;
        self.group_id = id(&self.group_id)?;
        let group = manifest
            .groups
            .iter()
            .find(|g| g.id == self.group_id)
            .ok_or_else(refused)?;
        self.currency = text(&self.currency, 3)?.to_uppercase();
        if self.currency.len() != 3 || !self.currency.bytes().all(|b| b.is_ascii_uppercase()) {
            return Err(refused());
        }
        self.interval.valid_from = text(&self.interval.valid_from, 10)?;
        self.interval.valid_to_exclusive = self
            .interval
            .valid_to_exclusive
            .as_deref()
            .map(|s| text(s, 10))
            .transpose()?;
        if !self.interval.valid() {
            return Err(refused());
        }
        if !(5..=512).contains(&self.reporting_lines.len()) {
            return Err(refused());
        }
        for l in &mut self.reporting_lines {
            l.id = id(&l.id)?;
            l.label = text(&l.label, 160)?;
            l.section = text(&l.section, 16)?;
            l.role = l.role.as_deref().map(|s| text(s, 32)).transpose()?;
            if !section(&l.section)
                || l.display_order > 100000
                || l.role
                    .as_deref()
                    .is_some_and(|r| r != "current-result" || l.section != "equity")
            {
                return Err(refused());
            }
        }
        self.reporting_lines
            .sort_by(|a, b| a.display_order.cmp(&b.display_order).then(a.id.cmp(&b.id)));
        if self
            .reporting_lines
            .iter()
            .map(|l| &l.id)
            .collect::<BTreeSet<_>>()
            .len()
            != self.reporting_lines.len()
            || self
                .reporting_lines
                .iter()
                .filter(|l| l.role.as_deref() == Some("current-result"))
                .count()
                != 1
            || ["asset", "liability", "equity", "income", "expense"]
                .iter()
                .any(|s| !self.reporting_lines.iter().any(|l| &l.section == s))
        {
            return Err(refused());
        }
        if self.account_mappings.is_empty() || self.account_mappings.len() > 4096 {
            return Err(refused());
        }
        for m in &mut self.account_mappings {
            m.id = id(&m.id)?;
            m.company_slug = text(&m.company_slug, 256)?;
            m.account_no = text(&m.account_no, 64)?;
            m.reporting_line_id = id(&m.reporting_line_id)?;
            m.interval.valid_from = text(&m.interval.valid_from, 10)?;
            m.interval.valid_to_exclusive = m
                .interval
                .valid_to_exclusive
                .as_deref()
                .map(|s| text(s, 10))
                .transpose()?;
            if !m.account_no.as_bytes()[0].is_ascii_alphanumeric()
                || !m
                    .account_no
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
                || !m.interval.valid()
                || !self.interval.covers(&m.interval)
                || !group
                    .memberships
                    .iter()
                    .any(|g| g.company_slug == m.company_slug && g.interval.covers(&m.interval))
                || !self
                    .reporting_lines
                    .iter()
                    .any(|l| l.id == m.reporting_line_id && l.role.is_none())
            {
                return Err(refused());
            }
        }
        self.account_mappings.sort_by(|a, b| a.id.cmp(&b.id));
        if self
            .account_mappings
            .iter()
            .map(|m| &m.id)
            .collect::<BTreeSet<_>>()
            .len()
            != self.account_mappings.len()
        {
            return Err(refused());
        }
        for (i, a) in self.account_mappings.iter().enumerate() {
            if self.account_mappings[i + 1..].iter().any(|b| {
                a.company_slug == b.company_slug
                    && a.account_no == b.account_no
                    && a.interval.overlaps(&b.interval)
            }) {
                return Err(refused());
            }
        }
        if self.evidence_refs.is_empty() || self.evidence_refs.len() > 64 {
            return Err(refused());
        }
        self.evidence_refs = self
            .evidence_refs
            .iter()
            .map(|s| text(s, 256))
            .collect::<Result<_, _>>()?;
        self.evidence_refs
            .sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
        if self.evidence_refs.windows(2).any(|w| w[0] == w[1]) {
            return Err(refused());
        }
        Ok(self)
    }
    /// Declaration order matches the legacy JSON.stringify source contract.
    pub fn canonical(&self) -> Result<String, Error> {
        let s = serde_json::to_string(self).map_err(|_| refused())?;
        if s.len() > 524288 {
            return Err(refused());
        }
        Ok(s)
    }
    pub fn companies(&self, m: &Manifest) -> Result<BTreeSet<String>, Error> {
        Ok(m.groups
            .iter()
            .find(|g| g.id == self.group_id)
            .ok_or_else(refused)?
            .memberships
            .iter()
            .map(|m| m.company_slug.clone())
            .collect())
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub account_no: String,
    pub account_type: String,
    pub active: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountValue {
    pub account_no: String,
    pub account_type: String,
    pub amount: Money,
    pub source_hash: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ledger {
    pub company_slug: String,
    pub currency: String,
    pub ledger_head_hash: Option<String>,
    pub entry_count: u64,
    pub accounts: Vec<Account>,
    pub pnl: Vec<AccountValue>,
    pub cumulative_result: Vec<AccountValue>,
    pub balance: Vec<AccountValue>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Applied {
    pub elimination_id: String,
    pub payload_hash: String,
    pub payload: crate::eliminations::BalancePayload,
}
fn compatible(kind: &str, section: &str) -> bool {
    kind == section || (kind == "vat" && (section == "asset" || section == "liability"))
}
fn negative(n: &Money) -> Money {
    Money::default().subtract(n)
}
fn push(blockers: &mut Vec<String>, s: &str) {
    if !blockers.iter().any(|v| v == s) {
        blockers.push(s.into());
    }
}
/// Authority/visibility and physical originals are checked by the application/host.
/// All amount arithmetic uses exact shared Money; only real applied evidence is accepted.
#[allow(clippy::too_many_arguments)] // Explicit source report inputs; no implicit financial context.
pub fn calculate(
    profile: &Profile,
    profile_hash: &str,
    from: &str,
    as_of: &str,
    ledgers: &[Ledger],
    applied: &[Applied],
    reconciliation: &[Value],
    mut blockers: Vec<String>,
    source_hash: impl Fn(&str) -> String,
) -> Result<Value, Error> {
    let lines: BTreeMap<_, _> = profile
        .reporting_lines
        .iter()
        .map(|l| (l.id.as_str(), l))
        .collect();
    let mappings: BTreeMap<_, _> = profile
        .account_mappings
        .iter()
        .filter(|m| m.interval.active(as_of))
        .map(|m| ((m.company_slug.as_str(), m.account_no.as_str()), m))
        .collect();
    if blockers.is_empty() {
        for m in mappings.values() {
            let account = ledgers
                .iter()
                .find(|l| l.company_slug == m.company_slug)
                .and_then(|l| l.accounts.iter().find(|a| a.account_no == m.account_no));
            if let Some(a) = account {
                if !a.active {
                    push(
                        &mut blockers,
                        "reporting profile references a missing or inactive source account",
                    );
                } else if !compatible(
                    &a.account_type,
                    &lines[m.reporting_line_id.as_str()].section,
                ) {
                    push(
                        &mut blockers,
                        "reporting mapping is incompatible with source account type",
                    );
                }
            } else {
                push(
                    &mut blockers,
                    "reporting profile references a missing or inactive source account",
                );
            }
        }
    }
    let mut by_line: BTreeMap<String, BTreeMap<String, (Money, Vec<String>)>> = BTreeMap::new();
    if blockers.is_empty() {
        for ledger in ledgers {
            for value in ledger.pnl.iter().chain(&ledger.balance) {
                if value.amount == Money::default() {
                    continue;
                }
                let Some(m) =
                    mappings.get(&(ledger.company_slug.as_str(), value.account_no.as_str()))
                else {
                    push(
                        &mut blockers,
                        "one or more non-zero company accounts lack an active reporting mapping",
                    );
                    continue;
                };
                let line = lines[m.reporting_line_id.as_str()];
                if !compatible(&value.account_type, &line.section) {
                    push(
                        &mut blockers,
                        "reporting mapping is incompatible with source account type",
                    );
                    continue;
                }
                let natural = if line.section == "asset" || line.section == "expense" {
                    value.amount.clone()
                } else {
                    negative(&value.amount)
                };
                let row = by_line
                    .entry(line.id.clone())
                    .or_default()
                    .entry(ledger.company_slug.clone())
                    .or_default();
                row.0.add(&natural);
                row.1.push(value.source_hash.clone());
            }
        }
        let result_line = profile
            .reporting_lines
            .iter()
            .find(|l| l.role.as_deref() == Some("current-result"))
            .ok_or_else(refused)?;
        for ledger in ledgers {
            let mut result = Money::default();
            for v in &ledger.cumulative_result {
                let mapping = mappings.get(&(ledger.company_slug.as_str(), v.account_no.as_str()));
                if v.amount != Money::default() && mapping.is_none() {
                    push(
                        &mut blockers,
                        "one or more cumulative result accounts lack an active reporting mapping",
                    );
                    continue;
                }
                if mapping.is_some() {
                    result.add(&negative(&v.amount));
                }
            }
            let provenance=source_hash(&json!({"companySlug":ledger.company_slug,"period":"inception-to-asOf","asOf":as_of,"sourceHashes":ledger.cumulative_result.iter().map(|v|&v.source_hash).collect::<Vec<_>>(),"integrityScope":"native_exact_consolidation_v1"}).to_string());
            by_line
                .entry(result_line.id.clone())
                .or_default()
                .insert(ledger.company_slug.clone(), (result, vec![provenance]));
        }
    }
    let mut raw = vec![];
    let mut totals: BTreeMap<&str, Money> = BTreeMap::new();
    for l in &profile.reporting_lines {
        if let Some(rows) = by_line.get(&l.id) {
            let mut total = Money::default();
            let companies: Vec<_> = rows
                .iter()
                .map(|(company, (amount, hashes))| {
                    total.add(amount);
                    let mut hashes = hashes.clone();
                    hashes.sort();
                    json!({"companySlug":company,"amount":amount,"sourceHashes":hashes})
                })
                .collect();
            totals.insert(&l.id, total.clone());
            raw.push(json!({"lineId":l.id,"byCompany":companies,"total":total}));
        }
    }
    let mut adjustments: BTreeMap<&str, Money> = BTreeMap::new();
    let mut used = vec![];
    if blockers.is_empty() {
        for e in applied {
            let p = serde_json::to_value(&e.payload).map_err(|_| refused())?;
            let normalized = e.payload.adjustments()?;
            if p["groupId"] != profile.group_id {
                continue;
            }
            let fresh = reconciliation
                .iter()
                .find(|r| r["mappingId"] == p["mappingId"]);
            let mut matches = false;
            if let Some(f) = fresh {
                matches = f["status"] == "matched" && f["mappingHash"] == p["mappingHash"];
                for (index, side) in ["left", "right"].into_iter().enumerate() {
                    let a = &f[side];
                    let b = &p[side];
                    matches = matches
                        && serde_json::to_value(&normalized[index].position)
                            .map_err(|_| refused())?
                            == a["position"]
                        && a.is_object()
                        && a["sourceSnapshot"]["selectionHash"] == b["selectionHash"]
                        && a["sourceSnapshot"]["ledgerHeadHash"] == b["ledgerHeadHash"]
                        && a["sourceSnapshot"]["entryCount"] == b["entryCount"]
                        && a["balance"]
                            .as_str()
                            .and_then(|s| Money::parse_canonical(s).ok())
                            == Some(Money::parse(&format!(
                                "{}e-2",
                                p["amountOre"].as_str().ok_or_else(refused)?
                            ))?);
                }
            }
            if !matches {
                push(
                    &mut blockers,
                    "applied elimination source snapshot or orientation changed; reverse/replan and apply a new elimination",
                );
                continue;
            }
            let mut resolved = vec![];
            for side in ["left", "right"] {
                let s = &p[side];
                let company = s["companySlug"].as_str().ok_or_else(refused)?;
                let accounts = s["accountNos"].as_array().ok_or_else(refused)?;
                let resolved_lines: Option<BTreeSet<_>> = accounts
                    .iter()
                    .map(|a| {
                        mappings
                            .get(&(company, a.as_str()?))
                            .map(|m| m.reporting_line_id.as_str())
                    })
                    .collect();
                let Some(resolved_lines) = resolved_lines.filter(|v| v.len() == 1) else {
                    push(
                        &mut blockers,
                        "applied elimination accounts do not resolve to one reporting line per side",
                    );
                    continue;
                };
                resolved.push(*resolved_lines.iter().next().unwrap());
            }
            if resolved.len() != 2 {
                continue;
            }
            if resolved.iter().zip(&normalized).any(|(line, side)| {
                lines[*line].section
                    != if side.position == crate::intercompany::Position::Receivable {
                        "asset"
                    } else {
                        "liability"
                    }
            }) {
                push(
                    &mut blockers,
                    "balance elimination requires the actual receivable asset and payable liability reporting lines",
                );
                continue;
            }
            let amount = negative(&Money::parse(&format!(
                "{}e-2",
                p["amountOre"].as_str().ok_or_else(refused)?
            ))?);
            for line in &resolved {
                adjustments.entry(line).or_default().add(&amount);
            }
            used.push(json!({"eliminationId":e.elimination_id,"payloadHash":e.payload_hash,"adjustments":[{"lineId":resolved[0],"amount":amount},{"lineId":resolved[1],"amount":amount}]}));
        }
    }
    let mut figures = Value::Null;
    if blockers.is_empty() {
        let mut sections: BTreeMap<&str, Money> = BTreeMap::new();
        let rows:Vec<_>=profile.reporting_lines.iter().map(|l|{let raw=totals.get(l.id.as_str()).cloned().unwrap_or_default();let adjustment=adjustments.get(l.id.as_str()).cloned().unwrap_or_default();let mut total=raw.clone();total.add(&adjustment);sections.entry(&l.section).or_default().add(&total);json!({"lineId":l.id,"label":l.label,"section":l.section,"rawCompanySum":raw,"eliminationAdjustment":adjustment,"consolidatedAmount":total})}).collect();
        let assets = sections.get("asset").cloned().unwrap_or_default();
        let mut other = sections.get("liability").cloned().unwrap_or_default();
        other.add(&sections.get("equity").cloned().unwrap_or_default());
        if assets != other {
            push(
                &mut blockers,
                "consolidated balance does not satisfy assets equals liabilities plus equity",
            );
        } else {
            figures = json!(rows);
        }
    }
    Ok(
        json!({"scope":"consolidated-report","profileId":profile.id,"profileHash":profile_hash,"groupId":profile.group_id,"period":{"from":from,"to":as_of},"currency":profile.currency,"status":if blockers.is_empty(){"ready"}else{"blocked"},"blockers":blockers,"rawCompanySums":raw,"appliedEliminations":used,"consolidatedFigures":figures,"sourceSnapshots":ledgers.iter().map(|l|json!({"companySlug":l.company_slug,"ledgerHeadHash":l.ledger_head_hash,"entryCount":l.entry_count,"integrityScope":"native_period_sources_and_physical_originals"})).collect::<Vec<_>>()}),
    )
}
#[cfg(test)]
mod tests;
