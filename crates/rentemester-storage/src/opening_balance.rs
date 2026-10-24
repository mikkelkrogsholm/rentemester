//! Atomic opening marker/provenance in a caller-owned company writer. Physical originals remain admission responsibility.
use crate::{
    archive_import::{self, VerifiedArchive},
    journal_post::{self, Actor},
};
use rentemester_engine::opening_balance::refused;
pub use rentemester_engine::opening_balance::{Apply, ArchiveCitation, Input, Record, SourceBasis};
use rentemester_engine::{Error, fiscal_year::Date, manual_journal, money::Money};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
pub const PROGRAM: &str = "rentemester-opening-balance";
const EVENT: &str = "native_opening_balance";
const VERSION: &str = "native_opening_balance/1";
#[cfg(test)]
mod tests;
fn sql(_: impl std::fmt::Display) -> Error {
    refused("opening source or retained provenance unavailable")
}
fn hash(v: &impl Serialize) -> Result<String, Error> {
    use sha2::{Digest, Sha256};
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(v).map_err(sql)?)
    ))
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Binding {
    input: Input,
    source_basis: SourceBasis,
    ledger_uuid: String,
    head: Option<(i64, String)>,
    company: Value,
    accounts: Vec<Value>,
    rule_version: String,
    facts: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    synthetic_source: Option<crate::synthetic_import::Input>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Event {
    version: String,
    binding: Binding,
    record: Record,
}
pub struct Prepared {
    binding: Binding,
    hash: String,
    blockers: Vec<String>,
    existing: Option<Record>,
    debit: Money,
    credit: Money,
}
impl Prepared {
    pub fn review(&self) -> Value {
        json!({"planHash":self.hash,"input":self.binding.input,"ruleVersion":self.binding.rule_version,"sourceBasis":self.binding.source_basis,"totals":{"debit":self.debit,"credit":self.credit},"existing":self.existing,"blockers":self.blockers,"status":if self.blockers.is_empty(){"ready"}else{"blocked"}})
    }
}
/// Validated immutable marker/journal metadata, not a physical-source or live-authority grant.
pub struct VerifiedOpening {
    record: Record,
}
impl VerifiedOpening {
    pub fn record(&self) -> &Record {
        &self.record
    }
    pub fn source_archive_import_ids(&self) -> Vec<i64> {
        self.record
            .source_basis
            .source_archives
            .iter()
            .map(|r| r.import_id)
            .collect()
    }
    pub fn cut_over_date(&self) -> Date {
        self.record.cut_over_date
    }
}
fn citations(db: &Connection, ids: &[i64]) -> Result<Vec<ArchiveCitation>, Error> {
    archive_import::validate_history(db)?;
    let originals = archive_import::originals(db)?;
    let mut out = vec![];
    for id in ids {
        let original = originals
            .iter()
            .find(|r| r.import_id == *id)
            .ok_or_else(|| refused("cited archive import not found"))?;
        let body:String=db.query_row("SELECT message FROM audit_log WHERE id=? AND event_type='native_closed_history_archive_import'",[id],|r|r.get(0)).map_err(sql)?;
        let body: Value = serde_json::from_str(&body).map_err(sql)?;
        let archive = &body["binding"]["archive"];
        out.push(ArchiveCitation {
            import_id: *id,
            source_hash: original.sha256.clone(),
            parser_contract: archive["parserContract"]
                .as_str()
                .ok_or_else(|| sql("parser"))?
                .into(),
            declared_currency: archive["declaredCurrency"]
                .as_str()
                .ok_or_else(|| sql("currency"))?
                .into(),
        });
    }
    Ok(out)
}
pub fn prepare(
    tx: &Transaction<'_>,
    input: &Input,
    verified: &[VerifiedArchive],
    latest: Date,
) -> Result<Prepared, Error> {
    prepare_mode(tx, input, verified, latest, false, None)
}
fn prepare_mode(
    tx: &Transaction<'_>,
    input: &Input,
    verified: &[VerifiedArchive],
    latest: Date,
    source_import: bool,
    synthetic_source: Option<&crate::synthetic_import::Input>,
) -> Result<Prepared, Error> {
    crate::journal_preview::require_current(tx)?;
    input.validate()?;
    archive_import::read(tx, verified, None)?;
    let existing = validate_history(tx)?.map(|v| v.record);
    let company:Value=tx.query_row("SELECT name,cvr,currency,fiscal_year_start_month,fiscal_year_label_strategy FROM companies WHERE id=1",[],|r|Ok(json!({"name":r.get::<_,String>(0)?,"cvr":r.get::<_,Option<String>>(1)?,"currency":r.get::<_,String>(2)?,"fiscalYearStartMonth":r.get::<_,i64>(3)?,"fiscalYearLabelStrategy":r.get::<_,String>(4)?}))).map_err(sql)?;
    let currency = company["currency"]
        .as_str()
        .ok_or_else(|| sql("currency"))?;
    let request = input.request(currency)?;
    let rules = crate::journal_preview::compiled_rules()?;
    let rule_version = rules.version().to_owned();
    let facts = crate::journal_preview::collect_facts(tx, &request, latest)?;
    let validation = if source_import {
        let mut value = serde_json::to_value(&request).map_err(sql)?;
        value["createdByProgram"] = json!("rentemester-import-postings");
        let source_request = serde_json::from_value(value).map_err(sql)?;
        manual_journal::validate_historical_import(&source_request, &facts, &rules, &rule_version)?
    } else {
        manual_journal::validate(&request, &facts, &rules, &rule_version)?
    };
    let mut blockers: Vec<String> = validation
        .errors
        .iter()
        .map(|e| e.message.clone())
        .collect();
    if existing.is_some() {
        blockers.push("opening balance already exists".into());
    }
    let before: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM journal_entries WHERE transaction_date<?)",
            [input.cut_over_date.to_string()],
            |r| r.get(0),
        )
        .map_err(sql)?;
    if before {
        blockers.push("native journal history predates the proposed cutover".into());
    }
    // Match exact persistence limits before offering a ready plan.
    for line in &input.lines {
        journal_post::money(&line.debit_amount)?;
        journal_post::money(&line.credit_amount)?;
    }
    let mut q=tx.prepare("SELECT id,account_no,name,type,normal_balance,active,allow_direct_posting FROM accounts ORDER BY id").map_err(sql)?;
    let accounts = q
        .query_map([], |r| {
            Ok(json!([
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, i64>(6)?
            ]))
        })
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    let binding = Binding {
        input: input.clone(),
        source_basis: SourceBasis {
            kind: if synthetic_source.is_some() {
                "synthetic_csv"
            } else {
                "operator_supplied"
            }
            .into(),
            currency: currency.into(),
            source_archives: citations(tx, &input.source_archive_import_ids)?,
        },
        ledger_uuid: tx
            .query_row(
                "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
                [],
                |r| r.get(0),
            )
            .map_err(sql)?,
        head: tx
            .query_row(
                "SELECT id,entry_hash FROM journal_entries ORDER BY id DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(sql)?,
        company,
        accounts,
        rule_version,
        facts: serde_json::to_value(&facts).map_err(sql)?,
        synthetic_source: synthetic_source.cloned(),
    };
    Ok(Prepared {
        hash: hash(&binding)?,
        binding,
        blockers,
        existing,
        debit: validation.total_debit,
        credit: validation.total_credit,
    })
}
pub(crate) fn prepare_import(
    tx: &Transaction<'_>,
    input: &Input,
    verified: &[VerifiedArchive],
    latest: Date,
    _admitted: &crate::dinero_import::AdmittedVoucher,
) -> Result<Prepared, Error> {
    prepare_mode(tx, input, verified, latest, true, None)
}
/// Fresh-key domain callback only. The authorized adapter owns durable keyed receipts and commit.
pub fn apply(
    tx: &Transaction<'_>,
    input: &Apply,
    verified: &[VerifiedArchive],
    latest: Date,
    actor: Actor<'_>,
    now_ms: i64,
) -> Result<Value, Error> {
    apply_mode(tx, &input.input, Some(input), verified, latest, actor, now_ms, None, None)
}
pub(crate) fn apply_import(
    tx: &Transaction<'_>,
    input: &Apply,
    verified: &[VerifiedArchive],
    latest: Date,
    actor: Actor<'_>,
    now_ms: i64,
    admitted: &crate::dinero_import::AdmittedVoucher,
) -> Result<Value, Error> {
    apply_mode(
        tx,
        &input.input,
        Some(input),
        verified,
        latest,
        actor,
        now_ms,
        Some(admitted),
        None,
    )
}
#[allow(clippy::too_many_arguments)]
fn apply_mode(
    tx: &Transaction<'_>,
    input: &Input,
    reviewed: Option<&Apply>,
    verified: &[VerifiedArchive],
    latest: Date,
    actor: Actor<'_>,
    now_ms: i64,
    admitted: Option<&crate::dinero_import::AdmittedVoucher>,
    synthetic_source: Option<&crate::synthetic_import::Input>,
) -> Result<Value, Error> {
    journal_post::require_current(tx)?;
    if let Some(reviewed) = reviewed {
        crate::idempotency::validate_key(&reviewed.idempotency_key)?;
    }
    if reviewed.is_some_and(|r| !r.confirm)
        || actor.program != PROGRAM
        || !actor.subject.starts_with("user:")
        || actor.subject.len() <= 5
    {
        return Err(refused(
            "explicit current human opening confirmation required",
        ));
    }
    let prepared = prepare_mode(
        tx,
        input,
        verified,
        latest,
        admitted.is_some(),
        synthetic_source,
    )?;
    if reviewed.is_some_and(|r| r.plan_hash != prepared.hash) {
        return Err(refused("opening plan changed; review a fresh plan"));
    }
    if !prepared.blockers.is_empty() {
        return Err(refused(prepared.blockers.join("; ")));
    }
    let request = input
        .request(&prepared.binding.source_basis.currency)?;
    let posted = if let Some(admitted) = admitted {
        journal_post::post_import_opening(tx, &request, latest, actor.subject, admitted)?
    } else {
        journal_post::post(
            tx,
            &request,
            &prepared.binding.rule_version,
            latest,
            Actor {
                subject: actor.subject,
                program: PROGRAM,
            },
        )?
    };
    if posted["ok"] != true {
        return Err(refused("opening journal was refused; abort transaction"));
    }
    let created_at = rentemester_engine::company_knowledge::timestamp(now_ms)?;
    let record = Record {
        cut_over_date: input.cut_over_date,
        journal_entry_id: posted["entryId"].as_i64().ok_or_else(|| sql("entry"))?,
        journal_entry_no: posted["entryNo"]
            .as_str()
            .ok_or_else(|| sql("number"))?
            .into(),
        journal_entry_hash: posted["entryHash"]
            .as_str()
            .ok_or_else(|| sql("hash"))?
            .into(),
        plan_hash: prepared.hash,
        source_basis: prepared.binding.source_basis.clone(),
        created_at,
        actor: actor.subject.into(),
    };
    let event = Event {
        version: VERSION.into(),
        binding: prepared.binding,
        record: record.clone(),
    };
    tx.execute("INSERT INTO opening_balances(id,cut_over_date,journal_entry_id,journal_entry_no,created_at)VALUES(1,?1,?2,?3,?4)",params![record.cut_over_date.to_string(),record.journal_entry_id,record.journal_entry_no,record.created_at]).map_err(sql)?;
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor,created_at)VALUES(?1,'opening_balance','1',?2,?3,?4)",params![EVENT,serde_json::to_string(&event).map_err(sql)?,record.actor,record.created_at]).map_err(sql)?;
    validate_history(tx)?;
    Ok(json!({"ok":true,"openingBalance":record,"errors":[]}))
}
pub fn read(db: &Connection) -> Result<Value, Error> {
    Ok(json!({"ok":true,"openingBalance":validate_history(db)?.map(|v|v.record),"errors":[]}))
}
/// All retained bytes and original references, independent of current chart/configuration/authority.
pub fn validate_history(db: &Connection) -> Result<Option<VerifiedOpening>, Error> {
    crate::journal_preview::require_current(db)?;
    let count: i64 = db
        .query_row("SELECT count(*) FROM opening_balances", [], |r| r.get(0))
        .map_err(sql)?;
    let events: i64 = db
        .query_row(
            "SELECT count(*) FROM audit_log WHERE event_type=?",
            [EVENT],
            |r| r.get(0),
        )
        .map_err(sql)?;
    let programs: i64 = db
        .query_row(
            "SELECT count(*) FROM journal_entries WHERE created_by_program=?",
            [PROGRAM],
            |r| r.get(0),
        )
        .map_err(sql)?;
    if (count, events, programs) == (0, 0, 0) {
        return Ok(None);
    }
    if (count, events, programs) != (1, 1, 1) {
        return Err(refused(
            "opening marker, program and provenance must agree; no automatic adoption",
        ));
    }
    let (body,actor,at,entity,entity_id):(String,String,String,String,String)=db.query_row("SELECT message,actor,created_at,entity_type,entity_id FROM audit_log WHERE event_type=?",[EVENT],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).map_err(sql)?;
    let e: Event = serde_json::from_str(&body).map_err(sql)?;
    let r = &e.record;
    if e.version != VERSION
        || serde_json::to_string(&e).map_err(sql)? != body
        || entity != "opening_balance"
        || entity_id != "1"
        || actor != r.actor
        || at != r.created_at
        || !r.actor.starts_with("user:")
        || r.actor.len() <= 5
        || r.plan_hash != hash(&e.binding)?
        || r.source_basis != e.binding.source_basis
        || r.cut_over_date != e.binding.input.cut_over_date
        || r.source_basis.kind
            != if e.binding.synthetic_source.is_some() {
                "synthetic_csv"
            } else {
                "operator_supplied"
            }
        || r.source_basis.source_archives
            != citations(db, &e.binding.input.source_archive_import_ids)?
    {
        return Err(refused("opening canonical retained provenance differs"));
    }
    e.binding.input.validate()?;
    if let Some(source) = &e.binding.synthetic_source {
        crate::synthetic_import::verify_opening(source, &e.binding.input)?;
    }
    if e.binding.company["currency"] != r.source_basis.currency {
        return Err(refused("opening original currency binding differs"));
    }

    let uuid: String = db
        .query_row(
            "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
            [],
            |r| r.get(0),
        )
        .map_err(sql)?;
    if uuid != e.binding.ledger_uuid {
        return Err(refused("opening ledger identity differs"));
    }
    let marker:(String,i64,String,String)=db.query_row("SELECT cut_over_date,journal_entry_id,journal_entry_no,created_at FROM opening_balances WHERE id=1",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).map_err(sql)?;
    if marker
        != (
            r.cut_over_date.to_string(),
            r.journal_entry_id,
            r.journal_entry_no.clone(),
            r.created_at.clone(),
        )
    {
        return Err(refused("opening marker differs from original event"));
    }
    let request = e.binding.input.request(&r.source_basis.currency)?;
    struct Posted {
        number: String,
        date: String,
        text: String,
        currency: String,
        actor: String,
        program: String,
        status: String,
        previous: Option<String>,
        hash: String,
        document: Option<i64>,
        bank: Option<i64>,
        foreign: Option<f64>,
        dkk: Option<f64>,
        fx: Option<f64>,
        reversal: Option<i64>,
    }
    let original=db.query_row("SELECT entry_no,transaction_date,text,currency,created_by,created_by_program,status,previous_hash,entry_hash,document_id,source_bank_transaction_id,amount_foreign,amount_dkk,fx_rate_to_dkk,reversal_of_entry_id FROM journal_entries WHERE id=?",[r.journal_entry_id],|row|Ok(Posted{number:row.get(0)?,date:row.get(1)?,text:row.get(2)?,currency:row.get(3)?,actor:row.get(4)?,program:row.get(5)?,status:row.get(6)?,previous:row.get(7)?,hash:row.get(8)?,document:row.get(9)?,bank:row.get(10)?,foreign:row.get(11)?,dkk:row.get(12)?,fx:row.get(13)?,reversal:row.get(14)?})).map_err(sql)?;
    let expected_previous = e
        .binding
        .head
        .as_ref()
        .map(|(_, h)| h.as_str())
        .unwrap_or("GENESIS");
    if original.number != r.journal_entry_no
        || original.date != r.cut_over_date.to_string()
        || original.text != request.text
        || original.currency != r.source_basis.currency
        || original.actor != r.actor
        || original.program != PROGRAM
        || original.status != "posted"
        || original.hash != r.journal_entry_hash
        || original.previous.as_deref() != Some(expected_previous)
        || original.document.is_some()
        || original.bank.is_some()
        || original.foreign.is_some()
        || original.dkk.is_some()
        || original.fx.is_some()
        || original.reversal.is_some()
    {
        return Err(refused(
            "opening journal identity or original context differs",
        ));
    }
    let postaudit:i64=db.query_row("SELECT count(*) FROM audit_log WHERE event_type='journal_post' AND entity_type='journal_entry' AND entity_id=?1 AND actor=?2 AND message=?3",params![r.journal_entry_id.to_string(),format!("{} via {PROGRAM}",r.actor),format!("Posted journal entry {}",r.journal_entry_no)],|row|row.get(0)).map_err(sql)?;
    if postaudit != 1 {
        return Err(refused("opening original posting audit differs"));
    }
    let version: String = db
        .query_row(
            "SELECT rule_version FROM journal_entries WHERE id=?",
            [r.journal_entry_id],
            |row| row.get(0),
        )
        .map_err(sql)?;
    if version != e.binding.rule_version {
        return Err(refused("opening historical rule version differs"));
    }
    if let Some((id, h)) = &e.binding.head {
        let actual: String = db
            .query_row(
                "SELECT entry_hash FROM journal_entries WHERE id=?",
                [id],
                |row| row.get(0),
            )
            .map_err(sql)?;
        if *id >= r.journal_entry_id || actual != *h {
            return Err(refused("opening reviewed head is not an original prefix"));
        }
    }
    let incompatible:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM journal_entries WHERE transaction_date<?1 OR reversal_of_entry_id=?2)",params![r.cut_over_date.to_string(),r.journal_entry_id],|row|row.get(0)).map_err(sql)?;
    if incompatible {
        return Err(refused(
            "opening basis has pre-cutover history or an unsupported opening reversal",
        ));
    }
    let mut q=db.prepare("SELECT a.account_no,l.debit_amount,l.credit_amount,l.text,l.vat_code,l.currency FROM journal_lines l JOIN accounts a ON a.id=l.account_id WHERE l.journal_entry_id=? ORDER BY l.id").map_err(sql)?;
    let mut rows = q.query([r.journal_entry_id]).map_err(sql)?;
    for line in &request.lines {
        let row = rows
            .next()
            .map_err(sql)?
            .ok_or_else(|| refused("opening original line missing"))?;
        if row.get::<_, String>(0).map_err(sql)? != line.account_no.as_str()
            || crate::legacy_money(row.get_ref(1).map_err(sql)?)? != line.debit_amount
            || crate::legacy_money(row.get_ref(2).map_err(sql)?)? != line.credit_amount
            || row.get::<_, Option<String>>(3).map_err(sql)? != line.text
            || row.get::<_, Option<String>>(4).map_err(sql)?.is_some()
            || row.get::<_, String>(5).map_err(sql)? != "DKK"
        {
            return Err(refused("opening original line differs"));
        }
    }
    if rows.next().map_err(sql)?.is_some() || !crate::journal_hash::verify(db)?.ok {
        return Err(refused("opening journal chain or line inventory differs"));
    }
    Ok(Some(VerifiedOpening { record: e.record }))
}
pub fn require_posting_date(db: &Connection, date: Date) -> Result<(), Error> {
    if let Some(opening) = validate_history(db)?
        && date < opening.cut_over_date()
    {
        return Err(refused("posting predates the explicit opening cutover"));
    }
    Ok(())
}
fn report_basis(db: &Connection, from: Option<&str>, to: &str) -> Result<Value, Error> {
    let end = Date::parse(to)?;
    let start = from.map(Date::parse).transpose()?;
    if start.is_some_and(|d| d > end) {
        return Err(refused("report range is inverted"));
    }
    let opening = validate_history(db)?;
    if let Some(v) = opening {
        if end < v.cut_over_date() || start.is_some_and(|d| d < v.cut_over_date()) {
            return Err(Error::new(
                "archive_report_unsupported",
                "requested report starts before explicit live cutover",
            ));
        }
        return Ok(
            json!({"basis":"live","cutOverDate":v.cut_over_date(),"openingJournalEntryId":v.record.journal_entry_id,"archiveAmountsIncluded":false}),
        );
    }
    archive_import::require_live_report_only(db)?;
    Ok(
        json!({"basis":"live","cutOverDate":null,"openingJournalEntryId":null,"archiveAmountsIncluded":false}),
    )
}
pub fn require_report_range(db: &Connection, from: &str, to: &str) -> Result<Value, Error> {
    report_basis(db, Some(from), to)
}
pub fn require_report_as_of(db: &Connection, as_of: &str) -> Result<Value, Error> {
    report_basis(db, None, as_of)
}

