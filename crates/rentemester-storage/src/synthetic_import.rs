//! Original CSV entrypoint adapted to the common chart and opening engine.
//! Exact UTF-8 source is retained inside the immutable opening event and signed database backup.
use crate::{
    archive_import::VerifiedArchive, company_setup::source_chart, journal_post::Actor,
    opening_balance,
};
use rentemester_engine::{Error, fiscal_year::Date, synthetic_import as parser};
use rusqlite::Transaction;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Input {
    pub source_system: String,
    pub content: String,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Apply {
    pub input: Input,
    pub plan_hash: String,
    pub idempotency_key: String,
    pub confirm: bool,
}
fn refused(message: impl Into<String>) -> Error {
    Error::new("synthetic_import_refused", message)
}
fn source(input: &Input) -> Result<parser::Source, Error> {
    if input.source_system != parser::SYSTEM
        || input.content.is_empty()
        || input.content.len() > 48 * 1024
    {
        return Err(refused(
            "explicit synthetic-csv UTF-8 source required, maximum48KiB",
        ));
    }
    parser::parse(input.content.as_bytes())
}
fn opening(parsed: &parser::Source) -> rentemester_engine::opening_balance::Input {
    let mut lines: Vec<_> = parsed
        .opening_balances
        .iter()
        .map(|l| l.opening_line())
        .collect();
    for line in &mut lines {
        line.text = parsed
            .chart_of_accounts
            .iter()
            .find(|a| a.account_no == line.account_no)
            .map(|a| a.name.clone());
    }
    rentemester_engine::opening_balance::Input {
        cut_over_date: parsed.cut_over_date,
        lines,
        note: Some(format!(
            "Import fra synthetic-csv{}",
            parsed
                .note
                .as_deref()
                .filter(|n| !n.trim().is_empty())
                .map(|n| format!(" — {}", n.trim()))
                .unwrap_or_default()
        )),
        source_archive_import_ids: vec![],
    }
}
pub(crate) fn verify_opening(
    input: &Input,
    retained: &rentemester_engine::opening_balance::Input,
) -> Result<(), Error> {
    let expected = opening(&source(input)?);
    if serde_json::to_value(expected).map_err(|_| refused("source unavailable"))?
        != serde_json::to_value(retained).map_err(|_| refused("opening unavailable"))?
    {
        return Err(refused("retained CSV and opening interpretation differ"));
    }
    Ok(())
}
fn chart(parsed: &parser::Source) -> Vec<source_chart::SourceAccount> {
    parsed
        .chart_of_accounts
        .iter()
        .map(|a| source_chart::SourceAccount {
            account_no: a.account_no.as_str().into(),
            name: a.name.clone(),
            kind: None,
            normal_balance: None,
            default_vat_code: None,
        })
        .collect()
}
fn hash(value: &Value) -> Result<String, Error> {
    use sha2::{Digest, Sha256};
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(value).map_err(|_| refused("plan unavailable"))?)
    ))
}
/// Read-only review. Unclassified missing accounts need explicit normal chart setup first.
pub fn prepare(
    tx: &Transaction<'_>,
    input: &Input,
    verified: &[VerifiedArchive],
    latest: Date,
) -> Result<Value, Error> {
    let parsed = source(input)?;
    let changes = source_chart::prepare(tx, &chart(&parsed))?;
    let opening =
        opening_balance::prepare_synthetic(tx, &opening(&parsed), verified, latest, input)?
            .review();
    let mut blockers: Vec<Value> = opening["blockers"].as_array().cloned().unwrap_or_default();
    blockers.extend(
        changes
            .iter()
            .filter_map(|c| c.conflict.as_ref().map(|m| json!(m))),
    );
    let binding =
        json!({"contract":"synthetic-csv/1","input":input,"chart":changes,"opening":opening});
    Ok(
        json!({"ok":true,"plan":{"planHash":hash(&binding)?,"sourceSystem":parser::SYSTEM,"chart":changes,"opening":opening,"blockers":blockers,"status":if blockers.is_empty(){"ready"}else{"blocked"}},"errors":[]}),
    )
}
/// Caller supplies current authority, durable idempotency and the outer atomic transaction.
pub fn apply(
    tx: &Transaction<'_>,
    input: &Apply,
    verified: &[VerifiedArchive],
    latest: Date,
    actor: Actor<'_>,
    now_ms: i64,
) -> Result<Value, Error> {
    let review = prepare(tx, &input.input, verified, latest)?;
    if !input.confirm
        || review["plan"]["planHash"] != input.plan_hash
        || review["plan"]["status"] != "ready"
    {
        return Err(refused("confirmed unchanged ready source plan required"));
    }
    let parsed = source(&input.input)?;
    let accounts = chart(&parsed);
    let changes = source_chart::prepare(tx, &accounts)?;
    let at = rentemester_engine::company_knowledge::timestamp(now_ms)?;
    source_chart::apply(
        tx,
        &accounts,
        &changes,
        actor
            .subject
            .strip_prefix("user:")
            .filter(|s| !s.is_empty())
            .ok_or_else(|| refused("current human actor required"))?,
        &at,
    )?;
    // The reviewed outer hash binds the original chart and requested changes. Rebind the same
    // opening policy to the now-reconciled chart; never trust a caller-supplied posting/hash.
    let inner = opening(&parsed);
    let plan =
        opening_balance::prepare_synthetic(tx, &inner, verified, latest, &input.input)?.review();
    let command = opening_balance::Apply {
        input: inner,
        plan_hash: plan["planHash"]
            .as_str()
            .ok_or_else(|| refused("opening plan unavailable"))?
            .into(),
        idempotency_key: input.idempotency_key.clone(),
        confirm: true,
    };
    let mut result = opening_balance::apply_synthetic(
        tx,
        &command,
        verified,
        latest,
        actor,
        now_ms,
        &input.input,
    )?;
    result["sourceSystem"] = json!(parser::SYSTEM);
    result["sourcePlanHash"] = json!(input.plan_hash);
    result["chart"] = serde_json::to_value(changes).map_err(|_| refused("chart unavailable"))?;
    Ok(result)
}

#[cfg(test)]
mod tests;
