//! Native reminder-only claims. Current immutable relations, account effects and bank facts are rechecked.
use crate::{invalid, legacy_money};
use rentemester_engine::{
    Error,
    currency::Currency,
    fiscal_year::Date,
    identifiers::{AccountNo, BankTransactionId, DocumentId},
    manual_journal::{JournalLine, ManualJournal},
    money::Money,
    reminder_claims::{self as engine, refused},
    sales_invoice::wire,
};
use rusqlite::{OptionalExtension, Transaction, params};
use serde::Deserialize;
use serde_json::{Number, Value, json};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Register {
    pub invoice_document_id: i64,
    pub reminder_date: String,
    pub fee_amount: Option<Number>,
    pub note: Option<String>,
    pub confirm: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Post {
    pub invoice_document_id: i64,
    pub reminder_id: i64,
    pub transaction_date: Option<String>,
    pub receivable_account_no: String,
    pub reminder_income_account_no: String,
    pub confirm: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Settle {
    pub invoice_document_id: i64,
    pub bank_transaction_id: i64,
    pub payment_date: Option<String>,
    pub amount: Option<Number>,
    pub receivable_account_no: Option<String>,
    pub confirm: bool,
}
fn positive(id: i64) -> Result<(), Error> {
    if !(1..=9007199254740991).contains(&id) {
        Err(refused("positive exact identifier required"))
    } else {
        Ok(())
    }
}
fn money(n: &Number) -> Result<Money, Error> {
    Money::parse(&n.to_string())
}
fn audit(
    tx: &Transaction<'_>,
    event: &str,
    entity: i64,
    message: &str,
    actor: &str,
) -> Result<(), Error> {
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor)VALUES(?1,'invoice_reminder_claim',?2,?3,?4)",params![event,entity.to_string(),message,format!("user:{actor}")]).map_err(invalid)?;
    Ok(())
}
fn unsupported(tx: &Transaction<'_>, id: i64) -> Result<(), Error> {
    let yes:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM invoice_interest_claims WHERE invoice_document_id=?1 UNION ALL SELECT 1 FROM invoice_compensation_claims WHERE invoice_document_id=?1 UNION ALL SELECT 1 FROM invoice_interest_corrections WHERE invoice_document_id=?1 UNION ALL SELECT 1 FROM invoice_bad_debt_writeoffs WHERE invoice_document_id=?1)",[id],|r|r.get(0)).map_err(invalid)?;
    if yes {
        return Err(refused(
            "interest/compensation/correction/bad-debt claims require their native evidence flow; reminder-only operation unavailable",
        ));
    }
    Ok(())
}
struct Invoice {
    no: String,
    date: String,
    due: String,
    debtor: String,
    gross: Money,
}
fn invoice(tx: &Transaction<'_>, id: i64) -> Result<Invoice, Error> {
    positive(id)?;
    unsupported(tx, id)?;
    let row:Option<(String,String,String,String,Money)>=tx.query_row("SELECT d.invoice_no,d.invoice_date,d.payload_json,a.account_no,p.booked_gross_dkk FROM documents d JOIN issued_invoice_postings p ON p.invoice_document_id=d.id JOIN journal_entries j ON j.id=p.journal_entry_id JOIN accounts a ON a.id=p.receivable_account_id WHERE d.id=?1 AND d.document_type='issued_invoice' AND d.currency='DKK' AND j.status='posted' AND j.document_id=d.id AND j.transaction_date=d.invoice_date AND j.source_bank_transaction_id IS NULL AND NOT EXISTS(SELECT 1 FROM journal_entries reversal WHERE reversal.reversal_of_entry_id=j.id)",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,legacy_money(r.get_ref(4)?).map_err(|_|rusqlite::Error::InvalidQuery)?))).optional().map_err(invalid)?;
    let (no, date, payload, debtor, gross) =
        row.ok_or_else(|| refused("active ledger-posted DKK issued invoice required"))?;
    let p: Value = serde_json::from_str(&payload).map_err(invalid)?;
    let due = engine::effective_due_date(&date, p["dueDate"].as_str())?;
    Date::parse(&due)?;
    Ok(Invoice {
        no,
        date,
        due,
        debtor,
        gross,
    })
}
#[derive(Clone)]
struct Effect {
    account: String,
    kind: String,
    normal: String,
    debit: Money,
    credit: Money,
}
fn lines(tx: &Transaction<'_>, journal: i64) -> Result<Vec<Effect>, Error> {
    let mut q=tx.prepare("SELECT a.account_no,a.type,a.normal_balance,jl.debit_amount,jl.credit_amount,jl.vat_code,jl.currency FROM journal_lines jl JOIN accounts a ON a.id=jl.account_id WHERE jl.journal_entry_id=?1 ORDER BY jl.id").map_err(invalid)?;
    let mut rs = q.query([journal]).map_err(invalid)?;
    let mut out = vec![];
    while let Some(r) = rs.next().map_err(invalid)? {
        if r.get::<_, Option<String>>(5).map_err(invalid)?.is_some()
            || r.get::<_, Option<String>>(6)
                .map_err(invalid)?
                .as_deref()
                .unwrap_or("DKK")
                .trim()
                .to_uppercase()
                != "DKK"
        {
            return Err(refused(
                "claim journal cannot carry VAT or foreign currency effects",
            ));
        }
        out.push(Effect {
            account: r.get(0).map_err(invalid)?,
            kind: r.get(1).map_err(invalid)?,
            normal: r.get(2).map_err(invalid)?,
            debit: legacy_money(r.get_ref(3).map_err(invalid)?)?,
            credit: legacy_money(r.get_ref(4).map_err(invalid)?)?,
        });
    }
    if out.len() < 2 {
        return Err(refused("claim journal lines missing"));
    }
    Ok(out)
}
fn active(
    tx: &Transaction<'_>,
    journal: i64,
    id: i64,
    bank: Option<i64>,
    date: &str,
) -> Result<(), Error> {
    let yes:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM journal_entries j WHERE j.id=?1 AND j.document_id=?2 AND j.source_bank_transaction_id IS ?3 AND j.transaction_date=?4 AND j.status='posted' AND UPPER(TRIM(COALESCE(j.currency,'DKK')))='DKK' AND j.amount_foreign IS NULL AND j.amount_dkk IS NULL AND j.fx_rate_to_dkk IS NULL AND j.reversal_of_entry_id IS NULL AND NOT EXISTS(SELECT 1 FROM journal_entries r WHERE r.reversal_of_entry_id=j.id))",params![journal,id,bank,date],|r|r.get(0)).map_err(invalid)?;
    if !yes {
        return Err(refused(
            "claim/current cash journal missing, reversed or inconsistent",
        ));
    }
    Ok(())
}
struct Bank {
    date: String,
    amount: Money,
    account: String,
    reference: Option<String>,
    source: String,
    hash: String,
}
fn bank(tx: &Transaction<'_>, id: i64) -> Result<Bank, Error> {
    positive(id)?;
    type BankRow = (
        String,
        Money,
        String,
        String,
        Option<String>,
        String,
        String,
    );
    let row:Option<BankRow>=tx.query_row("SELECT b.transaction_date,b.amount,b.currency,a.ledger_account_no,b.reference,b.source_file_hash,b.transaction_hash FROM bank_transactions b JOIN bank_accounts a ON a.id=b.bank_account_id JOIN accounts c ON c.account_no=a.ledger_account_no WHERE b.id=?1 AND a.active=1 AND a.currency='DKK' AND c.active=1 AND c.type='asset' AND c.normal_balance='debit'",[id],|r|Ok((r.get(0)?,crate::bank::signed_money(r.get_ref(1)?).map_err(|_|rusqlite::Error::InvalidQuery)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?))).optional().map_err(invalid)?;
    let (date, amount, currency, account, reference, source, hash) = row.ok_or_else(|| {
        refused(
            "bank transaction requires current explicit active mapped account and archived source",
        )
    })?;
    if currency != "DKK" || amount.is_negative() || amount == Money::default() {
        return Err(refused("positive incoming DKK claim receipt required"));
    }
    crate::bank_reconciliation::source_hash(tx, id)?;
    Ok(Bank {
        date,
        amount,
        account,
        reference,
        source,
        hash,
    })
}
#[derive(Default)]
pub struct Claims {
    pub journals: BTreeSet<i64>,
    pub balances: BTreeMap<String, Money>,
    pub registered: Money,
    pub paid: Money,
    pub reminders: Vec<Value>,
    pub payments: Vec<Value>,
    pub pending: bool,
}
/// Exact validated related journals, the only exclusions allowed in principal carrying.
pub fn validated_claims(tx: &Transaction<'_>, id: i64) -> Result<Claims, Error> {
    unsupported(tx, id)?;
    let mut c = Claims::default();
    let mut events = vec![];
    let mut q=tx.prepare("SELECT r.id,r.reminder_date,r.fee_amount,r.currency,r.note,p.journal_entry_id,j.transaction_date FROM invoice_reminders r LEFT JOIN invoice_reminder_postings p ON p.reminder_id=r.id LEFT JOIN journal_entries j ON j.id=p.journal_entry_id WHERE r.invoice_document_id=?1 ORDER BY r.reminder_date,r.id").map_err(invalid)?;
    let mut rs = q.query([id]).map_err(invalid)?;
    while let Some(r) = rs.next().map_err(invalid)? {
        let reminder: i64 = r.get(0).map_err(invalid)?;
        let date: String = r.get(1).map_err(invalid)?;
        let fee = legacy_money(r.get_ref(2).map_err(invalid)?)?;
        if r.get::<_, String>(3).map_err(invalid)? != "DKK" {
            return Err(refused("DKK reminder evidence required"));
        }
        let journal: Option<i64> = r.get(5).map_err(invalid)?;
        c.registered.add(&fee);
        c.reminders.push(json!({"reminderId":reminder,"reminderDate":date,"feeAmount":wire(&fee)?,"note":r.get::<_,Option<String>>(4).map_err(invalid)?,"journalEntryId":journal}));
        if let Some(j) = journal {
            let posted: String = r.get(6).map_err(invalid)?;
            if posted < date {
                return Err(refused("reminder posting predates registered claim"));
            }
            active(tx, j, id, None, &posted)?;
            events.push((j, false, fee, posted, None));
        } else {
            c.pending = true;
        }
    }
    let mut q=tx.prepare("SELECT id,amount,payment_date,bank_transaction_id,journal_entry_id,currency FROM invoice_claim_payments WHERE invoice_document_id=?1 ORDER BY journal_entry_id,id").map_err(invalid)?;
    let mut rs = q.query([id]).map_err(invalid)?;
    while let Some(r) = rs.next().map_err(invalid)? {
        if r.get::<_, String>(5).map_err(invalid)? != "DKK" {
            return Err(refused("DKK claim payment evidence required"));
        }
        let amount = legacy_money(r.get_ref(1).map_err(invalid)?)?;
        let date: String = r.get(2).map_err(invalid)?;
        let bank: i64 = r.get(3).map_err(invalid)?;
        let journal: i64 = r.get(4).map_err(invalid)?;
        c.payments.push(json!({"claimPaymentId":r.get::<_,i64>(0).map_err(invalid)?,"amount":wire(&amount)?,"paymentDate":date,"bankTransactionId":bank,"journalEntryId":journal}));
        events.push((journal, true, amount, date, Some(bank)));
    }
    events.sort_by_key(|e| e.0);
    let mut latest_claim = String::new();
    for (j, payment, amount, date, bank_id) in events {
        let effects = lines(tx, j)?;
        if !payment {
            let mut debit = Money::default();
            let mut credit = Money::default();
            for e in effects {
                if e.kind == "asset"
                    && e.normal == "debit"
                    && e.credit == Money::default()
                    && e.debit != Money::default()
                {
                    debit.add(&e.debit);
                    c.balances.entry(e.account).or_default().add(&e.debit);
                } else if e.kind == "income"
                    && e.normal == "credit"
                    && e.debit == Money::default()
                    && e.credit != Money::default()
                {
                    credit.add(&e.credit);
                } else {
                    return Err(refused(
                        "reminder posting must debit receivable assets and credit income without other effects",
                    ));
                }
            }
            if debit != amount || credit != amount {
                return Err(refused(
                    "reminder journal amount differs from registered fee",
                ));
            }
            latest_claim = latest_claim.max(date);
        } else {
            let bid = bank_id.unwrap();
            let applications:i64=tx.query_row("SELECT count(*) FROM (SELECT bank_transaction_id FROM invoice_claim_payments UNION ALL SELECT bank_transaction_id FROM invoice_payments UNION ALL SELECT bank_transaction_id FROM invoice_refunds) WHERE bank_transaction_id=?1",[bid],|r|r.get(0)).map_err(invalid)?;
            let journals: i64 = tx
                .query_row(
                    "SELECT count(*) FROM journal_entries WHERE source_bank_transaction_id=?1",
                    [bid],
                    |r| r.get(0),
                )
                .map_err(invalid)?;
            if applications != 1 || journals != 1 {
                return Err(refused(
                    "bank claim receipt has duplicate/cross-invoice application evidence",
                ));
            }
            let b = bank(tx, bid)?;
            active(tx, j, id, Some(bid), &date)?;
            if amount != b.amount || date < latest_claim || b.date < latest_claim {
                return Err(refused(
                    "claim payment amount/date not supported by prior posted claims and bank evidence",
                ));
            }
            let allocated = engine::allocate(&c.balances, &amount)?;
            let mut credits = BTreeMap::<String, Money>::new();
            let mut debit = Money::default();
            for e in effects {
                if e.account == b.account && e.credit == Money::default() {
                    debit.add(&e.debit);
                } else if e.kind == "asset"
                    && e.normal == "debit"
                    && e.debit == Money::default()
                    && e.credit != Money::default()
                {
                    credits.entry(e.account).or_default().add(&e.credit);
                } else {
                    return Err(refused("claim receipt journal has unsupported effects"));
                }
            }
            if debit != amount || credits != allocated {
                return Err(refused(
                    "claim receipt must clear exactly its ledger-backed account allocation",
                ));
            }
            for (a, v) in allocated {
                let left = c.balances[&a].subtract(&v);
                c.balances.insert(a, left);
            }
            c.paid.add(&amount);
        }
        c.journals.insert(j);
    }
    Ok(c)
}
fn principal(tx: &Transaction<'_>, id: i64, i: &Invoice) -> Result<Money, Error> {
    let mut paid = Money::default();
    let mut q=tx.prepare("SELECT p.amount,p.currency,j.status,EXISTS(SELECT 1 FROM journal_entries r WHERE r.reversal_of_entry_id=j.id) FROM invoice_payments p LEFT JOIN journal_entries j ON j.id=p.journal_entry_id WHERE p.invoice_document_id=?1").map_err(invalid)?;
    let mut rs = q.query([id]).map_err(invalid)?;
    while let Some(r) = rs.next().map_err(invalid)? {
        if r.get::<_, String>(1).map_err(invalid)? != "DKK"
            || r.get::<_, String>(2).map_err(invalid)? != "posted"
            || r.get::<_, bool>(3).map_err(invalid)?
        {
            return Err(refused("principal receipt evidence requires resolution"));
        }
        paid.add(&legacy_money(r.get_ref(0).map_err(invalid)?)?);
    }
    let mut open = i
        .gross
        .subtract(&crate::credit_note::credited(tx, id)?)
        .subtract(&paid);
    open.add(&crate::invoice_settlement::refunded(tx, id)?);
    let debtor_id: i64 = tx
        .query_row(
            "SELECT id FROM accounts WHERE account_no=?1",
            [&i.debtor],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    if crate::invoice_settlement::principal_carrying(tx, id, debtor_id)? != open {
        return Err(refused(
            "invoice principal differs from current canonical carrying; review linked journal evidence",
        ));
    }
    Ok(open)
}
pub fn required_banks(tx: &Transaction<'_>, id: i64) -> Result<Vec<i64>, Error> {
    let mut q=tx.prepare("SELECT bank_transaction_id FROM invoice_payments WHERE invoice_document_id=?1 UNION SELECT bank_transaction_id FROM invoice_refunds WHERE invoice_document_id=?1 UNION SELECT bank_transaction_id FROM invoice_claim_payments WHERE invoice_document_id=?1").map_err(invalid)?;
    q.query_map([id], |r| r.get(0))
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)
}
pub fn read(tx: &Transaction<'_>, id: i64, as_of: &str) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    let i = invoice(tx, id)?;
    let c = validated_claims(tx, id)?;
    let open = principal(tx, id, &i)?;
    let mut claim_open = open.clone();
    claim_open.add(&c.registered);
    claim_open = claim_open.subtract(&c.paid);
    let mut ledger = Money::default();
    for v in c.balances.values() {
        ledger.add(v);
    }
    let mut q=tx.prepare("SELECT account_no,name,type,normal_balance FROM accounts WHERE active=1 AND allow_direct_posting=1 AND (type='income' AND normal_balance='credit' OR type='asset' AND normal_balance='debit') ORDER BY account_no").map_err(invalid)?;
    let accounts=q.query_map([],|r|Ok(json!({"accountNo":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"type":r.get::<_,String>(2)?,"normalBalance":r.get::<_,String>(3)?}))).map_err(invalid)?.collect::<Result<Vec<_>,_>>().map_err(invalid)?;
    let mut q=tx.prepare("SELECT b.id,b.transaction_date,b.text,b.amount,b.reference,a.ledger_account_no,b.source_file_hash,b.transaction_hash FROM bank_transactions b JOIN bank_accounts a ON a.id=b.bank_account_id WHERE b.amount>0 AND b.currency='DKK' AND a.active=1 AND NOT EXISTS(SELECT 1 FROM journal_entries j WHERE j.source_bank_transaction_id=b.id) AND NOT EXISTS(SELECT 1 FROM bank_journal_reconciliations r WHERE r.bank_transaction_id=b.id) ORDER BY b.transaction_date,b.id").map_err(invalid)?;
    let banks=q.query_map([],|r|Ok(json!({"id":r.get::<_,i64>(0)?,"date":r.get::<_,String>(1)?,"text":r.get::<_,String>(2)?,"amount":r.get::<_,f64>(3)?,"reference":r.get::<_,Option<String>>(4)?,"bankAccountNo":r.get::<_,Option<String>>(5)?,"sourceHash":r.get::<_,Option<String>>(6)?,"transactionHash":r.get::<_,Option<String>>(7)?}))).map_err(invalid)?.collect::<Result<Vec<_>,_>>().map_err(invalid)?;
    Ok(
        json!({"invoiceDocumentId":id,"invoiceNumber":i.no,"asOfDate":as_of,"dueDate":i.due,"originalReceivableAccountNo":i.debtor,"principalOpenBalance":wire(&open)?,"claimOpenBalance":wire(&claim_open)?,"totalReminderFees":wire(&c.registered)?,"totalClaimPayments":wire(&c.paid)?,"ledgerBackedClaimBalance":wire(&ledger)?,"pendingUnpostedReminders":c.pending,"reminders":c.reminders,"claimPayments":c.payments,"receivableBalances":c.balances.iter().map(|(a,v)|Ok(json!({"accountNo":a,"amountDkk":wire(v)?}))).collect::<Result<Vec<Value>,Error>>()?,"accounts":accounts,"incomingBankTransactions":banks,"rules":{"ruleId":engine::REGISTER_RULE,"maximumFeeAmount":100,"maximumReminders":3,"minimumDaysBetweenReminders":10},"delivery":{"available":false,"sentByThisFlow":false,"priorDeliveryStatus":"unavailable","note":"Native registration and accounting perform no email send; previous delivery is not established"},"settlementAvailable":open==Money::default()&&!c.pending&&ledger!=Money::default()}),
    )
}
pub fn register(tx: &Transaction<'_>, input: &Register, actor: &str) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    if !input.confirm {
        return Err(refused(
            "explicit reminder registration confirmation required",
        ));
    }
    let i = invoice(tx, input.invoice_document_id)?;
    let c = validated_claims(tx, input.invoice_document_id)?;
    let open = principal(tx, input.invoice_document_id, &i)?;
    let dates = c
        .reminders
        .iter()
        .map(|r| r["reminderDate"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    let fee = engine::registration(
        &i.due,
        &input.reminder_date,
        &open,
        input.fee_amount.as_ref().map(money).transpose()?,
        &dates,
    )?;
    if input.note.as_ref().is_some_and(|s| s.len() > 4096) {
        return Err(refused("reminder note exceeds4096 bytes"));
    }
    let id:i64=tx.query_row("INSERT INTO invoice_reminders(invoice_document_id,reminder_date,fee_amount,currency,note)VALUES(?1,?2,?3,'DKK',?4)RETURNING id",params![input.invoice_document_id,input.reminder_date,wire(&fee)?.as_f64(),input.note],|r|r.get(0)).map_err(invalid)?;
    audit(
        tx,
        "invoice_reminder_register",
        id,
        &format!(
            "Registered reminder fee {} on invoice {}",
            fee.decimal(),
            i.no
        ),
        actor,
    )?;
    Ok(
        json!({"ok":true,"reminderId":id,"reminderSequence":dates.len()+1,"invoiceDocumentId":input.invoice_document_id,"invoiceNumber":i.no,"reminderDate":input.reminder_date,"feeAmount":wire(&fee)?,"appliedRules":[engine::REGISTER_RULE],"errors":[]}),
    )
}
fn post_journal(
    tx: &Transaction<'_>,
    id: i64,
    bank: Option<i64>,
    date: &str,
    text: String,
    lines: Vec<JournalLine>,
    posting: (&str, Date),
) -> Result<Value, Error> {
    let (actor, latest) = posting;
    let request = ManualJournal {
        transaction_date: Date::parse(date)?,
        text,
        currency: Currency::parse("DKK")?,
        document_id: Some(DocumentId::from_sqlite(id)),
        source_bank_transaction_id: bank.map(BankTransactionId::from_sqlite),
        amount_foreign: None,
        amount_dkk: None,
        fx_rate_to_dkk: None,
        created_by_program: None,
        lines,
    };
    let r = crate::journal_post::post(
        tx,
        &request,
        &crate::journal_preview::manual_journal_rule_version()?,
        latest,
        crate::journal_post::Actor {
            subject: &format!("user:{actor}"),
            program: "rentemester-rust",
        },
    )?;
    if r["ok"] != true {
        return Err(refused(r["errors"].to_string()));
    }
    Ok(r)
}
fn line(account: &str, debit: Money, credit: Money, text: &str) -> Result<JournalLine, Error> {
    Ok(JournalLine {
        account_no: AccountNo::parse(account)?,
        debit_amount: debit,
        credit_amount: credit,
        vat_code: None,
        text: Some(text.into()),
    })
}
pub fn post(tx: &Transaction<'_>, input: &Post, actor: &str, latest: Date) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    if !input.confirm {
        return Err(refused(
            "explicit separate reminder posting confirmation required",
        ));
    }
    positive(input.reminder_id)?;
    let i = invoice(tx, input.invoice_document_id)?;
    let c = validated_claims(tx, input.invoice_document_id)?;
    let r = c
        .reminders
        .iter()
        .find(|r| r["reminderId"] == input.reminder_id)
        .ok_or_else(|| refused("registered reminder not found for invoice"))?;
    if !r["journalEntryId"].is_null() {
        return Err(refused("reminder is already posted"));
    }
    let date = input
        .transaction_date
        .as_deref()
        .unwrap_or(r["reminderDate"].as_str().unwrap());
    if date < r["reminderDate"].as_str().unwrap() {
        return Err(refused("posting cannot predate registered reminder"));
    }
    for (account, kind, normal) in [
        (&input.receivable_account_no, "asset", "debit"),
        (&input.reminder_income_account_no, "income", "credit"),
    ] {
        let yes:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM accounts WHERE account_no=?1 AND type=?2 AND normal_balance=?3 AND active=1 AND allow_direct_posting=1)",params![account,kind,normal],|r|r.get(0)).map_err(invalid)?;
        if !yes {
            return Err(refused(
                "explicit active directly-postable compatible receivable/income accounts required",
            ));
        }
    }
    let fee = Money::parse(&r["feeAmount"].to_string())?;
    let result = post_journal(
        tx,
        input.invoice_document_id,
        None,
        date,
        format!("Reminder fee {}", i.no),
        vec![
            line(
                &input.receivable_account_no,
                fee.clone(),
                Money::default(),
                "Reminder receivable",
            )?,
            line(
                &input.reminder_income_account_no,
                Money::default(),
                fee,
                "Reminder income",
            )?,
        ],
        (actor, latest),
    )?;
    tx.execute(
        "INSERT INTO invoice_reminder_postings(reminder_id,journal_entry_id)VALUES(?1,?2)",
        params![input.reminder_id, result["entryId"].as_i64()],
    )
    .map_err(invalid)?;
    audit(
        tx,
        "invoice_reminder_post",
        input.reminder_id,
        &format!(
            "Posted reminder for invoice {} in journal {}",
            i.no, result["entryNo"]
        ),
        actor,
    )?;
    Ok(
        json!({"ok":true,"reminderId":input.reminder_id,"invoiceDocumentId":input.invoice_document_id,"entryId":result["entryId"],"entryNo":result["entryNo"],"appliedRules":[engine::POST_RULE],"errors":[]}),
    )
}
pub fn settle(
    tx: &Transaction<'_>,
    input: &Settle,
    actor: &str,
    latest: Date,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    if !input.confirm {
        return Err(refused("explicit claim settlement confirmation required"));
    }
    let i = invoice(tx, input.invoice_document_id)?;
    let c = validated_claims(tx, input.invoice_document_id)?;
    if principal(tx, input.invoice_document_id, &i)? != Money::default() {
        return Err(refused("settle invoice principal before claim receipts"));
    }
    if c.pending {
        return Err(refused(
            "all registered claims must be ledger-posted before receipt",
        ));
    }
    let b = bank(tx, input.bank_transaction_id)?;
    let used:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM journal_entries WHERE source_bank_transaction_id=?1 UNION ALL SELECT 1 FROM bank_journal_reconciliations WHERE bank_transaction_id=?1 UNION ALL SELECT 1 FROM invoice_payments WHERE bank_transaction_id=?1 UNION ALL SELECT 1 FROM invoice_claim_payments WHERE bank_transaction_id=?1 UNION ALL SELECT 1 FROM invoice_refunds WHERE bank_transaction_id=?1)",[input.bank_transaction_id],|r|r.get(0)).map_err(invalid)?;
    if used {
        return Err(refused("bank transaction already linked or applied"));
    }
    if input
        .amount
        .as_ref()
        .map(money)
        .transpose()?
        .is_some_and(|m| m != b.amount)
    {
        return Err(refused(
            "claim receipt amount must equal complete imported bank amount",
        ));
    }
    let date = input.payment_date.as_deref().unwrap_or(&b.date);
    Date::parse(date)?;
    if date < i.date.as_str() {
        return Err(refused("claim receipt cannot precede invoice"));
    }
    for r in &c.reminders {
        let posted: Option<String> = tx
            .query_row(
                "SELECT transaction_date FROM journal_entries WHERE id=?1",
                [r["journalEntryId"].as_i64()],
                |row| row.get(0),
            )
            .optional()
            .map_err(invalid)?;
        if posted
            .as_ref()
            .is_some_and(|p| p.as_str() > date || p > &b.date)
        {
            return Err(refused(
                "claim must already be posted on both payment and bank evidence dates",
            ));
        }
    }
    let allocated = engine::allocate(&c.balances, &b.amount)?;
    if allocated.contains_key(&b.account)
        || input
            .receivable_account_no
            .as_ref()
            .is_some_and(|a| allocated.keys().any(|k| k != a))
    {
        return Err(refused(
            "receipt must clear its actual historical receivable accounts, distinct from bank",
        ));
    }
    let mut entries = vec![line(
        &b.account,
        b.amount.clone(),
        Money::default(),
        "Bank claim receipt",
    )?];
    for (a, v) in allocated {
        entries.push(line(
            &a,
            Money::default(),
            v,
            "Claim receivable settlement",
        )?);
    }
    let result = post_journal(
        tx,
        input.invoice_document_id,
        Some(input.bank_transaction_id),
        date,
        format!("Customer claim payment for invoice {}", i.no),
        entries,
        (actor, latest),
    )?;
    let payment:i64=tx.query_row("INSERT INTO invoice_claim_payments(invoice_document_id,bank_transaction_id,journal_entry_id,payment_date,amount,currency,note)VALUES(?1,?2,?3,?4,?5,'DKK',?6)RETURNING id",params![input.invoice_document_id,input.bank_transaction_id,result["entryId"].as_i64(),date,wire(&b.amount)?.as_f64(),format!("Claim settlement from transaction {}",input.bank_transaction_id)],|r|r.get(0)).map_err(invalid)?;
    audit(
        tx,
        "invoice_claim_payment_apply",
        payment,
        &format!(
            "Applied claim receipt {} to invoice {}",
            b.amount.decimal(),
            i.no
        ),
        actor,
    )?;
    let after = validated_claims(tx, input.invoice_document_id)?;
    Ok(
        json!({"ok":true,"claimPaymentId":payment,"invoiceNumber":i.no,"entryId":result["entryId"],"entryNo":result["entryNo"],"remainingClaimOpenBalance":wire(&after.registered.subtract(&after.paid))?,"bankEvidence":{"bankTransactionId":input.bank_transaction_id,"reference":b.reference,"sourceHash":b.source,"transactionHash":b.hash},"appliedRules":[engine::SETTLE_RULE],"errors":[]}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (rusqlite::Connection, i64) {
        let mut control = rusqlite::Connection::open_in_memory().unwrap();
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic reminder seller")
            .unwrap();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        tx.execute_batch("UPDATE companies SET address='Synthetic Road 1',cvr='12345678',vat_period_type='quarter';INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('R','Synthetic debtor','asset','debit'),('R2','Other debtor','asset','debit'),('S','Synthetic sales','income','credit'),('F','Synthetic claim fees','income','credit'),('V','Synthetic VAT','vat','credit'),('A','Synthetic bank','asset','debit');INSERT INTO account_role_mappings(role,account_no,version,confirmed_by)VALUES('debtors','R',1,'synthetic'),('output_vat','V',1,'synthetic');INSERT INTO bank_accounts(slug,name,ledger_account_no)VALUES('reminder-synthetic','Synthetic reminder bank','A');").unwrap();
        let customer=crate::sales::create_customer(&tx,&serde_json::from_value(json!({"name":"Synthetic reminder buyer","address":"Synthetic Road 2","vatOrCvr":"DK87654321","paymentTermsDays":1})).unwrap(),"synthetic").unwrap();
        let input=serde_json::from_value(json!({"issueDate":"2026-01-01","dueDate":"2026-01-02","customerId":customer["id"],"lines":[{"description":"Synthetic reminder service","quantity":1,"unitPriceExVat":80}]})).unwrap();
        let payload = crate::sales::prepare(&tx, &input, "2026-01-01T12:00:00Z", true).unwrap();
        let artifact = |mime| crate::sales::Artifact {
            filename: "synthetic",
            path: "synthetic",
            hash: if mime == "application/json" {
                "synthetic-json"
            } else {
                "synthetic-pdf"
            },
            mime,
        };
        let result = crate::sales::register(
            &tx,
            &payload,
            artifact("application/json"),
            artifact("application/pdf"),
            "synthetic",
        )
        .unwrap();
        let id = result["documentId"].as_i64().unwrap();
        crate::sales::post(
            &tx,
            &crate::sales::Post {
                invoice_document_id: id,
                revenue_account_no: "S".into(),
                confirm: true,
            },
            "synthetic",
            Date::parse("2026-01-03").unwrap(),
        )
        .unwrap();
        let csv = "date;text;amount;currency;reference\n2026-01-05;Synthetic principal;100;DKK;reminder-principal\n2026-01-06;Synthetic partial fee;20;DKK;reminder-part\n2026-01-07;Synthetic remaining fee;30;DKK;reminder-final\n2026-01-07;Synthetic outgoing;-1;DKK;reminder-out\n2026-01-07;Synthetic excess;51;DKK;reminder-excess\n";
        let rows = rentemester_engine::bank::parse(csv).unwrap();
        crate::bank::import(
            &tx,
            csv,
            &rows,
            Some("reminder-synthetic"),
            "synthetic",
            "2026-01-07T12:00:00Z",
            "synthetic",
        )
        .unwrap();
        tx.commit().unwrap();
        (db, id)
    }
    fn reg(id: i64) -> Register {
        serde_json::from_value(json!({"invoiceDocumentId":id,"reminderDate":"2026-01-03","feeAmount":50,"note":"Synthetic reviewed reminder","confirm":true})).unwrap()
    }
    fn posting(id: i64, r: i64) -> Post {
        serde_json::from_value(json!({"invoiceDocumentId":id,"reminderId":r,"receivableAccountNo":"R","reminderIncomeAccountNo":"F","confirm":true})).unwrap()
    }
    fn settle_input(tx: &Transaction<'_>, id: i64, reference: &str) -> Settle {
        let bank: i64 = tx
            .query_row(
                "SELECT id FROM bank_transactions WHERE reference=?1",
                [reference],
                |r| r.get(0),
            )
            .unwrap();
        serde_json::from_value(
            json!({"invoiceDocumentId":id,"bankTransactionId":bank,"confirm":true}),
        )
        .unwrap()
    }
    #[test]
    fn complete_register_separate_post_principal_then_partial_claim_receipts() {
        let (mut db, id) = fixture();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let latest = Date::parse("2026-01-07").unwrap();
        let r = register(&tx, &reg(id), "synthetic").unwrap();
        let rid = r["reminderId"].as_i64().unwrap();
        assert_eq!(
            read(&tx, id, "2026-01-07").unwrap()["ledgerBackedClaimBalance"],
            json!(0.0)
        );
        assert!(register(&tx, &reg(id), "synthetic").is_err());
        post(&tx, &posting(id, rid), "synthetic", latest).unwrap();
        assert!(post(&tx, &posting(id, rid), "synthetic", latest).is_err());
        assert!(
            settle(
                &tx,
                &settle_input(&tx, id, "reminder-part"),
                "synthetic",
                latest
            )
            .is_err()
        );
        // This regression failed before canonical claims were excluded from principal carrying.
        let principal=serde_json::from_value(json!({"invoiceDocumentId":id,"bankTransactionReference":"reminder-principal","confirm":true})).unwrap();
        crate::invoice_settlement::settle(&tx, &principal, "synthetic", latest).unwrap();
        assert_eq!(
            read(&tx, id, "2026-01-07").unwrap()["principalOpenBalance"],
            json!(0.0)
        );
        assert!(
            settle(
                &tx,
                &settle_input(&tx, id, "reminder-out"),
                "synthetic",
                latest
            )
            .is_err()
        );
        assert!(
            settle(
                &tx,
                &settle_input(&tx, id, "reminder-excess"),
                "synthetic",
                latest
            )
            .is_err()
        );
        let part = settle_input(&tx, id, "reminder-part");
        assert_eq!(
            settle(&tx, &part, "synthetic", latest).unwrap()["remainingClaimOpenBalance"],
            json!(30.0)
        );
        assert!(settle(&tx, &part, "synthetic", latest).is_err());
        assert_eq!(
            settle(
                &tx,
                &settle_input(&tx, id, "reminder-final"),
                "synthetic",
                latest
            )
            .unwrap()["remainingClaimOpenBalance"],
            json!(0.0)
        );
        assert_eq!(
            tx.query_row("SELECT count(*) FROM journal_entries", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            5
        );
        let after = read(&tx, id, "2026-01-07").unwrap();
        assert_eq!(after["claimPayments"].as_array().unwrap().len(), 2);
        assert_eq!(after["ledgerBackedClaimBalance"], json!(0.0));
        assert_eq!(after["delivery"]["sentByThisFlow"], false);
        tx.commit().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("claims.sqlite");
        db.execute("VACUUM INTO ?1", [path.to_str().unwrap()])
            .unwrap();
        drop(db);
        let mut reopened = rusqlite::Connection::open(path).unwrap();
        let tx = reopened
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        assert_eq!(read(&tx, id, "2026-01-07").unwrap(), after);
    }
    #[test]
    fn arbitrary_invoice_debtor_effect_does_not_gain_principal_exclusion() {
        let (mut db, id) = fixture();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let latest = Date::parse("2026-01-07").unwrap();
        let r = register(&tx, &reg(id), "synthetic").unwrap();
        post(
            &tx,
            &posting(id, r["reminderId"].as_i64().unwrap()),
            "synthetic",
            latest,
        )
        .unwrap();
        post_journal(
            &tx,
            id,
            None,
            "2026-01-04",
            "Synthetic arbitrary invoice debit".into(),
            vec![
                line(
                    "R",
                    Money::parse("1").unwrap(),
                    Money::default(),
                    "unclassified",
                )
                .unwrap(),
                line(
                    "F",
                    Money::default(),
                    Money::parse("1").unwrap(),
                    "unclassified",
                )
                .unwrap(),
            ],
            ("synthetic", latest),
        )
        .unwrap();
        let principal=serde_json::from_value(json!({"invoiceDocumentId":id,"bankTransactionReference":"reminder-principal","confirm":true})).unwrap();
        assert!(crate::invoice_settlement::settle(&tx, &principal, "synthetic", latest).is_err());
        assert_eq!(
            tx.query_row("SELECT count(*) FROM invoice_payments", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn closed_period_refuses_separate_post_without_orphan_effects() {
        let (mut db, id) = fixture();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let r = register(&tx, &reg(id), "synthetic").unwrap();
        tx.execute_batch("INSERT INTO accounting_periods(period_start,period_end,kind,status)VALUES('2026-01-01','2026-01-31','custom','closed')").unwrap();
        assert!(
            post(
                &tx,
                &posting(id, r["reminderId"].as_i64().unwrap()),
                "synthetic",
                Date::parse("2026-01-07").unwrap()
            )
            .is_err()
        );
        assert_eq!(
            tx.query_row("SELECT count(*) FROM journal_entries", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            tx.query_row("SELECT count(*) FROM invoice_reminder_postings", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap(),
            0
        );
    }
    #[test]
    fn dates_accounts_and_stale_or_reversed_claim_evidence_refuse() {
        let (mut db, id) = fixture();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let latest = Date::parse("2026-01-07").unwrap();
        let r = register(&tx, &reg(id), "synthetic").unwrap();
        let mut p = posting(id, r["reminderId"].as_i64().unwrap());
        p.reminder_income_account_no = "R".into();
        assert!(post(&tx, &p, "synthetic", latest).is_err());
        p.reminder_income_account_no = "F".into();
        p.transaction_date = Some("2026-01-02".into());
        assert!(post(&tx, &p, "synthetic", latest).is_err());
        p.transaction_date = None;
        let result = post(&tx, &p, "synthetic", latest).unwrap();
        let guard: String = tx
            .query_row(
                "SELECT sql FROM sqlite_schema WHERE name='journal_entries_no_update'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        tx.execute_batch("DROP TRIGGER journal_entries_no_update")
            .unwrap();
        tx.execute(
            "UPDATE journal_entries SET status='reversed' WHERE id=?1",
            [result["entryId"].as_i64()],
        )
        .unwrap();
        tx.execute_batch(&guard).unwrap();
        assert!(validated_claims(&tx, id).is_err());
        assert!(read(&tx, id, "2026-01-07").is_err());
    }
}
