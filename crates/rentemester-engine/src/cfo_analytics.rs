//! Exact, source-linked CFO query projection. No SQL, authority inference or consolidation math.
use crate::{Error, fiscal_year::Date, money::Money};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
#[cfg(test)]
mod tests;
pub fn refused(message: impl Into<String>) -> Error {
    Error::new("cfo_analytics_refused", message)
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Query {
    pub scope: String,
    pub from: String,
    pub to: String,
    pub company_slugs: Vec<String>,
    pub group_profile_id: Option<String>,
    pub account: Option<String>,
    pub party: Option<String>,
    pub dimension: Option<String>,
    pub currency: Option<String>,
    pub cursor: Option<String>,
    pub aggregate: String,
    pub limit: usize,
}
impl Query {
    pub fn validate(&self) -> Result<(), Error> {
        Date::parse(&self.from).map_err(|_| refused("invalid from date"))?;
        Date::parse(&self.to).map_err(|_| refused("invalid to date"))?;
        if self.from > self.to
            || !["company", "portfolio", "group"].contains(&self.scope.as_str())
            || !["none", "sum"].contains(&self.aggregate.as_str())
            || !(1..=200).contains(&self.limit)
            || self.company_slugs.len() > 1000
        {
            return Err(refused("invalid CFO selectors"));
        }
        if self.scope == "company" && self.company_slugs.len() != 1
            || self.scope == "group" && self.group_profile_id.is_none()
            || self.scope != "group" && self.group_profile_id.is_some()
        {
            return Err(refused("scope selectors disagree"));
        }
        for value in self.company_slugs.iter().chain(
            [
                &self.group_profile_id,
                &self.account,
                &self.party,
                &self.dimension,
                &self.currency,
            ]
            .into_iter()
            .flatten(),
        ) {
            if value.trim() != value
                || value.is_empty()
                || value.len() > 256
                || value.chars().any(|c| c.is_control() || c == '|')
            {
                return Err(refused("invalid text selector"));
            }
        }
        if self
            .currency
            .as_ref()
            .is_some_and(|s| s.len() != 3 || !s.bytes().all(|b| b.is_ascii_alphabetic()))
        {
            return Err(refused("invalid currency"));
        }
        if let Some(cursor) = &self.cursor {
            decode_cursor(cursor)?;
        }
        Ok(())
    }
}
/// Produced by current control authorization, never deserialized from user input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    pub companies: Vec<String>,
    pub partial: bool,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dimension {
    pub dimension_id: String,
    pub member_id: String,
    pub amount_minor: i64,
    pub currency: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PartyLinkProvenance {
    pub link_id: i64,
    pub role: String,
    pub evidence_kind: String,
    pub plan_hash: String,
    pub linked_at: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub company_slug: String,
    pub source_type: String,
    pub source_hash: String,
    pub source_id: String,
    pub journal_entry_id: Option<i64>,
    pub journal_entry_no: Option<String>,
    pub document_id: Option<i64>,
    pub document_hash: Option<String>,
    pub party_id: Option<String>,
    pub party_name: Option<String>,
    pub document_party_name: Option<String>,
    pub party_link_provenance: Option<PartyLinkProvenance>,
    pub account_no: String,
    pub account_name: Option<String>,
    pub transaction_date: String,
    pub currency: String,
    pub amount: Money,
    pub text: Option<String>,
    pub dimensions: Vec<Dimension>,
}
#[derive(Clone, Debug)]
pub struct CompanyRows {
    pub company_slug: String,
    pub rows: Vec<Row>,
    pub evidence: Value,
}
fn key(row: &Row) -> String {
    [
        row.transaction_date.as_str(),
        row.source_type.as_str(),
        row.company_slug.as_str(),
        row.source_id.as_str(),
        row.account_no.as_str(),
    ]
    .join("|")
}
const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
fn encode(raw: &[u8]) -> String {
    let mut out = String::new();
    for bytes in raw.chunks(3) {
        let n = (u32::from(bytes[0]) << 16)
            | (u32::from(*bytes.get(1).unwrap_or(&0)) << 8)
            | u32::from(*bytes.get(2).unwrap_or(&0));
        for i in 0..bytes.len() + 1 {
            out.push(ALPHABET[((n >> (18 - i * 6)) & 63) as usize] as char);
        }
    }
    out
}
fn decode_cursor(s: &str) -> Result<String, Error> {
    if s.is_empty() || s.len() > 4096 || s.len() % 4 == 1 {
        return Err(refused("invalid cursor"));
    }
    let mut out = vec![];
    let mut bits = 0u32;
    let mut count = 0;
    for b in s.bytes() {
        let n = ALPHABET
            .iter()
            .position(|v| *v == b)
            .ok_or_else(|| refused("invalid cursor"))?;
        bits = (bits << 6) | n as u32;
        count += 6;
        if count >= 8 {
            count -= 8;
            out.push((bits >> count) as u8);
            bits &= (1 << count) - 1;
        }
    }
    if bits != 0 || encode(&out) != s {
        return Err(refused("invalid cursor"));
    }
    let text = String::from_utf8(out).map_err(|_| refused("invalid cursor"))?;
    let fields: Vec<_> = text.split('|').collect();
    if fields.len() != 5
        || Date::parse(fields[0]).is_err()
        || !["archive", "ledger"].contains(&fields[1])
        || fields
            .iter()
            .any(|s| s.is_empty() || s.chars().any(char::is_control))
    {
        return Err(refused("invalid cursor"));
    }
    Ok(text)
}
/// Filters and full-universe sums happen before the page is selected.
pub fn render(
    query: &Query,
    selection: &Selection,
    sources: &[CompanyRows],
) -> Result<Value, Error> {
    query.validate()?;
    if query.scope == "group" {
        return Err(refused("group must delegate approved consolidated report"));
    }
    let selected: BTreeSet<_> = selection.companies.iter().collect();
    if selected.len() != selection.companies.len() || sources.len() != selected.len() {
        return Err(refused("source selection differs"));
    }
    let mut seen = BTreeSet::new();
    let mut rows = vec![];
    let mut evidence = vec![];
    for source in sources {
        if !selected.contains(&source.company_slug) || !seen.insert(&source.company_slug) {
            return Err(refused("source selection differs"));
        }
        evidence.push(source.evidence.clone());
        for row in &source.rows {
            if row.company_slug != source.company_slug {
                return Err(refused("source company differs"));
            }
            if row.transaction_date < query.from || row.transaction_date > query.to {
                continue;
            }
            if query.account.as_ref().is_some_and(|s| s != &row.account_no)
                || query
                    .party
                    .as_ref()
                    .is_some_and(|s| Some(s) != row.party_id.as_ref())
                || query
                    .currency
                    .as_ref()
                    .is_some_and(|s| !s.eq_ignore_ascii_case(&row.currency))
                || query.dimension.as_ref().is_some_and(|s| {
                    !row.dimensions.iter().any(|d| {
                        s == &d.dimension_id || s == &format!("{}:{}", d.dimension_id, d.member_id)
                    })
                })
            {
                continue;
            }
            rows.push(row);
        }
    }
    rows.sort_by_key(|r| key(r));
    let mut identity = BTreeSet::new();
    let mut sums: BTreeMap<&str, Money> = BTreeMap::new();
    let mut hashes = BTreeSet::new();
    let mut freshness: BTreeMap<(&str, &str), &str> = BTreeMap::new();
    for row in &rows {
        if !identity.insert(key(row)) {
            return Err(refused("duplicate source row identity"));
        }
        sums.entry(&row.currency).or_default().add(&row.amount);
        hashes.insert(&row.source_hash);
        let entry = freshness
            .entry((&row.company_slug, &row.source_type))
            .or_default();
        if row.transaction_date.as_str() > *entry {
            *entry = &row.transaction_date;
        }
    }
    let after = query.cursor.as_deref().map(decode_cursor).transpose()?;
    let matching: Vec<_> = rows
        .iter()
        .filter(|r| after.as_ref().is_none_or(|a| key(r) > *a))
        .collect();
    let page: Vec<_> = matching.iter().take(query.limit).collect();
    let next = if matching.len() > page.len() {
        page.last().map(|r| encode(key(r).as_bytes()))
    } else {
        None
    };
    let allow = query.scope != "portfolio" || query.aggregate == "sum" && !selection.partial;
    let mut reconciliation = json!({"rowCount":rows.len(),"sourceHashes":hashes,"method":"sum(all matching rows.amount) by verified base currency; every row links to its canonical journal/archive source"});
    if allow {
        reconciliation["amountByCurrency"] = json!(sums);
    } else {
        reconciliation["omitted"] = json!("portfolio aggregate not requested or access is partial");
    }
    let limitations = if query.scope == "portfolio" {
        let mut notes = vec![
            "Portfolio is a juxtaposed, non-consolidated view. No eliminations or inferred currency conversion are applied.",
        ];
        if selection.partial {
            notes.push("One or more requested companies are hidden; aggregate totals are intentionally omitted.");
        }
        notes
    } else {
        vec![
            "Rows with no visible recorded party link expose partyId:null; no identity is inferred.",
        ]
    };
    Ok(
        json!({"ok":true,"schemaVersion":"rentemester-cfo-analytics-v1","scope":query.scope,"status":if selection.partial{"incomplete"}else{"ready"},"asOf":query.to,"from":query.from,"to":query.to,"companies":selection.companies,"partial":selection.partial,"mode":if query.scope=="portfolio"{"juxtaposed-non-consolidated"}else{"legal-company"},"aggregate":query.aggregate,"limitations":limitations,"rows":page,"page":{"limit":query.limit,"nextCursor":next},"freshness":freshness.into_iter().map(|((company,source),date)|json!({"source":source,"companySlug":company,"latestTransactionDate":date})).collect::<Vec<_>>(),"evidenceCompleteness":evidence,"reconciliation":reconciliation}),
    )
}
