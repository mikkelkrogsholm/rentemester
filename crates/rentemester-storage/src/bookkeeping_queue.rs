//! A finite bank-to-draft queue; deliberately no multi-item batch apply or invented final checks.
use crate::{
    accounting_drafts::{self, refused},
    invalid,
};
use rentemester_engine::{Error, fiscal_year::Date, money::Money, sales_invoice::wire};
use rusqlite::Transaction;
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Create {
    pub bank_transaction_id: i64,
    pub expected_source_hash: String,
    pub document_id: Option<i64>,
    pub account_no: String,
    pub draft_id: String,
    pub confirm: bool,
}
pub fn read(tx: &Transaction<'_>) -> Result<Value, Error> {
    crate::validate(tx)?;
    let rules = crate::posting_rules::list(tx)?;
    let at: String = tx
        .query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ','now')", [], |r| {
            r.get(0)
        })
        .map_err(invalid)?;
    let mut q=tx.prepare("SELECT b.id,b.transaction_date,b.text,b.amount,b.currency,a.ledger_account_no FROM bank_transactions b LEFT JOIN bank_accounts a ON a.id=b.bank_account_id WHERE NOT EXISTS(SELECT 1 FROM journal_entries j WHERE j.source_bank_transaction_id=b.id) AND NOT EXISTS(SELECT 1 FROM bank_journal_reconciliation_links l WHERE l.bank_transaction_id=b.id) ORDER BY b.transaction_date,b.id LIMIT 501").map_err(invalid)?;
    let mut rs = q.query([]).map_err(invalid)?;
    let mut rows = vec![];
    while let Some(r) = rs.next().map_err(invalid)? {
        let id: i64 = r.get(0).map_err(invalid)?;
        let text: String = r.get(2).map_err(invalid)?;
        let currency: String = r.get(4).map_err(invalid)?;
        let date: String = r.get(1).map_err(invalid)?;
        let amount = crate::legacy_money(r.get_ref(3).map_err(invalid)?)?;
        rows.push(json!({"bankTransactionId":id,"date":date,"text":text,"amount":wire(&amount)?,"currency":currency,"bankAccountNo":r.get::<_,Option<String>>(5).map_err(invalid)?,"suggestedAccount":crate::posting_rules::hint_from(&rules,&text,&currency,&at),"sourceHash":accounting_drafts::evidence(tx,&json!({"sourceBankTransactionId":id}))?}));
    }
    Ok(
        json!({"rows":rows,"truncated":rows.len()>500,"mode":"single-reviewed-draft","independentReviewerRequired":true}),
    )
}
pub fn create(tx: &Transaction<'_>, i: &Create, actor: &str, latest: Date) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    if !i.confirm || i.bank_transaction_id <= 0 {
        return Err(refused("explicit bank draft confirmation required"));
    }
    let evidence = accounting_drafts::evidence(
        tx,
        &json!({"sourceBankTransactionId":i.bank_transaction_id}),
    )?;
    if evidence != i.expected_source_hash {
        return Err(refused("bank source changed; reload queue"));
    }
    let (date,text,amount,currency,bank):(String,String,Money,String,Option<String>)=tx.query_row("SELECT b.transaction_date,b.text,b.amount,b.currency,a.ledger_account_no FROM bank_transactions b LEFT JOIN bank_accounts a ON a.id=b.bank_account_id WHERE b.id=?1",[i.bank_transaction_id],|r|{let raw=r.get_ref(2)?;Ok((r.get(0)?,r.get(1)?,crate::legacy_money(raw).map_err(|_|rusqlite::Error::InvalidQuery)?,r.get(3)?,r.get(4)?))}).map_err(invalid)?;
    if currency != "DKK" || amount == Money::default() {
        return Err(refused(
            "this queue requires nonzero DKK evidence; use an explicitly reviewed FX draft",
        ));
    }
    let bank = bank.ok_or_else(|| refused("map the bank account to a ledger account first"))?;
    if bank == i.account_no {
        return Err(refused("counter account must differ from bank account"));
    }
    let absolute = if amount.is_negative() {
        Money::default().subtract(&amount)
    } else {
        amount.clone()
    };
    let (debit, credit) = if amount.is_negative() {
        (&i.account_no, &bank)
    } else {
        (&bank, &i.account_no)
    };
    let mut payload = json!({"transactionDate":date,"text":text,"sourceBankTransactionId":i.bank_transaction_id,"lines":[{"accountNo":debit,"debitAmount":wire(&absolute)?},{"accountNo":credit,"creditAmount":wire(&absolute)?}]});
    if let Some(id) = i.document_id {
        payload["documentId"] = json!(id);
    }
    accounting_drafts::change(
        tx,
        &i.draft_id,
        "create",
        &accounting_drafts::Input {
            payload: Some(payload),
            ..Default::default()
        },
        actor,
        latest,
    )
}
