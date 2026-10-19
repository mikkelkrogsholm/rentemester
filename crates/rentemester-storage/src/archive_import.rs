//! Closed-history reference data, immutable source attestation and exact numeric storage.
use base64::{Engine as _, engine::general_purpose::STANDARD};
use rentemester_engine::{
    Error,
    archive_import::{self as policy, ArchiveDecimal, CsvFile, ParsedArchive, Year},
    cfo_analytics::Row,
};
use rusqlite::{Connection, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
mod history;
mod numeric;
#[cfg(test)]
mod tests;
pub use history::{
    OriginalRef, VerifiedArchive, cfo_rows, originals, read, validate_history, verify_original,
};
const EVENT: &str = "native_closed_history_archive_import";
fn sql(_: impl std::fmt::Display) -> Error {
    policy::refused("archive storage or retained source proof unavailable")
}
fn sha(s: impl AsRef<[u8]>) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(s.as_ref()))
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EntryFacts {
    pub path: String,
    pub sha256: String,
    pub size_bytes: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceFacts {
    pub path: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub entries: Vec<EntryFacts>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Package {
    format: String,
    currency: String,
    files: Vec<Encoded>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Encoded {
    path: String,
    content_base64: String,
}
pub fn decode_source(bytes: &[u8]) -> Result<(SourceFacts, ParsedArchive), Error> {
    if bytes.is_empty() || bytes.len() > 25 * 1024 * 1024 {
        return Err(policy::refused("archive original size exceeds bounds"));
    }
    let package: Package = serde_json::from_slice(bytes)
        .map_err(|_| policy::refused("invalid versioned archive source package"))?;
    if package.format != policy::PACKAGE || package.files.len() > 64 {
        return Err(policy::refused("unsupported archive source package"));
    }
    let mut files = vec![];
    let mut entries = vec![];
    for file in package.files {
        policy::canonical_path(&file.path)?;
        let decoded = STANDARD
            .decode(file.content_base64.as_bytes())
            .map_err(|_| policy::refused("invalid source base64"))?;
        if STANDARD.encode(&decoded) != file.content_base64 {
            return Err(policy::refused("noncanonical source base64"));
        }
        entries.push(EntryFacts {
            path: file.path.clone(),
            sha256: sha(&decoded),
            size_bytes: decoded.len() as u64,
        });
        files.push(CsvFile {
            path: file.path,
            bytes: decoded,
        });
    }
    let parsed = policy::parse(&files, &package.currency)?;
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    let hash = sha(bytes);
    Ok((
        SourceFacts {
            path: format!("documents/originals/{hash}.json"),
            sha256: hash,
            size_bytes: bytes.len() as u64,
            entries,
        },
        parsed,
    ))
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Context {
    ledger_uuid: String,
    currency: String,
    fiscal_year_start_month: i64,
    fiscal_year_label_strategy: String,
    archive_state_hash: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Binding {
    format: String,
    source: SourceFacts,
    archive: ParsedArchive,
    context: Context,
}
#[derive(Clone)]
pub struct Prepared {
    binding: Binding,
    hash: String,
    retained: BTreeMap<u16, i64>,
}
impl Prepared {
    pub fn review(&self) -> Value {
        json!({"format":policy::FORMAT,"status":"ready","planHash":self.hash,"source":{"sha256":self.binding.source.sha256,"sizeBytes":self.binding.source.size_bytes},"declaredCurrency":self.binding.archive.declared_currency,"currencyDeclaration":"operator","excludedLatestYear":self.binding.archive.excluded_latest_year,"years":self.binding.archive.years.iter().map(|y|json!({"fiscalYear":y.fiscal_year,"postingCount":y.postings.len(),"balanceCount":y.balances.len(),"hasBalances":y.has_balances,"status":if self.retained.contains_key(&y.fiscal_year){"retained"}else{"new"}})).collect::<Vec<_>>(),"limitations":[]})
    }
}
fn context(db: &Connection) -> Result<Context, Error> {
    let mut context=db.query_row("SELECT l.ledger_uuid,c.currency,c.fiscal_year_start_month,c.fiscal_year_label_strategy FROM ledger_identity l CROSS JOIN companies c WHERE l.id=1 AND c.id=1",[],|r|Ok(Context{ledger_uuid:r.get(0)?,currency:r.get(1)?,fiscal_year_start_month:r.get(2)?,fiscal_year_label_strategy:r.get(3)?,archive_state_hash:String::new()})).map_err(sql)?;
    let mut q = db
        .prepare("SELECT id,message FROM audit_log WHERE event_type=? ORDER BY id")
        .map_err(sql)?;
    let rows = q
        .query_map([EVENT], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    context.archive_state_hash = sha(serde_json::to_vec(&rows).map_err(sql)?);
    Ok(context)
}
pub fn prepare(
    tx: &Transaction<'_>,
    source: &SourceFacts,
    parsed: &ParsedArchive,
) -> Result<Prepared, Error> {
    crate::journal_preview::require_current(tx)?;
    let checked = history::checked(tx)?;
    history::source_shape(source, parsed)?;
    for year in &parsed.years {
        numeric::check_year(year)?;
    }
    let mut retained = BTreeMap::new();
    for year in &parsed.years {
        if let Some(old) = checked.years.get(&year.fiscal_year) {
            if old.year != *year || old.currency != parsed.declared_currency {
                return Err(policy::refused(
                    "existing archive year differs; no overwrite or adoption",
                ));
            }
            retained.insert(year.fiscal_year, old.id);
        }
    }
    if let Some(old) = checked
        .imports
        .iter()
        .find(|i| i.event.binding.source == *source && i.event.binding.archive == *parsed)
    {
        return Ok(Prepared {
            binding: old.event.binding.clone(),
            hash: old.event.plan_hash.clone(),
            retained,
        });
    }
    let context = context(tx)?;
    if context.currency != parsed.declared_currency {
        return Err(policy::refused(
            "declared historical currency must match current functional currency",
        ));
    }
    let binding = Binding {
        format: policy::FORMAT.into(),
        source: source.clone(),
        archive: parsed.clone(),
        context,
    };
    let hash = sha(serde_json::to_vec(&binding).map_err(sql)?);
    Ok(Prepared {
        binding,
        hash,
        retained,
    })
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Apply {
    pub plan_hash: String,
    pub idempotency_key: String,
    pub confirm: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Installed {
    fiscal_year: u16,
    id: i64,
    imported_at: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Attestation {
    version: u32,
    binding: Binding,
    plan_hash: String,
    installed: Vec<Installed>,
    subject: String,
    at: String,
}
fn receipt(id: i64, event: &Attestation) -> Value {
    json!({"ok":true,"archiveImport":{"importId":id,"sourceSha256":event.binding.source.sha256,"planHash":event.plan_hash,"excludedLatestYear":event.binding.archive.excluded_latest_year,"declaredCurrency":event.binding.archive.declared_currency,"years":event.binding.archive.years.iter().map(|y|json!({"fiscalYear":y.fiscal_year,"postingCount":y.postings.len(),"balanceCount":y.balances.len()})).collect::<Vec<_>>()}})
}
/// Domain-only; the authorized application wraps this in the existing durable keyed receipt protocol.
pub fn apply(
    tx: &Transaction<'_>,
    prepared: &Prepared,
    input: &Apply,
    subject: &str,
    now_ms: i64,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    crate::idempotency::validate_key(&input.idempotency_key)?;
    if !input.confirm
        || subject.is_empty()
        || subject.chars().any(char::is_control)
        || input.plan_hash != prepared.hash
    {
        return Err(policy::refused(
            "current explicit reviewed archive confirmation required",
        ));
    }
    let fresh = prepare(tx, &prepared.binding.source, &prepared.binding.archive)?;
    if fresh.hash != prepared.hash {
        return Err(policy::refused("archive plan is stale"));
    }
    let checked = history::checked(tx)?;
    if let Some(old) = checked.imports.iter().find(|i| {
        i.event.binding.source == prepared.binding.source
            && i.event.binding.archive == prepared.binding.archive
    }) {
        return Ok(receipt(old.id, &old.event));
    }
    let at = rentemester_engine::company_knowledge::timestamp(now_ms)?;
    let mut installed = vec![];
    for year in &prepared.binding.archive.years {
        let id = if let Some(id) = fresh.retained.get(&year.fiscal_year) {
            *id
        } else {
            let id:i64=tx.query_row("INSERT INTO import_archive_years(source_system,fiscal_year,posting_count,balance_count,imported_at)VALUES('dinero',?,?,?,?) RETURNING id",params![year.fiscal_year,year.postings.len()as i64,year.balances.len()as i64,at],|r|r.get(0)).map_err(sql)?;
            for p in &year.postings {
                tx.execute("INSERT INTO import_archive_postings(archive_year_id,line_no,account_no,account_name,transaction_date,voucher,voucher_type,text,vat_type,amount,running_balance)VALUES(?,?,?,?,?,?,?,?,?,?,?)",params![id,p.line_no,p.account_no,p.account_name,p.transaction_date,p.voucher,p.voucher_type,p.text,p.vat_type,numeric::value(&p.amount)?,p.running_balance.as_ref().map(numeric::value).transpose()?]).map_err(sql)?;
            }
            for b in &year.balances {
                tx.execute("INSERT INTO import_archive_balances(archive_year_id,line_no,account_no,account_name,amount)VALUES(?,?,?,?,?)",params![id,b.line_no,b.account_no,b.account_name,numeric::value(&b.amount)?]).map_err(sql)?;
            }
            id
        };
        let imported_at = tx
            .query_row(
                "SELECT imported_at FROM import_archive_years WHERE id=?",
                [id],
                |r| r.get(0),
            )
            .map_err(sql)?;
        installed.push(Installed {
            fiscal_year: year.fiscal_year,
            id,
            imported_at,
        });
    }
    let event = Attestation {
        version: 1,
        binding: fresh.binding,
        plan_hash: fresh.hash,
        installed,
        subject: subject.into(),
        at,
    };
    let body = serde_json::to_string(&event).map_err(sql)?;
    let id:i64=tx.query_row("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor,created_at)VALUES(?,'closed_history_archive',?,?,?,?) RETURNING id",params![EVENT,event.binding.source.sha256,body,format!("user:{subject}"),event.at],|r|r.get(0)).map_err(sql)?;
    history::checked(tx)?;
    Ok(receipt(id, &event))
}
