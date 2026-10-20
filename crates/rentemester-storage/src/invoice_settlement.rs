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
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RefundInput {
    pub invoice_document_id: i64,
    pub bank_transaction_id: Option<i64>,
    pub bank_transaction_reference: Option<String>,
    pub refund_date: Option<String>,
    pub amount: Option<Number>,
    pub confirm: bool,
}
pub(crate) fn refunded(tx: &Transaction<'_>, id: i64) -> Result<Money, Error> {
    let currency: String = tx
        .query_row("SELECT currency FROM documents WHERE id=?1", [id], |r| {
            r.get(0)
        })
        .map_err(invalid)?;
    crate::invoice_fx::movements(tx, id, &currency, true)
}

pub fn refund(
    tx: &Transaction<'_>,
    input: &RefundInput,
    subject: &str,
    latest: Date,
) -> Result<Value, Error> {
    let input = Input {
        invoice_document_id: input.invoice_document_id,
        bank_transaction_id: input.bank_transaction_id,
        bank_transaction_reference: input.bank_transaction_reference.clone(),
        payment_date: input.refund_date.clone(),
        amount: input.amount.clone(),
        confirm: input.confirm,
    };
    transfer(tx, &input, subject, latest, true)
}
pub fn settle(
    tx: &Transaction<'_>,
    input: &Input,
    subject: &str,
    latest: Date,
) -> Result<Value, Error> {
    transfer(tx, input, subject, latest, false)
}
fn transfer(
    tx: &Transaction<'_>,
    input: &Input,
    subject: &str,
    latest: Date,
    refund: bool,
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
    let (date,amount,currency,mapping,account_id,account_currency):(String,Money,String,Option<String>,Option<i64>,Option<String>)=tx.query_row("SELECT bt.transaction_date,bt.amount,bt.currency,ba.ledger_account_no,bt.bank_account_id,ba.currency FROM bank_transactions bt LEFT JOIN bank_accounts ba ON ba.id=bt.bank_account_id WHERE bt.id=?1",[bank_id],|r|Ok((r.get(0)?,crate::bank::signed_money(r.get_ref(1)?).map_err(|_|rusqlite::Error::InvalidQuery)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).optional().map_err(invalid)?.ok_or_else(||refused("imported bank receipt required"))?;
    if amount == Money::default() || amount.is_negative() != refund {
        return Err(refused(if refund {
            "outgoing customer refund required"
        } else {
            "positive customer receipt required"
        }));
    }
    let amount = if refund {
        Money::default().subtract(&amount)
    } else {
        amount
    };
    if account_id.is_some()
        && (mapping.is_none() || account_currency.as_deref() != Some(currency.as_str()))
    {
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
            "amount must equal the absolute imported bank transaction amount",
        ));
    }
    if input
        .payment_date
        .as_deref()
        .is_some_and(|specified| specified != date)
    {
        return Err(refused(
            "native invoice cash movement uses the imported bank transaction date",
        ));
    }
    let id = input.invoice_document_id;
    let adjusted: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM invoice_bad_debt_writeoffs WHERE invoice_document_id=?1)",
            [id],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    if adjusted {
        return Err(refused(
            "invoices with bad-debt adjustments require their settlement flow to be ported first",
        ));
    }

    let (no,invoice_date,debtor_id,debtor,gross_dkk,invoice_currency,payload):(String,String,i64,String,Money,String,String)=tx.query_row("SELECT d.invoice_no,d.invoice_date,p.receivable_account_id,a.account_no,p.booked_gross_dkk,d.currency,d.payload_json FROM documents d JOIN issued_invoice_postings p ON p.invoice_document_id=d.id JOIN journal_entries j ON j.id=p.journal_entry_id JOIN accounts a ON a.id=p.receivable_account_id WHERE d.id=?1 AND d.document_type='issued_invoice' AND j.status='posted' AND j.document_id=d.id AND j.transaction_date=d.invoice_date AND j.source_bank_transaction_id IS NULL AND NOT EXISTS(SELECT 1 FROM journal_entries reversal WHERE reversal.reversal_of_entry_id=j.id)",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,legacy_money(r.get_ref(4)?).map_err(|_|rusqlite::Error::InvalidQuery)?,r.get(5)?,r.get(6)?))).optional().map_err(invalid)?.ok_or_else(||refused("active posted invoice required before settlement"))?;
    if bank == debtor || date < invoice_date {
        return Err(refused(
            "receipt must use a distinct bank account and cannot precede the invoice",
        ));
    }
    if currency != invoice_currency {
        return Err(refused(
            "imported cash movement must use the invoice currency",
        ));
    }
    let payload: Value = serde_json::from_str(&payload).map_err(invalid)?;
    let original = rentemester_engine::sales_invoice::project(&payload)?;
    if crate::invoice_fx::projection(&payload)?.gross != gross_dkk {
        return Err(refused("invoice frozen DKK basis differs from posting"));
    }
    let gross = original.gross;
    let paid = crate::invoice_fx::movements(tx, id, &currency, false)?;
    let refunded = refunded(tx, id)?;
    let mut open = gross
        .subtract(&crate::credit_note::credited(tx, id)?)
        .subtract(&paid);
    open.add(&refunded);
    let available = if refund {
        Money::default().subtract(&open)
    } else {
        open.clone()
    };
    if available.is_negative()
        || available == Money::default()
        || available.subtract(&amount).is_negative()
    {
        return Err(refused(if refund {
            "refund exceeds the invoice's refundable credit balance"
        } else {
            "receipt exceeds the invoice's outstanding principal; overpayments and claims require a separate flow"
        }));
    }
    let carrying = principal_carrying(tx, id, debtor_id)?;
    let basis = |a: &Money| -> Result<Money, Error> {
        if *a == Money::default() {
            Ok(Money::default())
        } else {
            crate::invoice_fx::portion(&gross, &gross_dkk, &Money::default(), a)
        }
    };
    let mut expected = gross_dkk
        .subtract(&crate::invoice_fx::credited_dkk(tx, id)?)
        .subtract(&basis(&paid)?);
    expected.add(&basis(&refunded)?);
    if carrying != expected {
        return Err(refused(
            "invoice outstanding principal does not match its booked receivable; review the linked entries",
        ));
    }
    let before = if refund { &refunded } else { &paid };
    let principal = crate::invoice_fx::portion(&gross, &gross_dkk, before, &amount)?;
    let (bank_dkk, bank_fx) = if currency == "DKK" {
        (amount.clone(), None)
    } else {
        let (dkk, rate): (Option<Money>, Option<String>) = tx
            .query_row(
                "SELECT amount_dkk,CAST(fx_rate_to_dkk AS TEXT) FROM bank_transactions WHERE id=?1",
                [bank_id],
                |r| {
                    Ok((
                        if matches!(r.get_ref(0)?, rusqlite::types::ValueRef::Null) {
                            None
                        } else {
                            Some(
                                crate::bank::signed_money(r.get_ref(0)?)
                                    .map_err(|_| rusqlite::Error::InvalidQuery)?,
                            )
                        },
                        r.get(1)?,
                    ))
                },
            )
            .map_err(invalid)?;
        let signed =
            dkk.ok_or_else(|| refused("foreign bank movement requires explicit DKK valuation"))?;
        let rate = rentemester_engine::fx::FxRate::parse(
            &rate.ok_or_else(|| refused("foreign bank movement requires explicit FX rate"))?,
        )?;
        if signed.is_negative() != refund {
            return Err(refused("foreign DKK bank valuation has wrong direction"));
        }
        let absolute = if refund {
            Money::default().subtract(&signed)
        } else {
            signed
        };
        if absolute == Money::default() || rate.apply(&amount) != absolute {
            return Err(refused(
                "foreign bank DKK valuation contradicts imported FX rate",
            ));
        }
        (absolute, Some(rate))
    };
    let difference = if refund {
        principal.subtract(&bank_dkk)
    } else {
        bank_dkk.subtract(&principal)
    };
    let fx_account = if difference != Money::default() {
        let role = if difference.is_negative() {
            "fx_loss"
        } else {
            "fx_gain"
        };
        let account = crate::native_fx_roles::resolve(tx, role)?.map(|record| record.0);
        let account = account
            .ok_or_else(|| refused("confirmed compatible realized FX account role required"))?;
        if account == bank || account == debtor {
            return Err(refused(
                "realized FX account must be distinct from bank and debtor",
            ));
        }
        Some(account)
    } else {
        None
    };
    let line = |account: &str, debit: Money, credit: Money| -> Result<JournalLine, Error> {
        Ok(JournalLine {
            account_no: AccountNo::parse(account)?,
            debit_amount: debit,
            credit_amount: credit,
            vat_code: None,
            text: Some(format!(
                "{} {no}",
                if refund {
                    "Tilbagebetaling"
                } else {
                    "Kundebetaling"
                }
            )),
        })
    };
    let mut lines = if refund {
        vec![
            line(&debtor, principal.clone(), Money::default())?,
            line(&bank, Money::default(), bank_dkk.clone())?,
        ]
    } else {
        vec![
            line(&bank, bank_dkk.clone(), Money::default())?,
            line(&debtor, Money::default(), principal.clone())?,
        ]
    };
    if let Some(account) = fx_account {
        lines.push(if difference.is_negative() {
            line(
                &account,
                Money::default().subtract(&difference),
                Money::default(),
            )?
        } else {
            line(&account, Money::default(), difference.clone())?
        });
    }
    let request = ManualJournal {
        transaction_date: Date::parse(&date)?,
        text: format!(
            "{} {no}",
            if refund {
                "Tilbagebetaling"
            } else {
                "Kundebetaling"
            }
        ),
        currency: Currency::parse(&currency)?,
        document_id: Some(DocumentId::from_sqlite(id)),
        source_bank_transaction_id: Some(BankTransactionId::from_sqlite(bank_id)),
        amount_foreign: (currency != "DKK").then_some(amount.clone()),
        amount_dkk: (currency != "DKK").then_some(bank_dkk.clone()),
        fx_rate_to_dkk: bank_fx,
        created_by_program: None,
        lines,
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
    if refund {
        let id:i64=tx.query_row("INSERT INTO invoice_refunds(invoice_document_id,bank_transaction_id,journal_entry_id,refund_date,amount,currency,note) VALUES(?1,?2,?3,?4,?5,?7,?6) RETURNING id",params![id,bank_id,result["entryId"].as_i64(),date,wire(&amount)?.as_f64(),format!("Customer refund from transaction {bank_id}"),currency],|r|r.get(0)).map_err(invalid)?;
        tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES('invoice_refund_apply','invoice_refund',?1,?2,?3)",params![id.to_string(),format!("Applied refund {} to invoice {no}",amount.decimal()),format!("user:{subject}")]).map_err(invalid)?;
        return Ok(
            json!({"entryId":result["entryId"],"refundId":id,"invoiceNumber":no,"remainingCreditBalance":wire(&available.subtract(&amount))?,"currency":currency,"principalAmountDkk":wire(&principal)?,"bankAmountDkk":wire(&bank_dkk)?,"realizedFxAmountDkk":wire(&difference)?}),
        );
    }
    let payment:i64=tx.query_row("INSERT INTO invoice_payments(invoice_document_id,bank_transaction_id,journal_entry_id,payment_date,amount,currency,note) VALUES(?1,?2,?3,?4,?5,?7,?6) RETURNING id",params![id,bank_id,result["entryId"].as_i64(),date,wire(&amount)?.as_f64(),format!("Bank settlement from transaction {bank_id}"),currency],|r|r.get(0)).map_err(invalid)?;
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES('invoice_payment_apply','invoice_payment',?1,?2,?3)",params![payment.to_string(),format!("Applied receipt {} to invoice {no}",amount.decimal()),format!("user:{subject}")]).map_err(invalid)?;
    Ok(
        json!({"entryId":result["entryId"],"paymentId":payment,"principalAmount":wire(&amount)?,"claimAmount":0,"invoiceNumber":no,"openBalance":wire(&open.subtract(&amount))?,"currency":currency,"principalAmountDkk":wire(&principal)?,"bankAmountDkk":wire(&bank_dkk)?,"realizedFxAmountDkk":wire(&difference)?}),
    )
}