/// Same reviewed opening policy, with exact synthetic CSV retained in the immutable event.
pub(crate) fn prepare_synthetic(
    tx: &Transaction<'_>,
    input: &Input,
    verified: &[VerifiedArchive],
    latest: Date,
    source: &crate::synthetic_import::Input,
) -> Result<Prepared, Error> {
    crate::synthetic_import::verify_opening(source, input)?;
    prepare_mode(tx, input, verified, latest, false, Some(source))
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_synthetic(
    tx: &Transaction<'_>,
    input: &Apply,
    verified: &[VerifiedArchive],
    latest: Date,
    actor: Actor<'_>,
    now_ms: i64,
    source: &crate::synthetic_import::Input,
) -> Result<Value, Error> {
    crate::synthetic_import::verify_opening(source, &input.input)?;
    apply_mode(
        tx,
        &input.input,
        Some(input),
        verified,
        latest,
        actor,
        now_ms,
        None,
        Some(source),
    )
}

/// Server-owned original admission: the plan is derived under the caller's transaction.
#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_original(
    tx: &Transaction<'_>, input: &Input, verified: &[VerifiedArchive], latest: Date,
    actor: Actor<'_>, now_ms: i64,
    admitted: Option<&crate::dinero_import::AdmittedVoucher>,
    source: Option<&crate::synthetic_import::Input>,
) -> Result<Value, Error> {
    apply_mode(tx, input, None, verified, latest, actor, now_ms, admitted, source)
}
