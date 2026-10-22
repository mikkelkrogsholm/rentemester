//! Original payment applications. Every Err requires rollback of the held transaction.
use crate::{idempotency::PrincipalKind, invalid};
use rentemester_engine::{Error, fiscal_year::Date, money::Money, sales_invoice::wire};
use rusqlite::{OptionalExtension, Transaction, params};
use serde::Deserialize;
use serde_json::{Number, Value, json};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Input {
    pub invoice_document_id: i64,
    pub payment_date: String,
    pub amount: Number,
    pub bank_transaction_id: Option<i64>,
    pub journal_entry_id: Option<i64>,
    pub bank_account_no: Option<String>,
    pub receivable_account_no: Option<String>,
    pub note: Option<String>,
}
fn refused(message: impl Into<String>) -> Error {
    Error::new("invoice_payment_refused", message)
}
fn require(ok: bool, message: &str) -> Result<(), Error> {
    if ok { Ok(()) } else { Err(refused(message)) }
}
fn audit_current(tx: &Transaction<'_>, id: i64) -> Result<(), Error> {
    crate::audit_invoice::payment_evidence(tx, id)
}

fn current_open(tx: &Transaction<'_>, id: i64, today: &str) -> Result<Money, Error> {
    let view = crate::invoice_original::read(
        tx,
        crate::invoice_original::Kind::Status,
        &crate::invoice_original::Query {
            document_id: Some(id),
            ..Default::default()
        },
        today,
    )?;
    require(view["ok"] == true, "current invoice status unavailable")?;
    crate::invoice_fx::amount(&view["data"]["openBalance"])
}
fn available(tx: &Transaction<'_>, journal: i64, bank: Option<i64>) -> Result<(), Error> {
    let used:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM (SELECT journal_entry_id,bank_transaction_id FROM invoice_payments UNION ALL SELECT journal_entry_id,bank_transaction_id FROM invoice_refunds UNION ALL SELECT journal_entry_id,bank_transaction_id FROM invoice_claim_payments) WHERE journal_entry_id=?1 OR (?2 IS NOT NULL AND bank_transaction_id=?2))",params![journal,bank],|r|r.get(0)).map_err(invalid)?;
    require(!used, "journal or bank transaction is already applied")
}
fn insert(
    tx: &Transaction<'_>,
    i: &Input,
    journal: i64,
    subject: &str,
    principal: PrincipalKind,
    amount: &Money,
) -> Result<i64, Error> {
    let id:i64=tx.query_row("INSERT INTO invoice_payments(invoice_document_id,bank_transaction_id,journal_entry_id,payment_date,amount,currency,note) VALUES(?1,?2,?3,?4,?5,'DKK',?6) RETURNING id",params![i.invoice_document_id,i.bank_transaction_id,journal,i.payment_date,wire(amount)?.as_f64(),i.note],|r|r.get(0)).map_err(invalid)?;
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor)VALUES('invoice_payment_apply','invoice_payment',?1,?2,?3)",params![id.to_string(),format!("Applied payment {} to invoice document {}",amount.decimal(),i.invoice_document_id),format!("{}:{subject}",match principal{PrincipalKind::User=>"user",PrincipalKind::ServiceAccount=>"service_account"})]).map_err(invalid)?;
    Ok(id)
}
fn candidate(
    tx: &Transaction<'_>,
    i: &Input,
    journal: i64,
    debtor: i64,
    amount: &Money,
) -> Result<(), Error> {
    available(tx, journal, i.bank_transaction_id)?;
    let matches:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM journal_entries j WHERE j.id=?1 AND j.document_id=?2 AND j.source_bank_transaction_id IS ?3 AND j.transaction_date=?4 AND j.currency='DKK' AND j.status='posted' AND j.reversal_of_entry_id IS NULL AND NOT EXISTS(SELECT 1 FROM journal_entries r WHERE r.reversal_of_entry_id=j.id) AND (SELECT count(*) FROM journal_lines l WHERE l.journal_entry_id=j.id)=2 AND NOT EXISTS(SELECT 1 FROM journal_lines l WHERE l.journal_entry_id=j.id AND l.vat_code IS NOT NULL))",params![journal,i.invoice_document_id,i.bank_transaction_id,i.payment_date],|r|r.get(0)).map_err(invalid)?;
    require(
        matches,
        "candidate must be an exact active DKK invoice receipt with matching date and nullable bank link",
    )?;
    if i.bank_transaction_id.is_none() {
        let actual = crate::audit_invoice::bankless_payment_journal(
            tx,
            journal,
            i.invoice_document_id,
            debtor,
            &i.payment_date,
            amount,
        )?;
        require(
            i.bank_account_no
                .as_deref()
                .is_none_or(|s| s.trim() == actual),
            "bankAccountNo differs from historical bank role receipt",
        )?;
    } else {
        // The complete cash audit after insertion authenticates the concrete bank and exact effects.
        let exact:i64=tx.query_row("SELECT count(*) FROM journal_lines WHERE journal_entry_id=?1 AND account_id=?2 AND debit_amount=0 AND credit_amount=?3",params![journal,debtor,wire(amount)?.as_f64()],|r|r.get(0)).map_err(invalid)?;
        require(
            exact == 1,
            "candidate does not relieve the exact original booked debtor amount",
        )?;
    }
    Ok(())
}
/// Current source/authentication is supplied by the ordinary lifecycle owner; all accounting remains native.
pub fn apply(
    tx: &Transaction<'_>,
    i: &Input,
    subject: &str,
    principal: PrincipalKind,
    latest: Date,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    require(i.invoice_document_id > 0, "invoice required")?;
    let date = Date::parse(&i.payment_date)?;
    let mut periods=tx.prepare("SELECT id,status FROM accounting_periods WHERE period_start<=?1 AND period_end>=?1 AND status IN ('closed','reported') ORDER BY period_end,id").map_err(invalid)?;
    for p in periods
        .query_map([&i.payment_date], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(invalid)?
    {
        let (id, status) = p.map_err(invalid)?;
        if crate::periods::effective(tx, id, status)? != "open" {
            return Err(Error::new(
                "invoice_payment_period_locked",
                "payment classification cannot change a closed or reported period; reopen through the native period workflow",
            ));
        }
    }
    let amount = crate::invoice_fx::amount(&json!(i.amount))?;
    require(
        !amount.is_negative() && amount != Money::default(),
        "payment amount must be positive exact cents",
    )?;
    let row:Option<(String,String,String,i64,String)>=tx.query_row("SELECT d.invoice_no,d.invoice_date,d.currency,p.receivable_account_id,a.account_no FROM documents d JOIN issued_invoice_postings p ON p.invoice_document_id=d.id JOIN accounts a ON a.id=p.receivable_account_id JOIN journal_entries j ON j.id=p.journal_entry_id WHERE d.id=?1 AND d.document_type='issued_invoice' AND j.status='posted' AND j.reversal_of_entry_id IS NULL AND NOT EXISTS(SELECT 1 FROM journal_entries r WHERE r.reversal_of_entry_id=j.id)",[i.invoice_document_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional().map_err(invalid)?;
    let (number, invoice_date, currency, debtor, debtor_no) =
        row.ok_or_else(|| refused("active booked invoice required before payment"))?;
    require(
        date >= Date::parse(&invoice_date)?,
        "payment cannot precede invoice date",
    )?;
    require(
        i.receivable_account_no
            .as_deref()
            .is_none_or(|s| s.trim() == debtor_no),
        "receivableAccountNo differs from original booked debtor",
    )?;
    require(
        currency == "DKK" || i.bank_transaction_id.is_some() && i.journal_entry_id.is_none(),
        "foreign payments require a real bank and cannot reuse a supplied journal",
    )?;
    if let Some(bank) = i.bank_transaction_id {
        crate::invoice_lifecycle::assert_accounts(
            tx,
            &json!({"bankAccountNo":i.bank_account_no,"receivableAccountNo":i.receivable_account_no}),
            i.invoice_document_id,
            bank,
        )?;
    }
    let mut fx_realized = false;
    let (payment, journal) = if let Some(journal) = i.journal_entry_id {
        require(
            currency == "DKK",
            "only DKK can classify an existing journal",
        )?;
        candidate(tx, i, journal, debtor, &amount)?;
        // This candidate already affects carrying. Classify it tentatively, then run complete evidence
        // and carrying validation; any error is propagated to the owner for transaction rollback.
        (
            insert(tx, i, journal, subject, principal, &amount)?,
            journal,
        )
    } else if let Some(bank) = i.bank_transaction_id {
        let r = crate::invoice_settlement::apply_with_principal(
            tx,
            &crate::invoice_settlement::Input {
                invoice_document_id: i.invoice_document_id,
                bank_transaction_id: Some(bank),
                bank_transaction_reference: None,
                payment_date: Some(i.payment_date.clone()),
                amount: Some(i.amount.clone()),
                confirm: true,
            },
            subject,
            latest,
            principal,
            i.note.as_deref(),
        )?;
        fx_realized = crate::invoice_fx::amount(&r["realizedFxAmountDkk"])? != Money::default();
        let payment = r["paymentId"]
            .as_i64()
            .ok_or_else(|| refused("native payment receipt unavailable"))?;
        (
            payment,
            r["entryId"]
                .as_i64()
                .ok_or_else(|| refused("native journal receipt unavailable"))?,
        )
    } else {
        audit_current(tx, i.invoice_document_id)?;
        let open = current_open(tx, i.invoice_document_id, &latest.to_string())?;
        require(
            !open.subtract(&amount).is_negative(),
            "payment exceeds current open principal",
        )?;
        let bank:String=tx.query_row("SELECT a.account_no FROM account_role_mappings r JOIN accounts a ON a.account_no=r.account_no WHERE r.role='bank' AND r.status='confirmed' AND a.active=1 AND a.type='asset' AND a.normal_balance='debit'",[],|r|r.get(0)).optional().map_err(invalid)?.ok_or_else(||refused("confirmed active compatible bank role required"))?;
        require(
            bank != debtor_no
                && i.bank_account_no
                    .as_deref()
                    .is_none_or(|s| s.trim() == bank),
            "bankAccountNo must agree with the confirmed historical bank role",
        )?;
        let request:rentemester_engine::manual_journal::ManualJournal=serde_json::from_value(json!({"transactionDate":i.payment_date,"text":format!("Manual invoice payment for invoice {number}"),"currency":"DKK","documentId":i.invoice_document_id.to_string(),"lines":[{"accountNo":bank,"debitAmount":amount.decimal()},{"accountNo":debtor_no,"creditAmount":amount.decimal()}]})).map_err(invalid)?;
        let r = crate::journal_post::post(
            tx,
            &request,
            &crate::journal_preview::manual_journal_rule_version()?,
            latest,
            crate::journal_post::Actor {
                subject: &format!("user:{subject}"),
                program: match principal {
                    PrincipalKind::User => "rentemester-rust",
                    PrincipalKind::ServiceAccount => "service-principal",
                },
            },
        )?;
        require(r["ok"] == true, "native payment posting refused")?;
        let journal = r["entryId"]
            .as_i64()
            .ok_or_else(|| refused("native journal receipt unavailable"))?;
        candidate(tx, i, journal, debtor, &amount)?;
        (
            insert(tx, i, journal, subject, principal, &amount)?,
            journal,
        )
    };
    audit_current(tx, i.invoice_document_id)?;
    let open = current_open(tx, i.invoice_document_id, &latest.to_string())?;
    require(
        !open.is_negative(),
        "payment exceeds current open principal",
    )?;
    let mut rules = vec![
        "DK-INVOICE-PAYMENT-001",
        "DK-INVOICE-CORRECTION-BALANCE-001",
    ];
    if fx_realized {
        rules.push("DK-INVOICE-FX-REALISED-001");
    }
    Ok(
        json!({"paymentId":payment,"journalEntryId":journal,"invoiceDocumentId":i.invoice_document_id,"invoiceNumber":number,"openBalance":wire(&open)?,"appliedRules":rules}),
    )
}
#[cfg(test)]
mod tests;
