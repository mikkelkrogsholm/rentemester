//! Append-only generic reversal. All original evidence is loaded under the caller writer.
use crate::journal_post::{self, Actor, Reversal};
use rentemester_engine::{
    Error,
    currency::Currency,
    fiscal_year::Date,
    fx::FxRate,
    identifiers::{AccountNo, BankTransactionId, DocumentId, JournalEntryId},
    journal_rules,
    manual_journal::{JournalLine, ManualJournal},
    money::Money,
};
use rusqlite::{OptionalExtension, Row, Transaction, types::ValueRef};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Input {
    pub entry_id: JournalEntryId,
    pub transaction_date: Date,
    pub reason: String,
}
fn failed(_: impl std::fmt::Display) -> Error {
    Error::new(
        "reversal_storage_failure",
        "reversal evidence cannot be read; abort transaction",
    )
}
fn precision() -> Error {
    Error::new(
        "unsupported_legacy_number",
        "stored original is outside exact supported legacy numeric domain",
    )
}
fn number(row: &Row<'_>, column: usize) -> Result<Option<f64>, Error> {
    match row.get_ref(column).map_err(failed)? {
        ValueRef::Null => Ok(None),
        ValueRef::Integer(n) if n.unsigned_abs() <= 9_007_199_254_740_991 => Ok(Some(n as f64)),
        ValueRef::Real(n) if n.is_finite() => Ok(Some(n)),
        _ => Err(precision()),
    }
}
fn amount(row: &Row<'_>, column: usize) -> Result<Option<Money>, Error> {
    number(row, column)?
        .map(|n| {
            let value = Money::parse(ryu_js::Buffer::new().format_finite(n))?;
            if journal_post::money(&value)? != n {
                return Err(precision());
            }
            Ok(value)
        })
        .transpose()
}
fn fx(row: &Row<'_>, column: usize) -> Result<Option<f64>, Error> {
    number(row, column)?
        .map(|n| {
            if n != 0.0 {
                let value = FxRate::parse(ryu_js::Buffer::new().format_finite(n.abs()))
                    .map_err(|_| precision())?;
                if journal_post::rate(&value)? != n.abs() {
                    return Err(precision());
                }
            }
            Ok(n)
        })
        .transpose()
}
fn refusal(message: String) -> Value {
    json!({"ok":false,"appliedRules":[journal_rules::APPEND_ONLY,journal_rules::REVERSAL],"errors":[message]})
}
/// No commit/open authority. Every Err and business rejection requires caller rollback.
pub fn reverse(
    tx: &Transaction<'_>,
    input: &Input,
    expected_version: &str,
    latest: Date,
    actor: Actor<'_>,
) -> Result<Value, Error> {
    journal_post::require_current(tx)?;
    let rules = crate::journal_preview::compiled_rules()?;
    rules.require_version(expected_version)?;
    rules.require_reversal()?;
    let id = input.entry_id.value();
    if id <= 0 || id > 9_007_199_254_740_991 {
        return Err(Error::new(
            "invalid_identifier",
            "entryId must be a positive safe integer",
        ));
    }
    let reason = journal_post::trim(&input.reason);
    if reason.is_empty() {
        return Ok(refusal("reason is required".to_owned()));
    }
    let original=tx.query_row("SELECT entry_no,reversal_of_entry_id,document_id,source_bank_transaction_id,currency,amount_foreign,amount_dkk,fx_rate_to_dkk,created_by_program FROM main.journal_entries WHERE id=?1",[id],|row| {
        Ok((row.get::<_,String>(0)?,row.get::<_,Option<i64>>(1)?,row.get::<_,Option<i64>>(2)?,row.get::<_,Option<i64>>(3)?,row.get::<_,Option<String>>(4)?,amount(row,5),amount(row,6),fx(row,7),row.get::<_,String>(8)?))
    }).optional().map_err(failed)?;
    let Some((
        original_no,
        original_reversal,
        document,
        bank,
        currency,
        foreign,
        dkk,
        rate,
        program,
    )) = original
    else {
        return Ok(refusal(format!("journal entry {id} does not exist")));
    };
    if original_reversal.is_some() {
        return Ok(refusal(format!(
            "journal entry {id} is already a reversal entry"
        )));
    }
    let accrual:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM accruals WHERE registration_journal_entry_id=?1) OR EXISTS(SELECT 1 FROM accrual_schedule_postings WHERE journal_entry_id=?1)",[id],|r|r.get(0)).map_err(failed)?;
    if accrual {
        return Ok(refusal(
            "accrual evidence requires an atomic accrual correction workflow".into(),
        ));
    }
    let asset:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM asset_depreciation_entries WHERE journal_entry_id=?1) OR EXISTS(SELECT 1 FROM asset_writeoffs WHERE journal_entry_id=?1) OR EXISTS(SELECT 1 FROM assets a JOIN journal_entries j ON j.document_id=a.purchase_document_id WHERE j.id=?1)",[id],|r|r.get(0)).map_err(failed)?;
    if asset {
        return Ok(refusal(
            "asset evidence requires an atomic asset correction workflow".into(),
        ));
    }
    let payable:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM payables WHERE journal_entry_id=?1) OR EXISTS(SELECT 1 FROM payable_payments WHERE journal_entry_id=?1)",[id],|r|r.get(0)).map_err(failed)?;
    if payable {
        return Ok(refusal(
            "payable evidence requires an atomic payable correction workflow".into(),
        ));
    }
    if let Some(document) = document {
        let protected:Option<String>=tx.query_row("SELECT document_type FROM main.documents WHERE id=?1 AND document_type IN ('issued_invoice','credit_note')",[document],|r|r.get(0)).optional().map_err(failed)?;
        if let Some(kind) = protected {
            return Ok(refusal(format!(
                "journal entry {id} is protected invoice evidence ({kind}) and cannot be reversed without an atomic invoice correction workflow"
            )));
        }
    }
    let prior: Option<String> = tx
        .query_row(
            "SELECT entry_no FROM main.journal_entries WHERE reversal_of_entry_id=?1 LIMIT 1",
            [id],
            |r| r.get(0),
        )
        .optional()
        .map_err(failed)?;
    if let Some(prior) = prior {
        return Ok(refusal(format!(
            "journal entry {id} already has reversal {prior}"
        )));
    }
    let mut statement=tx.prepare("SELECT a.account_no,jl.credit_amount,jl.debit_amount,jl.vat_code,jl.text FROM main.journal_lines jl JOIN main.accounts a ON a.id=jl.account_id WHERE jl.journal_entry_id=?1 ORDER BY jl.id").map_err(failed)?;
    let mut rows = statement.query([id]).map_err(failed)?;
    let mut lines = Vec::new();
    while let Some(row) = rows.next().map_err(failed)? {
        let text: Option<String> = row.get(4).map_err(failed)?;
        lines.push(JournalLine {
            account_no: AccountNo::parse(&row.get::<_, String>(0).map_err(failed)?)?,
            debit_amount: amount(row, 1)?.ok_or_else(precision)?,
            credit_amount: amount(row, 2)?.ok_or_else(precision)?,
            vat_code: row.get(3).map_err(failed)?,
            text: Some(match text.filter(|s| !s.is_empty()) {
                Some(text) => format!("REVERSAL: {text}"),
                None => format!("REVERSAL of {original_no}"),
            }),
        });
    }
    if lines.is_empty() {
        return Ok(refusal(format!(
            "journal entry {id} has no lines to reverse"
        )));
    }
    let historical = program == "rentemester-import-postings" && document.is_none();
    let raw_rate = rate?;
    let request = ManualJournal {
        transaction_date: input.transaction_date,
        text: format!("Reversal of {original_no}: {reason}"),
        currency: Currency::parse(
            &journal_post::trim(currency.as_deref().unwrap_or("DKK")).to_uppercase(),
        )?,
        document_id: document.map(DocumentId::from_sqlite),
        source_bank_transaction_id: bank.map(BankTransactionId::from_sqlite),
        amount_foreign: foreign?,
        amount_dkk: dkk?,
        fx_rate_to_dkk: raw_rate
            .filter(|n| *n > 0.0)
            .map(|n| FxRate::parse(ryu_js::Buffer::new().format_finite(n)))
            .transpose()?,
        created_by_program: historical.then(|| "rentemester-import-postings".to_owned()),
        lines,
    };
    journal_post::execute(
        tx,
        &request,
        expected_version,
        latest,
        actor,
        Some(&Reversal {
            id,
            number: original_no,
            original_program: program,
            document_id: request.document_id,
            raw_rate,
        }),
    )
}
#[cfg(test)]
mod tests;
