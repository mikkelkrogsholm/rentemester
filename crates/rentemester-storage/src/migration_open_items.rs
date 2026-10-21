//! Evidenced settlement of immutable imported control balances. No journal posting.
use crate::{bank::signed_money, invalid, journal_post, legacy_money};
use rentemester_engine::{Error, fiscal_year::Date, money::Money};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Apply {
    pub item_id: i64,
    pub amount: Money,
    pub application_date: String,
    pub source_journal_entry_id: i64,
    pub bank_transaction_id: Option<i64>,
    pub note: Option<String>,
}
fn refused(message: &str) -> Error {
    Error::new("migration_open_item_refused", message)
}
fn positive_id(id: i64) -> Result<(), Error> {
    if !(1..=9_007_199_254_740_991).contains(&id) {
        return Err(refused("positive exact evidence identifier required"));
    }
    Ok(())
}
fn applied(db: &Connection, item: i64) -> Result<Money, Error> {
    let mut q = db
        .prepare("SELECT amount FROM migration_open_item_applications WHERE item_id=?1 ORDER BY id")
        .map_err(invalid)?;
    let mut rs = q.query([item]).map_err(invalid)?;
    let mut sum = Money::default();
    while let Some(r) = rs.next().map_err(invalid)? {
        sum.add(&legacy_money(r.get_ref(0).map_err(invalid)?)?);
    }
    Ok(sum)
}
/// Caller owns authorization and an immediate company write transaction. Every
/// error requires rollback. Only one application is inserted; source evidence,
/// imported batches/items and posted journals are never changed.
pub fn apply(tx: &Transaction<'_>, input: &Apply) -> Result<Value, Error> {
    journal_post::require_current(tx)?;
    positive_id(input.item_id)?;
    positive_id(input.source_journal_entry_id)?;
    if input.amount.is_negative() || input.amount == Money::default() {
        return Err(refused(
            "migration open-item application needs a positive amount",
        ));
    }
    Date::parse(&input.application_date)?;
    let numeric_amount = journal_post::money(&input.amount)?;
    let mut q = tx.prepare("SELECT b.kind,b.control_account_no,i.open_amount_at_import FROM migration_open_items i JOIN migration_open_item_batches b ON b.id=i.batch_id WHERE i.id=?1").map_err(invalid)?;
    let mut rs = q.query([input.item_id]).map_err(invalid)?;
    let r = rs
        .next()
        .map_err(invalid)?
        .ok_or_else(|| refused("migration open-item application references an unknown item"))?;
    let kind: String = r.get(0).map_err(invalid)?;
    let control: String = r.get(1).map_err(invalid)?;
    let open = legacy_money(r.get_ref(2).map_err(invalid)?)?;
    let settlement = match kind.as_str() {
        "receivable" => Money::default().subtract(&input.amount),
        "payable" => input.amount.clone(),
        _ => return Err(refused("invalid migration open item kind")),
    };
    let bank_link: Option<Option<i64>> = tx.query_row("SELECT source_bank_transaction_id FROM journal_entries WHERE id=?1 AND status='posted'",[input.source_journal_entry_id],|r|r.get(0)).optional().map_err(invalid)?;
    let bank_link = bank_link.ok_or_else(|| {
        refused("migration open-item application journal is unknown or non-posted")
    })?;
    let mut jq = tx.prepare("SELECT l.debit_amount,l.credit_amount FROM journal_lines l JOIN accounts a ON a.id=l.account_id WHERE l.journal_entry_id=?1 AND a.account_no=?2 ORDER BY l.id").map_err(invalid)?;
    let mut lines = jq
        .query(params![input.source_journal_entry_id, control])
        .map_err(invalid)?;
    let mut effect = Money::default();
    while let Some(r) = lines.next().map_err(invalid)? {
        effect.add(&legacy_money(r.get_ref(0).map_err(invalid)?)?);
        effect = effect.subtract(&legacy_money(r.get_ref(1).map_err(invalid)?)?);
    }
    if effect != settlement {
        return Err(refused(
            "migration open-item application journal has the wrong control-account effect",
        ));
    }
    let reused: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM migration_open_item_applications WHERE source_journal_entry_id=?1)",[input.source_journal_entry_id],|r|r.get(0)).map_err(invalid)?;
    if reused {
        return Err(refused(
            "migration open-item application journal evidence is already used",
        ));
    }
    if let Some(bank) = input.bank_transaction_id {
        positive_id(bank)?;
        let mut bq = tx
            .prepare("SELECT COALESCE(amount_dkk,amount) FROM bank_transactions WHERE id=?1")
            .map_err(invalid)?;
        let mut banks = bq.query([bank]).map_err(invalid)?;
        let row = banks.next().map_err(invalid)?.ok_or_else(|| {
            refused("migration open-item application references an unknown bank transaction")
        })?;
        if signed_money(row.get_ref(0).map_err(invalid)?)? != Money::default().subtract(&settlement)
        {
            return Err(refused(
                "migration open-item application bank amount has the wrong signed amount",
            ));
        }
        if bank_link != Some(bank) {
            return Err(refused(
                "migration open-item application journal is not linked to its bank evidence",
            ));
        }
        let used: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM migration_open_item_applications WHERE bank_transaction_id=?1)",[bank],|r|r.get(0)).map_err(invalid)?;
        if used {
            return Err(refused(
                "migration open-item application bank evidence is already used",
            ));
        }
    }
    let mut total = applied(tx, input.item_id)?;
    total.add(&input.amount);
    if open.subtract(&total).is_negative() {
        return Err(refused(
            "migration open-item application would over-apply the open balance",
        ));
    }
    let note = input
        .note
        .as_deref()
        .map(journal_post::trim)
        .filter(|n| !n.is_empty());
    let application:i64 = tx.query_row("INSERT INTO migration_open_item_applications(item_id,amount,application_date,source_journal_entry_id,bank_transaction_id,note) VALUES(?1,?2,?3,?4,?5,?6) RETURNING id",params![input.item_id,numeric_amount,input.application_date,input.source_journal_entry_id,input.bank_transaction_id,note],|r|r.get(0)).map_err(invalid)?;
    Ok(json!({"ok":true,"itemId":input.item_id,"applicationId":application,"errors":[]}))
}
/// Derived read only balances, preserving source/resolution and each application.
/// Exact amounts use the native canonical decimal string representation.
pub fn read(db: &Connection, batch_id: Option<i64>) -> Result<Value, Error> {
    if let Some(id) = batch_id {
        positive_id(id)?;
    }
    let mut q = db.prepare("SELECT b.id,i.id,b.kind,b.control_account_no,i.external_ref,i.counterparty_name,i.issue_date,i.due_date,i.original_amount,i.open_amount_at_import,i.document_id,i.source_recognition_journal_entry_id,i.source_kind,i.resolution_status FROM migration_open_items i JOIN migration_open_item_batches b ON b.id=i.batch_id WHERE (?1 IS NULL OR b.id=?1) ORDER BY b.id,i.id").map_err(invalid)?;
    let mut rs = q.query([batch_id]).map_err(invalid)?;
    let mut rows = Vec::new();
    while let Some(r) = rs.next().map_err(invalid)? {
        let item: i64 = r.get(1).map_err(invalid)?;
        let open = legacy_money(r.get_ref(9).map_err(invalid)?)?;
        let total = applied(db, item)?;
        let balance = open.subtract(&total);
        let mut aq = db.prepare("SELECT id,amount,application_date,source_journal_entry_id,bank_transaction_id,note FROM migration_open_item_applications WHERE item_id=?1 ORDER BY id").map_err(invalid)?;
        let mut ars = aq.query([item]).map_err(invalid)?;
        let mut apps = Vec::new();
        while let Some(a) = ars.next().map_err(invalid)? {
            apps.push(json!({"applicationId":a.get::<_,i64>(0).map_err(invalid)?,"amount":legacy_money(a.get_ref(1).map_err(invalid)?)?,"applicationDate":a.get::<_,String>(2).map_err(invalid)?,"sourceJournalEntryId":a.get::<_,Option<i64>>(3).map_err(invalid)?,"bankTransactionId":a.get::<_,Option<i64>>(4).map_err(invalid)?,"note":a.get::<_,Option<String>>(5).map_err(invalid)?}));
        }
        rows.push(json!({"batchId":r.get::<_,i64>(0).map_err(invalid)?,"itemId":item,"kind":r.get::<_,String>(2).map_err(invalid)?,"controlAccountNo":r.get::<_,String>(3).map_err(invalid)?,"externalRef":r.get::<_,String>(4).map_err(invalid)?,"counterpartyName":r.get::<_,Option<String>>(5).map_err(invalid)?,"issueDate":r.get::<_,Option<String>>(6).map_err(invalid)?,"dueDate":r.get::<_,Option<String>>(7).map_err(invalid)?,"originalAmount":legacy_money(r.get_ref(8).map_err(invalid)?)?,"openAmountAtImport":open,"appliedAmount":total,"openBalance":balance,"status":if balance==Money::default(){"settled"}else{"open"},"documentId":r.get::<_,Option<i64>>(10).map_err(invalid)?,"sourceRecognitionJournalEntryId":r.get::<_,Option<i64>>(11).map_err(invalid)?,"sourceKind":r.get::<_,String>(12).map_err(invalid)?,"resolutionStatus":r.get::<_,String>(13).map_err(invalid)?,"applications":apps}));
    }
    Ok(json!({"ok":true,"rows":rows}))
}
#[cfg(test)]
mod tests;
