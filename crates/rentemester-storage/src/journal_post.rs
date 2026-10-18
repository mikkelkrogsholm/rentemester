//! Real manual journal effects in a caller-owned admitted company write transaction.
use crate::{
    journal_hash::{self, Entry, Line},
    journal_preview, sequences,
};
use rentemester_engine::{
    Error, calendar,
    fiscal_year::{self, Date, LabelStrategy, StartMonth},
    fx::FxRate,
    manual_journal::{self, ManualJournal},
    money::Money,
};
use rusqlite::{Connection, MAIN_DB, OptionalExtension, Transaction, TransactionState, params};
use serde_json::{Value, json};
const SAFE: i64 = 9_007_199_254_740_991;
pub struct Actor<'a> {
    pub subject: &'a str,
    pub program: &'a str,
}
fn failed(_: impl std::fmt::Display) -> Error {
    Error::new(
        "posting_storage_failure",
        "manual posting failed; abort caller transaction",
    )
}
fn precision() -> Error {
    Error::new(
        "unsupported_legacy_number",
        "exact value cannot be persisted in legacy numeric/hash domain; abort transaction",
    )
}
fn safe(n: i64) -> Result<i64, Error> {
    if n.unsigned_abs() > SAFE as u64 {
        Err(precision())
    } else {
        Ok(n)
    }
}
fn trim(s: &str) -> &str {
    s.trim_matches(|c|matches!(c,'\u{9}'..='\u{d}'|'\u{20}'|'\u{a0}'|'\u{1680}'|'\u{2000}'..='\u{200a}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}'|'\u{feff}'))
}
fn money(value: &Money) -> Result<f64, Error> {
    let scaled = safe(
        value
            .decimal()
            .replace('.', "")
            .parse::<i64>()
            .map_err(|_| precision())?,
    )?;
    let n = scaled as f64 / 100.0;
    if Money::parse(ryu_js::Buffer::new().format_finite(n)).map_err(|_| precision())? != *value {
        return Err(precision());
    }
    Ok(n)
}
fn rate(value: &FxRate) -> Result<f64, Error> {
    let scaled = safe(
        value
            .decimal()
            .replace('.', "")
            .parse::<i64>()
            .map_err(|_| precision())?,
    )?;
    let n = scaled as f64 / 1_000_000.0;
    if FxRate::parse(ryu_js::Buffer::new().format_finite(n)).map_err(|_| precision())? != *value {
        return Err(precision());
    }
    Ok(n)
}
fn next_id(db: &Connection, table: &str) -> Result<i64, Error> {
    let (min, max): (Option<i64>, Option<i64>) = db
        .query_row(
            &format!("SELECT MIN(id),MAX(id) FROM main.{table}"),
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(failed)?;
    match (min, max) {
        (None, None) => Ok(1),
        (Some(min), Some(max)) if min > 0 && max < SAFE => Ok(max + 1),
        _ => Err(precision()),
    }
}
struct Settings {
    scope: String,
    start: u32,
    strategy: LabelStrategy,
}
fn settings(db: &Connection) -> Result<Settings, Error> {
    let row:Option<(i64,Option<String>,i64,String)>=db.query_row("SELECT id,cvr,fiscal_year_start_month,fiscal_year_label_strategy FROM main.companies ORDER BY id LIMIT 1",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(failed)?;
    let (id, cvr, start, strategy) = row.unwrap_or((1, None, 1, "end-year".to_owned()));
    safe(id)?;
    let cvr = cvr.as_deref().map(trim).filter(|s| !s.is_empty());
    let scope = match cvr {
        None => format!("company-{id}"),
        Some(cvr) => {
            let upper = cvr.to_uppercase();
            let digits = upper.strip_prefix("DK").unwrap_or(&upper);
            if digits.len() != 8 || !digits.bytes().all(|c| c.is_ascii_digit()) {
                return Err(failed("cvr"));
            }
            format!("DK{digits}")
        }
    };
    Ok(Settings {
        scope,
        start: if (1..=12).contains(&start) {
            start as u32
        } else {
            1
        },
        strategy: LabelStrategy::try_from(strategy.as_str()).unwrap_or(LabelStrategy::EndYear),
    })
}
/// Main writer + exact current canonical company schema/history/integrity. No authority grant.
pub fn require_current(tx: &Transaction<'_>) -> Result<(), Error> {
    if tx.transaction_state(Some(MAIN_DB)).map_err(failed)? != TransactionState::Write {
        return Err(Error::new(
            "transaction_required",
            "manual posting requires main write transaction",
        ));
    }
    journal_preview::require_current(tx)?;
    Ok(())
}
/// Every Err requires caller rollback. An ok:false result has no writes; caller must also
/// roll back rather than commit it through a successful receipt. No capture, path or commit.
pub fn post(
    tx: &Transaction<'_>,
    request: &ManualJournal,
    expected_version: &str,
    latest: Date,
    actor: Actor<'_>,
) -> Result<Value, Error> {
    require_current(tx)?;
    let rules = journal_preview::compiled_rules()?;
    let facts = journal_preview::collect_facts(tx, request, latest)?;
    let mut validation = manual_journal::validate(request, &facts, &rules, expected_version)?;
    // Existing v36 readers validate FX with binary64 multiplication. Never persist an
    // exact-engine result which those readers would diagnose as inconsistent.
    if request.currency.as_str() != "DKK"
        && let (Some(foreign), Some(fx)) = (&request.amount_foreign, &request.fx_rate_to_dkk)
        && !foreign.is_negative()
        && foreign != &Money::default()
    {
        let product = money(foreign)? * rate(fx)?;
        if !product.is_finite() {
            return Err(precision());
        }
        let legacy =
            Money::parse(ryu_js::Buffer::new().format_finite(product)).map_err(|_| precision())?;
        if legacy != fx.apply(foreign) {
            return Err(precision());
        }
    }
    // Source treats a zero document reference as absent for income/expense. Keep
    // this storage compatibility rule out of the exact, advisory engine.
    if request.document_id.is_some_and(|id| id.value() == 0)
        && validation
            .required_rules
            .contains(&rentemester_engine::journal_rules::DOCUMENT)
    {
        validation
            .errors
            .retain(|error| error.code != "document_missing");
        let index = validation
            .errors
            .iter()
            .position(|e| {
                matches!(
                    e.code,
                    "credit_note_already_posted" | "bank_missing" | "bank_reconciled"
                )
            })
            .unwrap_or(validation.errors.len());
        validation.errors.insert(
            index,
            manual_journal::Violation {
                code: "document_required",
                field: "documentId".to_owned(),
                message: "documentId is required when posting expense or income lines".to_owned(),
            },
        );
        validation.ok = false;
    }
    if !validation.ok {
        return Ok(
            json!({"ok":false,"appliedRules":validation.required_rules,"errors":validation.errors.iter().map(|error| {
                match error.code {
                    "unbalanced" => Ok(format!("journal entry must balance: debit {} != credit {}",ryu_js::Buffer::new().format_finite(money(&validation.total_debit)?),ryu_js::Buffer::new().format_finite(money(&validation.total_credit)?))),
                    "fx_amount_mismatch" => {
                        let expected=request.fx_rate_to_dkk.as_ref().ok_or_else(precision)?.apply(request.amount_foreign.as_ref().ok_or_else(precision)?);
                        Ok(format!("amountDkk must equal amountForeign * fxRateToDkk ({})",ryu_js::Buffer::new().format_finite(money(&expected)?)))
                    },
                    _ => Ok(error.message.clone())
                }
            }).collect::<Result<Vec<String>,Error>>()?}),
        );
    }
    if request.created_by_program.is_some()
        || trim(actor.subject).is_empty()
        || actor.subject.len() > 512
        || trim(actor.program).is_empty()
        || trim(actor.program) == "rentemester-import-postings"
        || actor.program.len() > 256
    {
        return Err(Error::new(
            "invalid_input",
            "posting actor/program must be trusted runtime attribution",
        ));
    }
    let actor_subject = trim(actor.subject);
    let program = trim(actor.program);
    let settings = settings(tx)?;
    let fiscal = fiscal_year::for_date(
        request.transaction_date,
        StartMonth::new(settings.start as i32)?,
        settings.strategy,
    )?
    .identifier_label;
    let prefix = format!("{fiscal}-");
    let floor:i64=tx.query_row("SELECT COALESCE(MAX(CAST(substr(entry_no,?1+1) AS INTEGER)),0) FROM main.journal_entries WHERE substr(entry_no,1,?1)=?2 AND length(entry_no)>?1 AND substr(entry_no,?1+1) NOT GLOB '*[^0-9]*'",params![prefix.len() as i64,prefix],|r|r.get(0)).map_err(failed)?;
    safe(floor)?;
    let id = next_id(tx, "journal_entries")?;
    let mut line_id = next_id(tx, "journal_lines")?;
    let previous: Option<String> = tx
        .query_row(
            "SELECT entry_hash FROM main.journal_entries ORDER BY id DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(failed)?;
    if let Some(hash) = &previous
        && (hash.len() != 64
            || !hash
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)))
    {
        return Err(failed("previous hash"));
    }
    let previous = previous.unwrap_or_else(|| "GENESIS".to_owned());
    let foreign = request.amount_foreign.as_ref().map(money).transpose()?;
    let dkk = request.amount_dkk.as_ref().map(money).transpose()?;
    let fx = request.fx_rate_to_dkk.as_ref().map(rate).transpose()?;
    let bank = request
        .source_bank_transaction_id
        .map(|id| safe(id.value()))
        .transpose()?;
    let document = request.document_id.map(|id| safe(id.value())).transpose()?;
    let mut lines = Vec::with_capacity(request.lines.len());
    let mut bound = Vec::with_capacity(request.lines.len());
    for line in &request.lines {
        let account_id: i64 = tx
            .query_row(
                "SELECT id FROM main.accounts WHERE account_no=?1",
                [line.account_no.as_str()],
                |r| r.get(0),
            )
            .map_err(failed)?;
        safe(account_id)?;
        let debit = money(&line.debit_amount)?;
        let credit = money(&line.credit_amount)?;
        let vat = line
            .vat_code
            .as_deref()
            .map(trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        lines.push(Line {
            ordinal: lines.len(),
            account_no: line.account_no.as_str().to_owned(),
            debit_amount: journal_hash::json_number(debit)?,
            credit_amount: journal_hash::json_number(credit)?,
            vat_code: vat.clone(),
            text: line.text.clone(),
        });
        bound.push((account_id, debit, credit, vat, &line.text));
    }
    let next = sequences::next(
        tx,
        "journal_entry",
        &format!("{}:{fiscal}", settings.scope),
        floor,
    )?;
    safe(next)?;
    let number = format!("{fiscal}-{next:05}");
    let date = request.transaction_date.to_string();
    let retention = calendar::retention_deadline(&date, settings.start)?;
    let entry = Entry {
        id,
        entry_no: number.clone(),
        transaction_date: date.clone(),
        text: request.text.clone(),
        source_bank_transaction_id: bank,
        document_id: document,
        currency: request.currency.as_str().to_owned(),
        amount_foreign: foreign.map(journal_hash::json_number).transpose()?,
        amount_dkk: dkk.map(journal_hash::json_number).transpose()?,
        fx_rate_to_dkk: fx.map(journal_hash::json_number).transpose()?,
        rule_version: rules.version().to_owned(),
        created_by: actor_subject.to_owned(),
        created_by_program: program.to_owned(),
        status: "posted".to_owned(),
        reversal_of_entry_id: None,
        lines,
    };
    let hash = journal_hash::canonical_hash(&entry, &previous)?;
    checkpoint(tx, "sequence")?;
    tx.execute("INSERT INTO main.journal_entries(id,entry_no,transaction_date,text,source_bank_transaction_id,document_id,currency,amount_foreign,amount_dkk,fx_rate_to_dkk,rule_version,created_by,created_by_program,status,previous_hash,entry_hash,retain_until) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,'posted',?14,?15,?16)",params![id,number,date,request.text,bank,document,request.currency.as_str(),foreign,dkk,fx,rules.version(),actor_subject,program,previous,hash,retention]).map_err(failed)?;
    checkpoint(tx, "header")?;
    for (ordinal, (account, debit, credit, vat, text)) in bound.into_iter().enumerate() {
        if line_id > SAFE {
            return Err(precision());
        }
        tx.execute("INSERT INTO main.journal_lines(id,journal_entry_id,account_id,debit_amount,credit_amount,vat_code,currency,text) VALUES(?1,?2,?3,?4,?5,?6,'DKK',?7)",params![line_id,id,account,debit,credit,vat,text]).map_err(failed)?;
        line_id = line_id.checked_add(1).ok_or_else(precision)?;
        checkpoint(tx, &format!("line-{ordinal}"))?;
    }
    checkpoint(tx, "lines")?;
    let audit_id = next_id(tx, "audit_log")?;
    let audit_actor = if actor_subject == "system" && program == "system" {
        "system".to_owned()
    } else {
        format!("{actor_subject} via {program}")
    };
    tx.execute("INSERT INTO main.audit_log(id,event_type,entity_type,entity_id,message,actor) VALUES(?1,'journal_post','journal_entry',?2,?3,?4)",params![audit_id,id.to_string(),format!("Posted journal entry {number}"),audit_actor]).map_err(failed)?;
    checkpoint(tx, "audit")?;
    if let Some(bank) = bank.filter(|id| *id > 0) {
        tx.execute("UPDATE main.exceptions SET status='resolved',resolved_at=CURRENT_TIMESTAMP,resolved_by=?1,resolution_note=?2 WHERE type='UNMATCHED_BANK_TRANSACTION' AND status='open' AND related_bank_transaction_id=?3",params![actor_subject,format!("Resolved automatically by posted journal entry {number}"),bank]).map_err(failed)?;
    }
    checkpoint(tx, "exceptions")?;
    let (stored_retention, locked): (String, i64) = tx
        .query_row(
            "SELECT retain_until,locked FROM main.journal_entries WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(failed)?;
    if journal_hash::stored_hash(tx, id, &previous)? != hash
        || stored_retention != retention
        || locked != 1
    {
        return Err(failed("write readback"));
    }
    Ok(
        json!({"ok":true,"appliedRules":validation.required_rules,"errors":[],"entryId":id,"entryNo":number,"entryHash":hash}),
    )
}
fn checkpoint(tx: &Transaction<'_>, point: &str) -> Result<(), Error> {
    #[cfg(test)]
    {
        if std::env::var("RM_POST_FAIL").ok().as_deref() == Some(point) {
            tx.execute("INSERT INTO rm_post_synthetic_missing_table VALUES(1)", [])
                .map_err(failed)?;
        }
        if std::env::var("RM_POST_POINT").ok().as_deref() == Some(point) {
            std::fs::write(std::env::var("RM_POST_MARKER").unwrap(), point).unwrap();
            let mut line = String::new();
            std::io::stdin().read_line(&mut line).unwrap();
        }
    }
    #[cfg(not(test))]
    let _ = (tx, point);
    Ok(())
}
#[cfg(test)]
mod tests;
