//! Receipts relieve the invoice's booked receivable, never today's debtor-role mapping.
use crate::{invalid, legacy_money};
use rentemester_engine::{
    Error,
    currency::Currency,
    fiscal_year::Date,
    identifiers::{AccountNo, BankTransactionId, DocumentId},
    manual_journal::{JournalLine, ManualJournal},
    money::Money,
    sales_invoice::{refused, wire},
};
use rusqlite::{OptionalExtension, Transaction, params};
use serde::Deserialize;
use serde_json::{Number, Value, json};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Input {
    pub invoice_document_id: i64,
    pub bank_transaction_id: Option<i64>,
    pub bank_transaction_reference: Option<String>,
    pub payment_date: Option<String>,
    pub amount: Option<Number>,
    pub confirm: bool,
}
pub fn settle(
    tx: &Transaction<'_>,
    input: &Input,
    subject: &str,
    latest: Date,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    if !input.confirm
        || input.invoice_document_id <= 0
        || input.bank_transaction_id.is_some() == input.bank_transaction_reference.is_some()
    {
        return Err(refused(
            "invoice, confirmation and exactly one bank transaction selector required",
        ));
    }
    let bank_id = if let Some(id) = input.bank_transaction_id {
        if id <= 0 {
            return Err(refused("positive bank transaction id required"));
        }
        id
    } else {
        let reference = input.bank_transaction_reference.as_deref().unwrap().trim();
        if reference.is_empty() || reference.len() > 1024 {
            return Err(refused("bank reference required"));
        }
        let mut q = tx
            .prepare("SELECT id FROM bank_transactions WHERE reference=?1 ORDER BY id LIMIT 2")
            .map_err(invalid)?;
        let ids = q
            .query_map([reference], |r| r.get::<_, i64>(0))
            .map_err(invalid)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(invalid)?;
        if ids.len() != 1 {
            return Err(refused(
                "bank reference must identify exactly one imported receipt; select a transaction id when ambiguous",
            ));
        }
        ids[0]
    };
    let linked:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM bank_journal_reconciliations WHERE bank_transaction_id=?1)",[bank_id],|r|r.get(0)).map_err(invalid)?;
    if linked {
        return Err(refused(
            "bank transaction is already linked to a journal entry",
        ));
    }
    let (date,amount,currency,mapping,account_id,account_currency):(String,Money,String,Option<String>,Option<i64>,Option<String>)=tx.query_row("SELECT bt.transaction_date,bt.amount,bt.currency,ba.ledger_account_no,bt.bank_account_id,ba.currency FROM bank_transactions bt LEFT JOIN bank_accounts ba ON ba.id=bt.bank_account_id WHERE bt.id=?1",[bank_id],|r|Ok((r.get(0)?,legacy_money(r.get_ref(1)?).map_err(|_|rusqlite::Error::InvalidQuery)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).optional().map_err(invalid)?.ok_or_else(||refused("imported bank receipt required"))?;
    if currency != "DKK" || amount.is_negative() || amount == Money::default() {
        return Err(refused("positive DKK customer receipt required"));
    }
    if account_id.is_some() && (mapping.is_none() || account_currency.as_deref() != Some("DKK")) {
        return Err(refused(
            "receipt bank account requires its own compatible ledger mapping",
        ));
    }
    let bank=match mapping{
        Some(account)=>account,
        None=>tx.query_row("SELECT account_no FROM account_role_mappings WHERE role='bank' AND status='confirmed'",[],|r|r.get(0)).optional().map_err(invalid)?.ok_or_else(||refused("bank account mapping or confirmed bank role required"))?,
    };
    let compatible:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM accounts WHERE account_no=?1 AND type='asset' AND normal_balance='debit' AND active=1)",[&bank],|r|r.get(0)).map_err(invalid)?;
    if !compatible {
        return Err(refused("active debit bank ledger account required"));
    }
    if input
        .amount
        .as_ref()
        .map(|n| Money::parse(&n.to_string()))
        .transpose()?
        .is_some_and(|specified| specified != amount)
    {
        return Err(refused(
            "settlement amount must equal the imported bank receipt",
        ));
    }
    if input
        .payment_date
        .as_deref()
        .is_some_and(|specified| specified != date)
    {
        return Err(refused(
            "native receipt settlement uses the imported bank transaction date",
        ));
    }
    let id = input.invoice_document_id;
    let adjusted:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM credit_note_postings WHERE original_invoice_document_id=?1) OR EXISTS(SELECT 1 FROM invoice_bad_debt_writeoffs WHERE invoice_document_id=?1)",[id],|r|r.get(0)).map_err(invalid)?;
    if adjusted {
        return Err(refused(
            "invoices with credit notes or bad-debt adjustments require their settlement flow to be ported first",
        ));
    }

    let (no,invoice_date,debtor_id,debtor,gross):(String,String,i64,String,Money)=tx.query_row("SELECT d.invoice_no,d.invoice_date,p.receivable_account_id,a.account_no,p.booked_gross_dkk FROM documents d JOIN issued_invoice_postings p ON p.invoice_document_id=d.id JOIN journal_entries j ON j.id=p.journal_entry_id JOIN accounts a ON a.id=p.receivable_account_id WHERE d.id=?1 AND d.document_type='issued_invoice' AND d.currency='DKK' AND j.status='posted' AND j.document_id=d.id AND j.transaction_date=d.invoice_date AND j.source_bank_transaction_id IS NULL AND NOT EXISTS(SELECT 1 FROM journal_entries reversal WHERE reversal.reversal_of_entry_id=j.id)",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,legacy_money(r.get_ref(4)?).map_err(|_|rusqlite::Error::InvalidQuery)?))).optional().map_err(invalid)?.ok_or_else(||refused("active posted DKK invoice required before settlement"))?;
    if bank == debtor || date < invoice_date {
        return Err(refused(
            "receipt must use a distinct bank account and cannot precede the invoice",
        ));
    }
    let mut paid = Money::default();
    let mut q=tx.prepare("SELECT p.amount,p.currency,j.status,EXISTS(SELECT 1 FROM journal_entries reversal WHERE reversal.reversal_of_entry_id=j.id) FROM invoice_payments p JOIN journal_entries j ON j.id=p.journal_entry_id WHERE p.invoice_document_id=?1").map_err(invalid)?;
    let mut rows = q.query([id]).map_err(invalid)?;
    while let Some(r) = rows.next().map_err(invalid)? {
        if r.get::<_, String>(1).map_err(invalid)? != "DKK"
            || r.get::<_, String>(2).map_err(invalid)? != "posted"
            || r.get::<_, bool>(3).map_err(invalid)?
        {
            return Err(refused(
                "previous invoice payment evidence requires resolution",
            ));
        }
        paid.add(&legacy_money(r.get_ref(0).map_err(invalid)?)?);
    }
    let open = gross.subtract(&paid);
    if open.subtract(&amount).is_negative() {
        return Err(refused(
            "receipt exceeds the invoice's outstanding principal; overpayments and claims require a separate flow",
        ));
    }
    let mut q=tx.prepare("SELECT jl.debit_amount,jl.credit_amount FROM journal_lines jl JOIN journal_entries j ON j.id=jl.journal_entry_id WHERE j.document_id=?1 AND jl.account_id=?2").map_err(invalid)?;
    let mut rows = q.query(params![id, debtor_id]).map_err(invalid)?;
    let mut carrying = Money::default();
    while let Some(r) = rows.next().map_err(invalid)? {
        carrying.add(
            &legacy_money(r.get_ref(0).map_err(invalid)?)?
                .subtract(&legacy_money(r.get_ref(1).map_err(invalid)?)?),
        );
    }
    if carrying != open {
        return Err(refused(
            "invoice outstanding principal does not match its booked receivable; review the linked entries",
        ));
    }
    let line = |account: &str, debit: Money, credit: Money| -> Result<JournalLine, Error> {
        Ok(JournalLine {
            account_no: AccountNo::parse(account)?,
            debit_amount: debit,
            credit_amount: credit,
            vat_code: None,
            text: Some(format!("Kundebetaling {no}")),
        })
    };
    let request = ManualJournal {
        transaction_date: Date::parse(&date)?,
        text: format!("Kundebetaling {no}"),
        currency: Currency::parse("DKK")?,
        document_id: Some(DocumentId::from_sqlite(id)),
        source_bank_transaction_id: Some(BankTransactionId::from_sqlite(bank_id)),
        amount_foreign: None,
        amount_dkk: None,
        fx_rate_to_dkk: None,
        created_by_program: None,
        lines: vec![
            line(&bank, amount.clone(), Money::default())?,
            line(&debtor, Money::default(), amount.clone())?,
        ],
    };
    let result = crate::journal_post::post(
        tx,
        &request,
        &crate::journal_preview::manual_journal_rule_version()?,
        latest,
        crate::journal_post::Actor {
            subject: &format!("user:{subject}"),
            program: "rentemester-rust",
        },
    )?;
    if result["ok"] != true {
        return Err(refused(result["errors"].to_string()));
    }
    let payment:i64=tx.query_row("INSERT INTO invoice_payments(invoice_document_id,bank_transaction_id,journal_entry_id,payment_date,amount,currency,note) VALUES(?1,?2,?3,?4,?5,'DKK',?6) RETURNING id",params![id,bank_id,result["entryId"].as_i64(),date,wire(&amount)?.as_f64(),format!("Bank settlement from transaction {bank_id}")],|r|r.get(0)).map_err(invalid)?;
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES('invoice_payment_apply','invoice_payment',?1,?2,?3)",params![payment.to_string(),format!("Applied receipt {} to invoice {no}",amount.decimal()),format!("user:{subject}")]).map_err(invalid)?;
    Ok(
        json!({"entryId":result["entryId"],"paymentId":payment,"principalAmount":wire(&amount)?,"claimAmount":0,"invoiceNumber":no,"openBalance":wire(&open.subtract(&amount))?}),
    )
}