/// Same authoritative carrying check for principal transfers and reminder-claim review/settlement.
/// Only canonical related claim journals may be omitted from the principal projection.
pub(crate) fn principal_carrying(
    tx: &Transaction<'_>,
    id: i64,
    debtor_id: i64,
) -> Result<Money, Error> {
    let mut q=tx.prepare("SELECT j.id,jl.debit_amount,jl.credit_amount FROM journal_lines jl JOIN journal_entries j ON j.id=jl.journal_entry_id WHERE (j.document_id=?1 OR j.id IN (SELECT journal_entry_id FROM credit_note_postings WHERE original_invoice_document_id=?1)) AND jl.account_id=?2").map_err(invalid)?;
    let mut rows = q.query(params![id, debtor_id]).map_err(invalid)?;
    let mut carrying = Money::default();
    let claims = crate::reminder_claims::validated_claims(tx, id)?;
    while let Some(r) = rows.next().map_err(invalid)? {
        if claims
            .journals
            .contains(&r.get::<_, i64>(0).map_err(invalid)?)
        {
            continue;
        }
        carrying.add(
            &legacy_money(r.get_ref(1).map_err(invalid)?)?
                .subtract(&legacy_money(r.get_ref(2).map_err(invalid)?)?),
        );
    }
    Ok(carrying)
}
