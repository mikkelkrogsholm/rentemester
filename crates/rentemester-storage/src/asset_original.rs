//! Source asset ABI over the shared native register and admitted journal writer.
//! Caller owns authorization, physical original hash verification and transaction:
//! every Err must roll back; only the two documented exception refusals return Ok(false).
//! Registration intentionally requires unallocated capitalisation, unlike the TS metadata-only check.
use crate::{assets, idempotency::PrincipalKind, invalid};
use rentemester_engine::{
    Error,
    currency::Currency,
    document::money,
    fiscal_year::Date,
    identifiers::DocumentId,
    manual_journal::ManualJournal,
    money::Money,
    sales_invoice::{refused, wire},
};
use rusqlite::{Transaction, params};
use serde::Deserialize;
use serde_json::{Number, Value, json};
const DEPR: &str = "DK-ASSET-DEPR-001";
const WRITEOFF: &str = "DK-ASSET-WRITEOFF-001";
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Register {
    pub name: String,
    pub category: String,
    pub acquisition_date: String,
    pub cost: Number,
    pub useful_life_months: u32,
    pub document_id: i64,
    pub asset_account: Option<String>,
    pub depreciation_account: Option<String>,
    pub accumulated_account: Option<String>,
    pub note: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Depreciate {
    pub period: u32,
    pub date: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WriteOff {
    pub name: String,
    pub category: String,
    pub acquisition_date: String,
    pub cost: Number,
    pub document_id: i64,
    pub expense_account: String,
    pub date: String,
    pub threshold_rule_source: String,
    pub confirm_immediate_write_off: bool,
    pub payment_account: Option<String>,
    pub note: Option<String>,
}
fn note(n: &Option<String>) -> Option<String> {
    n.as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}
fn rules(mut v: Value, rule: &str) -> Value {
    let o = v.as_object_mut().expect("shared object receipt");
    o.entry("ok").or_insert(json!(true));
    o.entry("errors").or_insert(json!([]));
    let r = o
        .entry("appliedRules")
        .or_insert(json!([]))
        .as_array_mut()
        .expect("shared receipt rules");
    if !r.contains(&json!(rule)) {
        r.insert(0, json!(rule));
    }
    v
}
fn positive_id(id: i64) -> Result<(), Error> {
    if id <= 0 || id > 9_007_199_254_740_991 {
        Err(refused("positive safe document/asset ID required"))
    } else {
        Ok(())
    }
}
pub fn purchase_document_exists(tx: &Transaction<'_>, document_id: i64) -> Result<bool, Error> {
    positive_id(document_id)?;
    tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM documents WHERE id=?1)",
        [document_id],
        |r| r.get(0),
    )
    .map_err(invalid)
}
fn original(tx: &Transaction<'_>, id: i64) -> Result<(), Error> {
    let has: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM documents WHERE id=?1 AND stored_path IS NOT NULL AND length(sha256_hash)=64)",[id],|r|r.get(0)).map_err(invalid)?;
    if !has {
        return Err(refused("physical purchase original required"));
    }
    Ok(())
}
pub fn register(
    tx: &Transaction<'_>,
    input: &Register,
    subject: &str,
    kind: PrincipalKind,
    today: Date,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    let actor = crate::accruals::principal(subject, kind)?;
    positive_id(input.document_id)?;
    if input.note.as_ref().is_some_and(|s| s.len() > 4096) {
        return Err(refused("note exceeds supported length"));
    }
    original(tx, input.document_id)?;
    let native = assets::Register {
        name: input.name.clone(),
        category: input.category.clone(),
        acquisition_date: input.acquisition_date.clone(),
        cost: input.cost.clone(),
        useful_life_months: input.useful_life_months,
        purchase_document_id: input.document_id,
        asset_account_no: Some(input.asset_account.clone().unwrap_or_else(|| "5800".into())),
        depreciation_expense_account_no: Some(
            input
                .depreciation_account
                .clone()
                .unwrap_or_else(|| "5820".into()),
        ),
        accumulated_depreciation_account_no: Some(
            input
                .accumulated_account
                .clone()
                .unwrap_or_else(|| "5810".into()),
        ),
        note: note(&input.note),
        confirm: true,
    };
    Ok(rules(
        assets::register_resolved(tx, &native, &actor, today, true)?,
        DEPR,
    ))
}
pub fn depreciate(
    tx: &Transaction<'_>,
    asset_id: i64,
    input: &Depreciate,
    subject: &str,
    kind: PrincipalKind,
    latest: Date,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    let actor = crate::accruals::principal(subject, kind)?;
    positive_id(asset_id)?;
    original(tx, assets::document_id(tx, asset_id)?)?;
    let native = assets::Depreciate {
        transaction_date: Some(input.date.clone()),
        period_index: Some(input.period),
        confirm: true,
    };
    Ok(rules(
        assets::depreciate_resolved(tx, asset_id, &native, &actor, &input.date, latest, true)?,
        DEPR,
    ))
}
/// Known-edge clamping is a ported workflow guardrail, not a tax eligibility assessment.
fn threshold(year: u32) -> u32 {
    if year <= 2024 {
        33100
    } else if year == 2025 {
        34400
    } else {
        36000
    }
}
fn failure(message: String) -> Value {
    json!({"ok":false,"appliedRules":[WRITEOFF],"errors":[message]})
}
fn exception(
    tx: &Transaction<'_>,
    code: &str,
    document: Option<i64>,
    message: &str,
    action: &str,
    evidence: Value,
) -> Result<(), Error> {
    tx.execute("INSERT INTO exceptions(type,severity,status,related_document_id,message,required_action,source_evidence) SELECT ?1,'high','open',?2,?3,?4,?5 WHERE NOT EXISTS(SELECT 1 FROM exceptions WHERE status='open' AND resolution_key IS NULL AND type=?1 AND COALESCE(related_bank_transaction_id,0)=0 AND COALESCE(related_document_id,0)=COALESCE(?2,0) AND message=?3 AND COALESCE(required_action,'')=?4)",params![code,document,message,action,evidence.to_string()]).map_err(invalid)?;
    Ok(())
}
pub fn write_off(
    tx: &Transaction<'_>,
    input: &WriteOff,
    subject: &str,
    kind: PrincipalKind,
    latest: Date,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    let actor = crate::accruals::principal(subject, kind)?;
    positive_id(input.document_id)?;
    if [
        &input.name,
        &input.category,
        &input.expense_account,
        &input.threshold_rule_source,
    ]
    .iter()
    .any(|s| s.trim().is_empty() || s.len() > 500)
        || input.note.as_ref().is_some_and(|s| s.len() > 4096)
    {
        return Err(refused(
            "name, category, expense account and threshold rule source required",
        ));
    }
    Date::parse(&input.acquisition_date)?;
    let date = Date::parse(&input.date)?;
    let cost = money(&input.cost)?;
    if cost == Money::default() || cost.is_negative() {
        return Err(refused("cost must be positive"));
    }
    if !input.confirm_immediate_write_off {
        return Ok(failure("immediate write-off requires confirmImmediateWriteOff: true — the user/advisor must confirm the straksafskrivning treatment".into()));
    }
    let year: u32 = input.acquisition_date[..4].parse().map_err(invalid)?;
    let threshold_dkk = threshold(year);
    let bound = Money::parse(&threshold_dkk.to_string())?;
    let name = input.name.trim();
    if !purchase_document_exists(tx, input.document_id)? {
        let message = format!(
            "Immediate write-off for \"{name}\" references missing purchase document {}",
            input.document_id
        );
        exception(
            tx,
            "ASSET_WRITEOFF_MISSING_DOCUMENTATION",
            None,
            &message,
            "Attach the purchase document, then re-run the immediate write-off.",
            json!({"name":name,"cost":wire(&cost)?,"purchaseDocumentId":input.document_id}),
        )?;
        return Ok(failure(format!(
            "purchase document {} does not exist — write-off blocked, exception queued",
            input.document_id
        )));
    }
    original(tx, input.document_id)?;
    if bound.subtract(&cost).is_negative() {
        let message = format!(
            "Immediate write-off for \"{name}\" (cost {}) exceeds the small-asset threshold {threshold_dkk} for income year {year}",
            wire(&cost)?
        );
        exception(
            tx,
            "ASSET_WRITEOFF_ELIGIBILITY_UNCERTAIN",
            Some(input.document_id),
            &message,
            "Advisor must confirm straksafskrivning eligibility or capitalise and depreciate the asset instead.",
            json!({"name":name,"cost":wire(&cost)?,"thresholdDkk":threshold_dkk,"acquisitionYear":year,"thresholdRuleSource":input.threshold_rule_source.trim()}),
        )?;
        return Ok(failure(format!(
            "cost {} exceeds the small-asset threshold {threshold_dkk} for income year {year} — eligibility uncertain, exception queued for advisor review",
            wire(&cost)?
        )));
    }
    let metadata = crate::expense::metadata(tx, input.document_id)?;
    if metadata.currency.as_deref().unwrap_or("DKK") != "DKK"
        || !matches!(metadata.kind(), "purchase_sale" | "cash_register_receipt")
    {
        return Err(refused("write-off requires a DKK purchase document"));
    }
    let expense = assets::account(tx, Some(input.expense_account.trim()), "expense", "debit")?;
    let payment = match input.payment_account.as_deref() {
        Some(p) => p.trim().to_owned(),
        None => crate::expense::role(tx, "bank")?,
    };
    let duplicate: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM asset_writeoffs WHERE purchase_document_id=?1)",
            [input.document_id],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    if duplicate {
        return Err(refused("purchase document already has immediate write-off"));
    }
    let request = ManualJournal {
        transaction_date: date,
        text: format!("Immediate write-off (straksafskrivning) for {name}"),
        currency: Currency::parse("DKK")?,
        document_id: Some(DocumentId::from_sqlite(input.document_id)),
        source_bank_transaction_id: None,
        amount_foreign: None,
        amount_dkk: None,
        fx_rate_to_dkk: None,
        created_by_program: None,
        lines: vec![
            crate::expense::line(
                &expense,
                cost.clone(),
                Money::default(),
                None,
                format!("Straksafskrivning {name}"),
            )?,
            crate::expense::line(
                &payment,
                Money::default(),
                cost.clone(),
                None,
                format!("Write-off settlement {name}"),
            )?,
        ],
    };
    let mut receipt = crate::journal_post::post(
        tx,
        &request,
        &crate::journal_preview::manual_journal_rule_version()?,
        latest,
        crate::journal_post::Actor {
            subject: &actor,
            program: "rentemester-rust",
        },
    )?;
    if receipt["ok"] != true {
        return Err(refused("write-off journal refused"));
    }
    let entry = receipt["entryId"]
        .as_i64()
        .ok_or_else(|| invalid("missing write-off journal"))?;
    let id: i64 = tx.query_row("INSERT INTO asset_writeoffs(name,category,acquisition_date,writeoff_date,cost,purchase_document_id,expense_account_no,confirmed,threshold_dkk,threshold_rule_source,note,journal_entry_id) VALUES(?1,?2,?3,?4,?5,?6,?7,1,?8,?9,?10,?11) RETURNING id",params![name,input.category.trim(),input.acquisition_date,input.date,crate::journal_post::money(&cost)?,input.document_id,expense,threshold_dkk,input.threshold_rule_source.trim(),note(&input.note),entry],|r|r.get(0)).map_err(invalid)?;
    assets::audit(tx, "asset_immediate_writeoff", id, &actor)?;
    receipt["writeOffId"] = json!(id);
    receipt["cost"] = wire(&cost)?;
    receipt["thresholdDkk"] = json!(threshold_dkk);
    Ok(rules(receipt, WRITEOFF))
}
pub fn report(tx: &Transaction<'_>) -> Result<Value, Error> {
    let mut q = tx
        .prepare("SELECT id,category FROM assets ORDER BY id")
        .map_err(invalid)?;
    let refs = q
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    let mut rows = vec![];
    let mut cost = Money::default();
    let mut accumulated = Money::default();
    for (id, category) in refs {
        let a = assets::load(tx, id)?;
        let (indices, total) = assets::posted(tx, &a)?;
        cost.add(&a.cost);
        accumulated.add(&total);
        rows.push(json!({"assetId":id,"name":a.name,"category":category,"acquisitionDate":a.acquisition,"cost":wire(&a.cost)?,"usefulLifeMonths":a.months,"postedPeriods":indices.len(),"accumulatedDepreciation":wire(&total)?,"netBookValue":wire(&a.cost.subtract(&total))?}));
    }
    Ok(
        json!({"ok":true,"assets":rows,"totals":{"cost":wire(&cost)?,"accumulatedDepreciation":wire(&accumulated)?,"netBookValue":wire(&cost.subtract(&accumulated))?},"errors":[]}),
    )
}

#[cfg(test)]
mod tests;
