//! Native reminder and compensation claims. Current immutable relations, account effects and bank facts are rechecked.
use crate::{idempotency::PrincipalKind, invalid, legacy_money};
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
pub(crate) mod principal;
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
    principal: PrincipalKind,
) -> Result<(), Error> {
    let (actor, message) = principal::audit(actor, message, principal)?;
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor)VALUES(?1,'invoice_reminder_claim',?2,?3,?4)",params![event,entity.to_string(),message,actor]).map_err(invalid)?;
    Ok(())
}
fn unsupported(tx: &Transaction<'_>, id: i64) -> Result<(), Error> {
    crate::invoice_bad_debt::history(tx, id)?;
    Ok(())
}
struct Invoice {
    no: String,
    date: String,
    due: String,
    debtor: String,
    gross: Money,
    gross_dkk: Money,
    currency: String,
}
fn invoice(tx: &Transaction<'_>, id: i64) -> Result<Invoice, Error> {
    positive(id)?;
    unsupported(tx, id)?;
    let row:Option<(String,String,String,String,Money,String)>=tx.query_row("SELECT d.invoice_no,d.invoice_date,d.payload_json,a.account_no,p.booked_gross_dkk,d.currency FROM documents d JOIN issued_invoice_postings p ON p.invoice_document_id=d.id JOIN journal_entries j ON j.id=p.journal_entry_id JOIN accounts a ON a.id=p.receivable_account_id WHERE d.id=?1 AND d.document_type='issued_invoice' AND j.status='posted' AND j.document_id=d.id AND j.transaction_date=d.invoice_date AND j.source_bank_transaction_id IS NULL AND NOT EXISTS(SELECT 1 FROM journal_entries reversal WHERE reversal.reversal_of_entry_id=j.id)",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,legacy_money(r.get_ref(4)?).map_err(|_|rusqlite::Error::InvalidQuery)?,r.get(5)?))).optional().map_err(invalid)?;
    let (no, date, payload, debtor, gross_dkk, currency) =
        row.ok_or_else(|| refused("active ledger-posted issued invoice required"))?;
    let p: Value = serde_json::from_str(&payload).map_err(invalid)?;
    if p["currency"] != currency
        || p["invoiceNumber"] != no
        || p["issueDate"] != date
        || crate::invoice_fx::projection(&p)?.gross != gross_dkk
    {
        return Err(refused(
            "invoice frozen currency/carrying differs from immutable snapshot",
        ));
    }
    Currency::parse(&currency)?;
    let gross = rentemester_engine::sales_invoice::project(&p)?.gross;
    let due = engine::effective_due_date(&date, p["dueDate"].as_str())?;
    Date::parse(&due)?;
    Ok(Invoice {
        no,
        date,
        due,
        debtor,
        gross,
        gross_dkk,
        currency,
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
    pub interest_total: Money,
    pub interest_corrections_total: Money,
    pub interest_claims: Vec<Value>,
    pub interest_corrections: Vec<Value>,
    pub pending_interest: bool,
    pub reminder_total: Money,
    pub compensation_total: Money,
    pub compensations: Vec<Value>,
    pub pending_reminders: bool,
    pub pending_compensation: bool,
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
        Date::parse(&date)?;
        if fee.is_negative() || fee == Money::default() {
            return Err(refused("positive reminder amount required"));
        }
        c.registered.add(&fee);
        c.reminder_total.add(&fee);
        c.reminders.push(json!({"reminderId":reminder,"reminderDate":date,"feeAmount":wire(&fee)?,"note":r.get::<_,Option<String>>(4).map_err(invalid)?,"journalEntryId":journal}));
        if let Some(j) = journal {
            let posted: String = r.get(6).map_err(invalid)?;
            if posted < date {
                return Err(refused("reminder posting predates registered claim"));
            }
            active(tx, j, id, None, &posted)?;
            events.push((j, 0u8, fee, posted, None));
        } else {
            c.pending = true;
            c.pending_reminders = true;
        }
    }
    let mut q = tx.prepare("SELECT c.id,c.claim_date,c.amount_dkk,p.journal_entry_id,j.transaction_date FROM invoice_compensation_claims c LEFT JOIN invoice_compensation_postings p ON p.compensation_claim_id=c.id LEFT JOIN journal_entries j ON j.id=p.journal_entry_id WHERE c.invoice_document_id=?1 ORDER BY c.id").map_err(invalid)?;
    let mut rs = q.query([id]).map_err(invalid)?;
    while let Some(r) = rs.next().map_err(invalid)? {
        if !c.compensations.is_empty() {
            return Err(refused("one compensation claim per invoice required"));
        }
        let original = invoice(tx, id)?;
        let claim: i64 = r.get(0).map_err(invalid)?;
        let date: String = r.get(1).map_err(invalid)?;
        Date::parse(&date)?;
        if date <= original.due || date.as_str() < "2013-03-01" {
            return Err(refused(
                "compensation registered date is outside eligible overdue period",
            ));
        }
        let amount = legacy_money(r.get_ref(2).map_err(invalid)?)?;
        if amount.is_negative()
            || amount == Money::default()
            || Money::parse("310")?.subtract(&amount).is_negative()
        {
            return Err(refused(
                "compensation amount outside supported frozen policy",
            ));
        }
        let journal: Option<i64> = r.get(3).map_err(invalid)?;
        c.registered.add(&amount);
        c.compensation_total.add(&amount);
        c.compensations.push(json!({"claimId":claim,"claimDate":date,"amountDkk":wire(&amount)?,"journalEntryId":journal}));
        if let Some(j) = journal {
            let posted: String = r.get(4).map_err(invalid)?;
            if posted < date {
                return Err(refused("compensation posting predates registered claim"));
            }
            active(tx, j, id, None, &posted)?;
            events.push((j, 0u8, amount, posted, None));
        } else {
            c.pending = true;
            c.pending_compensation = true;
        }
    }
    let orphan: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM invoice_compensation_postings p LEFT JOIN invoice_compensation_claims c ON c.id=p.compensation_claim_id JOIN journal_entries j ON j.id=p.journal_entry_id WHERE j.document_id=?1 AND (c.id IS NULL OR c.invoice_document_id<>?1))",[id],|r|r.get(0)).map_err(invalid)?;
    if orphan {
        return Err(refused("compensation posting invoice ownership differs"));
    }
    let has_interest:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM invoice_interest_claims WHERE invoice_document_id=?1 UNION ALL SELECT 1 FROM invoice_interest_corrections WHERE invoice_document_id=?1)",[id],|r|r.get(0)).map_err(invalid)?;
    if has_interest {
        let loaded = crate::invoice_interest::history::load(tx, id)?;
        rentemester_engine::invoice_interest::correction(&loaded.history)?;
        for claim in &loaded.history.claims {
            c.registered.add(&claim.amount);
            c.interest_total.add(&claim.amount);
            c.interest_claims.push(json!({"claimId":claim.id,"claimDate":claim.claim_date,"amountDkk":wire(&claim.amount)?,"journalEntryId":claim.posted_journal_id}));
            if let Some(j) = claim.posted_journal_id {
                let posted: String = tx
                    .query_row(
                        "SELECT transaction_date FROM journal_entries WHERE id=?1",
                        [j],
                        |r| r.get(0),
                    )
                    .map_err(invalid)?;
                if Date::parse(&posted)? < claim.claim_date {
                    return Err(refused("interest posting predates registered claim"));
                }
                active(tx, j, id, None, &posted)?;
                events.push((j, 0u8, claim.amount.clone(), posted, None));
            } else {
                c.pending = true;
                c.pending_interest = true;
            }
        }
        for correction in &loaded.history.corrections {
            crate::audit_invoice::interest_correction_effects(tx, correction.id)?;
            c.interest_corrections_total.add(&correction.amount);
            c.registered = c.registered.subtract(&correction.amount);
            c.interest_corrections.push(json!({"correctionId":correction.id,"correctionDate":correction.correction_date,"amountDkk":wire(&correction.amount)?,"journalEntryId":correction.journal_id}));
            events.push((
                correction.journal_id,
                2u8,
                correction.amount.clone(),
                correction.correction_date.to_string(),
                None,
            ));
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
        events.push((journal, 1u8, amount, date, Some(bank)));
    }
    events.sort_by_key(|e| e.0);
    let mut latest_claim = String::new();
    for (j, kind, amount, date, bank_id) in events {
        if !c.journals.insert(j) {
            return Err(refused("claim journal has duplicate owners"));
        }
        let owners: i64 = tx.query_row("SELECT count(*) FROM (SELECT journal_entry_id FROM invoice_reminder_postings UNION ALL SELECT journal_entry_id FROM invoice_compensation_postings UNION ALL SELECT journal_entry_id FROM invoice_claim_payments UNION ALL SELECT journal_entry_id FROM issued_invoice_postings UNION ALL SELECT journal_entry_id FROM invoice_payments UNION ALL SELECT journal_entry_id FROM invoice_refunds UNION ALL SELECT journal_entry_id FROM credit_note_postings UNION ALL SELECT journal_entry_id FROM invoice_interest_postings UNION ALL SELECT journal_entry_id FROM invoice_interest_corrections UNION ALL SELECT journal_entry_id FROM invoice_bad_debt_writeoffs UNION ALL SELECT journal_entry_id FROM payables UNION ALL SELECT journal_entry_id FROM payable_payments UNION ALL SELECT journal_entry_id FROM opening_balances) WHERE journal_entry_id=?1",[j],|r|r.get(0)).map_err(invalid)?;
        if owners != 1 && !(kind == 1 && owners == 2) {
            return Err(refused("claim journal has reused or missing ownership"));
        }
        Date::parse(&date)?;
        let effects = lines(tx, j)?;
        if kind == 2 {
            let mut credits = BTreeMap::<String, Money>::new();
            let mut debit = Money::default();
            let mut credit = Money::default();
            for e in effects {
                if e.kind == "income" && e.normal == "credit" && e.credit == Money::default() {
                    debit.add(&e.debit);
                } else if e.kind == "asset" && e.normal == "debit" && e.debit == Money::default() {
                    credit.add(&e.credit);
                    credits.entry(e.account).or_default().add(&e.credit);
                } else {
                    return Err(refused("interest correction has unsupported effects"));
                }
            }
            if debit != amount || credit != amount {
                return Err(refused("interest correction amount differs"));
            }
            for (a, v) in credits {
                let old = c.balances.entry(a).or_default();
                if old.subtract(&v).is_negative() {
                    return Err(refused("interest correction exceeds prior account balance"));
                }
                *old = old.subtract(&v);
            }
        } else if kind == 0 {
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
                        "claim posting must debit receivable assets and credit income without other effects",
                    ));
                }
            }
            if debit != amount || credit != amount {
                return Err(refused(
                    "claim journal amount differs from registered amount",
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
            if !(applications == 1 || applications == 2) || journals != 1 {
                return Err(refused(
                    "bank claim receipt has duplicate/cross-invoice application evidence",
                ));
            }
            let grouped = applications == 2;
            if grouped {
                crate::audit_invoice::interest_cash_evidence(tx, id)?;
            }
            let b = bank(tx, bid)?;
            active(tx, j, id, Some(bid), &date)?;
            if (!grouped && amount != b.amount)
                || (grouped && b.amount.subtract(&amount).is_negative())
                || date < latest_claim
                || b.date < latest_claim
            {
                return Err(refused(
                    "claim payment amount/date not supported by prior posted claims and bank evidence",
                ));
            }
            let allocated = engine::allocate(&c.balances, &amount)?;
            let mut credits = BTreeMap::<String, Money>::new();
            let mut debit = Money::default();
            for e in effects {
                if grouped {
                    continue;
                }
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
            if !grouped && (debit != amount || credits != allocated) {
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
    let written = crate::invoice_bad_debt::history(tx, id)?;
    let paid = crate::invoice_fx::movements(tx, id, &i.currency, false)?;
    let refunded = crate::invoice_fx::movements(tx, id, &i.currency, true)?;
    let mut open = i
        .gross
        .subtract(&crate::credit_note::credited(tx, id)?)
        .subtract(&paid);
    open.add(&refunded);
    open = open.subtract(&written.gross);
    let basis = |amount: &Money| {
        if *amount == Money::default() {
            Ok(Money::default())
        } else {
            crate::invoice_fx::portion(&i.gross, &i.gross_dkk, &Money::default(), amount)
        }
    };
    let mut expected = i
        .gross_dkk
        .subtract(&crate::invoice_fx::credited_dkk(tx, id)?)
        .subtract(&basis(&paid)?);
    expected.add(&basis(&refunded)?);
    let debtor_id: i64 = tx
        .query_row(
            "SELECT id FROM accounts WHERE account_no=?1",
            [&i.debtor],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    if !written.entries.is_empty() {
        expected = crate::invoice_bad_debt::adjusted_carrying(tx, id, debtor_id)?;
    }
    if crate::invoice_settlement::principal_carrying(tx, id, debtor_id)? != expected {
        return Err(refused(
            "invoice principal differs from current canonical carrying; review linked journal evidence",
        ));
    }
    Ok(open)
}
/// Current collectible principal in original invoice currency, with frozen DKK carrying proof.
/// Read-only: calculation does not require a writable transaction or grant mutation authority.
pub fn principal_open(tx: &Transaction<'_>, id: i64) -> Result<(Money, String), Error> {
    crate::journal_preview::require_current(tx)?;
    let invoice = invoice(tx, id)?;
    Ok((principal(tx, id, &invoice)?, invoice.currency))
}
pub fn required_banks(tx: &Transaction<'_>, id: i64) -> Result<Vec<i64>, Error> {
    let mut q=tx.prepare("SELECT bank_transaction_id FROM invoice_payments WHERE invoice_document_id=?1 AND bank_transaction_id IS NOT NULL UNION SELECT bank_transaction_id FROM invoice_refunds WHERE invoice_document_id=?1 AND bank_transaction_id IS NOT NULL UNION SELECT bank_transaction_id FROM invoice_claim_payments WHERE invoice_document_id=?1 AND bank_transaction_id IS NOT NULL").map_err(invalid)?;
    q.query_map([id], |r| r.get(0))
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)
}
/// Recheck persisted reminder workflows for period assurance without changing them.
pub fn audit_evidence(tx: &Transaction<'_>) -> Result<Vec<Value>, Error> {
    let mut q = tx.prepare("SELECT invoice_document_id FROM invoice_reminders UNION SELECT invoice_document_id FROM invoice_compensation_claims UNION SELECT invoice_document_id FROM invoice_interest_claims UNION SELECT invoice_document_id FROM invoice_interest_corrections UNION SELECT invoice_document_id FROM invoice_claim_payments UNION SELECT invoice_document_id FROM invoice_bad_debt_writeoffs").map_err(invalid)?;
    let invoices = q
        .query_map([], |row| row.get::<_, i64>(0))
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    let mut errors = vec![];
    for id in invoices {
        let check = (|| {
            let i = invoice(tx, id)?;
            principal(tx, id, &i)?;
            let claims = validated_claims(tx, id)?;
            for reminder in &claims.reminders {
                Date::parse(
                    reminder["reminderDate"]
                        .as_str()
                        .ok_or_else(|| refused("reminder date missing"))?,
                )?;
                require_audit(
                    tx,
                    "invoice_reminder_register",
                    reminder["reminderId"].as_i64().unwrap(),
                )?;
                if !reminder["journalEntryId"].is_null() {
                    require_audit(
                        tx,
                        "invoice_reminder_post",
                        reminder["reminderId"].as_i64().unwrap(),
                    )?;
                }
            }
            for claim in &claims.compensations {
                for event in ["invoice_compensation_register", "invoice_compensation_post"] {
                    if event.ends_with("post") && claim["journalEntryId"].is_null() {
                        continue;
                    }
                    require_compensation_audit(tx, event, claim["claimId"].as_i64().unwrap())?;
                }
            }
            for claim in &claims.interest_claims {
                require_interest_audit(
                    tx,
                    "invoice_interest_register",
                    "invoice_interest_claim",
                    claim["claimId"].as_i64().unwrap(),
                )?;
                if !claim["journalEntryId"].is_null() {
                    require_interest_audit(
                        tx,
                        "invoice_interest_post",
                        "invoice_interest_claim",
                        claim["claimId"].as_i64().unwrap(),
                    )?;
                }
            }
            for correction in &claims.interest_corrections {
                require_interest_audit(
                    tx,
                    "invoice_interest_correction",
                    "invoice_interest_correction",
                    correction["correctionId"].as_i64().unwrap(),
                )?;
            }
            for writeoff in crate::invoice_bad_debt::history(tx, id)?.entries {
                require_interest_audit(
                    tx,
                    "invoice_bad_debt_writeoff",
                    "invoice_bad_debt_writeoff",
                    writeoff["writeOffId"].as_i64().unwrap(),
                )?;
            }
            for payment in &claims.payments {
                require_audit(
                    tx,
                    "invoice_claim_payment_apply",
                    payment["claimPaymentId"].as_i64().unwrap(),
                )?;
            }
            Ok::<(), Error>(())
        })();
        if let Err(error) = check {
            errors.push(json!({"code":"INVALID_REMINDER_CLAIM_EVIDENCE","invoiceDocumentId":id,"error":error.message}));
        }
    }
    Ok(errors)
}
fn require_audit(tx: &Transaction<'_>, event: &str, id: i64) -> Result<(), Error> {
    let (count, valid): (i64, i64) = tx.query_row("SELECT count(*),COALESCE(sum(length(trim(actor))>0),0) FROM audit_log WHERE event_type=?1 AND entity_type='invoice_reminder_claim' AND entity_id=?2", params![event,id.to_string()], |r| Ok((r.get(0)?,r.get(1)?))).map_err(invalid)?;
    if count != 1 || valid != 1 {
        return Err(refused("unique reminder workflow audit evidence required"));
    }
    Ok(())
}
fn require_compensation_audit(tx: &Transaction<'_>, event: &str, id: i64) -> Result<(), Error> {
    let rows:Vec<(String,String,bool)>=tx.prepare("SELECT actor,message,actor LIKE 'user:%' AND length(trim(substr(actor,6)))>0 FROM audit_log WHERE event_type=?1 AND entity_type='invoice_compensation_claim' AND entity_id=?2").map_err(invalid)?.query_map(params![event,id.to_string()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(invalid)?.collect::<Result<_,_>>().map_err(invalid)?;
    if rows.len() != 1 {
        return Err(refused(
            "unique compensation workflow audit evidence required",
        ));
    }
    let (actor, message, human) = &rows[0];
    if *human {
        return Ok(());
    }
    let subject = principal::service_subject(actor, message)
        .ok_or_else(|| refused("typed compensation service audit evidence required"))?;
    if event == "invoice_compensation_post" {
        let bound:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM invoice_compensation_postings p JOIN journal_entries j ON j.id=p.journal_entry_id WHERE p.compensation_claim_id=?1 AND j.created_by=?2 AND j.created_by_program='service-principal')",params![id,format!("user:{subject}")],|r|r.get(0)).map_err(invalid)?;
        if !bound {
            return Err(refused(
                "compensation service audit and journal principal differ",
            ));
        }
    }
    Ok(())
}

fn require_interest_audit(
    tx: &Transaction<'_>,
    event: &str,
    entity: &str,
    id: i64,
) -> Result<(), Error> {
    let (count,valid):(i64,i64)=tx.query_row("SELECT count(*),COALESCE(sum(length(trim(actor))>0),0) FROM audit_log WHERE event_type=?1 AND entity_type=?2 AND entity_id=?3",params![event,entity,id.to_string()],|r|Ok((r.get(0)?,r.get(1)?))).map_err(invalid)?;
    if count != 1 || valid != 1 {
        return Err(refused("unique interest workflow audit evidence required"));
    }
    Ok(())
}
pub fn read(tx: &Transaction<'_>, id: i64, as_of: &str) -> Result<Value, Error> {
    crate::journal_preview::require_current(tx)?;
    let i = invoice(tx, id)?;
    let c = validated_claims(tx, id)?;
    let written = crate::invoice_bad_debt::history(tx, id)?;
    let open = principal(tx, id, &i)?;
    let claim_open = if i.currency == "DKK" {
        let mut total = open.clone();
        total.add(&c.registered);
        wire(&total.subtract(&c.paid))?
    } else {
        Value::Null
    };
    let mut ledger = Money::default();
    for v in c.balances.values() {
        ledger.add(v);
    }
    let mut q=tx.prepare("SELECT account_no,name,type,normal_balance FROM accounts WHERE active=1 AND allow_direct_posting=1 AND (type='income' AND normal_balance='credit' OR type IN ('asset','expense') AND normal_balance='debit') ORDER BY account_no").map_err(invalid)?;
    let accounts=q.query_map([],|r|Ok(json!({"accountNo":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"type":r.get::<_,String>(2)?,"normalBalance":r.get::<_,String>(3)?}))).map_err(invalid)?.collect::<Result<Vec<_>,_>>().map_err(invalid)?;
    let mut q=tx.prepare("SELECT b.id,b.transaction_date,b.text,b.amount,b.reference,a.ledger_account_no,b.source_file_hash,b.transaction_hash FROM bank_transactions b JOIN bank_accounts a ON a.id=b.bank_account_id WHERE b.amount>0 AND b.currency='DKK' AND a.active=1 AND NOT EXISTS(SELECT 1 FROM journal_entries j WHERE j.source_bank_transaction_id=b.id) AND NOT EXISTS(SELECT 1 FROM bank_journal_reconciliations r WHERE r.bank_transaction_id=b.id) ORDER BY b.transaction_date,b.id").map_err(invalid)?;
    let banks=q.query_map([],|r|Ok(json!({"id":r.get::<_,i64>(0)?,"date":r.get::<_,String>(1)?,"text":r.get::<_,String>(2)?,"amount":r.get::<_,f64>(3)?,"reference":r.get::<_,Option<String>>(4)?,"bankAccountNo":r.get::<_,Option<String>>(5)?,"sourceHash":r.get::<_,Option<String>>(6)?,"transactionHash":r.get::<_,Option<String>>(7)?}))).map_err(invalid)?.collect::<Result<Vec<_>,_>>().map_err(invalid)?;
    Ok(
        json!({"invoiceDocumentId":id,"invoiceNumber":i.no,"asOfDate":as_of,"dueDate":i.due,"originalReceivableAccountNo":i.debtor,"principalOpenBalance":wire(&open)?,"principalCurrency":i.currency,"badDebtWriteOffs":written.entries,"totalBadDebtWrittenOff":wire(&written.gross)?,"totalBadDebtWrittenOffDkk":wire(&written.gross_dkk)?,"claimCurrency":"DKK","claimOpenBalance":claim_open,"totalReminderFees":wire(&c.reminder_total)?,"totalCompensationDkk":wire(&c.compensation_total)?,"totalInterestDkk":wire(&c.interest_total)?,"totalInterestCorrectionsDkk":wire(&c.interest_corrections_total)?,"pendingUnpostedInterest":c.pending_interest,"interestClaims":c.interest_claims,"interestCorrections":c.interest_corrections,"registeredClaimBalanceDkk":wire(&c.registered.subtract(&c.paid))?,"pendingUnpostedCompensation":c.pending_compensation,"compensations":c.compensations,"totalClaimPayments":wire(&c.paid)?,"ledgerBackedClaimBalance":wire(&ledger)?,"pendingUnpostedReminders":c.pending_reminders,"reminders":c.reminders,"claimPayments":c.payments,"receivableBalances":c.balances.iter().map(|(a,v)|Ok(json!({"accountNo":a,"amountDkk":wire(v)?}))).collect::<Result<Vec<Value>,Error>>()?,"accounts":accounts,"incomingBankTransactions":banks,"rules":{"ruleId":engine::REGISTER_RULE,"maximumFeeAmount":100,"maximumReminders":3,"minimumDaysBetweenReminders":10},"delivery":{"available":false,"sentByThisFlow":false,"priorDeliveryStatus":"unavailable","note":"Native registration and accounting perform no email send; previous delivery is not established"},"settlementAvailable":open==Money::default()&&!c.pending&&ledger!=Money::default()}),
    )
}
pub fn register(tx: &Transaction<'_>, input: &Register, actor: &str) -> Result<Value, Error> {
    register_with_principal(tx, input, actor, PrincipalKind::User)
}
pub fn register_with_principal(
    tx: &Transaction<'_>,
    input: &Register,
    actor: &str,
    principal: PrincipalKind,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    if !input.confirm {
        return Err(refused(
            "explicit reminder registration confirmation required",
        ));
    }
    let i = invoice(tx, input.invoice_document_id)?;
    if i.currency != "DKK" {
        return Err(refused(
            "DKK invoice required for reminder registration/posting",
        ));
    }
    let c = validated_claims(tx, input.invoice_document_id)?;
    let open = self::principal(tx, input.invoice_document_id, &i)?;
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
        principal,
    )?;
    Ok(
        json!({"ok":true,"reminderId":id,"reminderSequence":dates.len()+1,"invoiceDocumentId":input.invoice_document_id,"invoiceNumber":i.no,"reminderDate":input.reminder_date,"feeAmount":wire(&fee)?,"appliedRules":[engine::REGISTER_RULE],"errors":[]}),
    )
}
pub(crate) fn post_journal(
    tx: &Transaction<'_>,
    id: i64,
    bank: Option<i64>,
    date: &str,
    text: String,
    lines: Vec<JournalLine>,
    posting: (&str, Date, PrincipalKind),
) -> Result<Value, Error> {
    let (actor, latest, principal) = posting;
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
            program: principal::program(principal),
        },
    )?;
    if r["ok"] != true {
        return Err(refused(r["errors"].to_string()));
    }
    Ok(r)
}
pub(crate) fn line(
    account: &str,
    debit: Money,
    credit: Money,
    text: &str,
) -> Result<JournalLine, Error> {
    Ok(JournalLine {
        account_no: AccountNo::parse(account)?,
        debit_amount: debit,
        credit_amount: credit,
        vat_code: None,
        text: Some(text.into()),
    })
}
pub fn post(tx: &Transaction<'_>, input: &Post, actor: &str, latest: Date) -> Result<Value, Error> {
    post_with_principal(tx, input, actor, latest, PrincipalKind::User)
}
pub fn post_with_principal(
    tx: &Transaction<'_>,
    input: &Post,
    actor: &str,
    latest: Date,
    principal: PrincipalKind,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    if !input.confirm {
        return Err(refused(
            "explicit separate reminder posting confirmation required",
        ));
    }
    positive(input.reminder_id)?;
    let i = invoice(tx, input.invoice_document_id)?;
    if i.currency != "DKK" {
        return Err(refused(
            "DKK invoice required for reminder registration/posting",
        ));
    }
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
        (actor, latest, principal),
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
        principal,
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
    settle_with_principal(tx, input, actor, latest, PrincipalKind::User)
}
pub fn settle_with_principal(
    tx: &Transaction<'_>,
    input: &Settle,
    actor: &str,
    latest: Date,
    principal: PrincipalKind,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    if !input.confirm {
        return Err(refused("explicit claim settlement confirmation required"));
    }
    let i = invoice(tx, input.invoice_document_id)?;
    let c = validated_claims(tx, input.invoice_document_id)?;
    if self::principal(tx, input.invoice_document_id, &i)? != Money::default() {
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
    for r in c
        .reminders
        .iter()
        .chain(&c.compensations)
        .chain(&c.interest_claims)
        .chain(&c.interest_corrections)
    {
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
        (actor, latest, principal),
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
        principal,
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
        fixture_currency("DKK")
    }
    fn fixture_currency(currency: &str) -> (rusqlite::Connection, i64) {
        let mut control = rusqlite::Connection::open_in_memory().unwrap();
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic reminder seller")
            .unwrap();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        tx.execute_batch("UPDATE companies SET address='Synthetic Road 1',cvr='12345678',vat_period_type='quarter';INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('R','Synthetic debtor','asset','debit'),('R2','Other debtor','asset','debit'),('S','Synthetic sales','income','credit'),('F','Synthetic claim fees','income','credit'),('V','Synthetic VAT','vat','credit'),('A','Synthetic bank','asset','debit');INSERT INTO account_role_mappings(role,account_no,version,confirmed_by)VALUES('debtors','R',1,'synthetic'),('output_vat','V',1,'synthetic');INSERT INTO bank_accounts(slug,name,ledger_account_no)VALUES('reminder-synthetic','Synthetic reminder bank','A');").unwrap();
        let customer=crate::sales::create_customer(&tx,&serde_json::from_value(json!({"name":"Synthetic reminder buyer","address":"Synthetic Road 2","vatOrCvr":"DK87654321","paymentTermsDays":1})).unwrap(),"synthetic").unwrap();
        let input=serde_json::from_value(json!({"issueDate":"2026-01-01","dueDate":"2026-01-02","currency":currency,"fxRateToDkk":if currency=="DKK"{1.0}else{7.46},"customerId":customer["id"],"lines":[{"description":"Synthetic reminder service","quantity":1,"unitPriceExVat":80}]})).unwrap();
        let payload = crate::sales::prepare(&tx, &input, "2026-01-01T12:00:00Z", true).unwrap();
        use sha2::{Digest, Sha256};
        let json_hash = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec_pretty(&payload).unwrap())
        );
        let pdf_hash = format!("{:x}", Sha256::digest(b"synthetic compensation PDF"));
        let artifact = |mime| crate::sales::Artifact {
            filename: "synthetic",
            path: "synthetic",
            hash: if mime == "application/json" {
                &json_hash
            } else {
                &pdf_hash
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
        if currency != "DKK" {
            tx.execute("INSERT INTO bank_accounts(slug,name,ledger_account_no,currency) VALUES('synthetic-eur','Synthetic EUR bank','A','EUR')",[]).unwrap();
            let csv = "date;text;amount;currency;reference;amount_dkk;fx_rate_to_dkk\n2026-01-05;Synthetic foreign principal;100;EUR;foreign-principal;746;7.46\n";
            let rows = rentemester_engine::bank::parse(csv).unwrap();
            crate::bank::import(
                &tx,
                csv,
                &rows,
                Some("synthetic-eur"),
                "synthetic",
                "2026-01-07T12:00:00Z",
                "synthetic",
            )
            .unwrap();
        }
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
    fn compensation_register(tx: &Transaction<'_>, id: i64) -> Value {
        crate::invoice_compensation::register(tx,&serde_json::from_value(json!({"invoiceDocumentId":id,"asOfDate":"2026-01-03","compensationAmountDkk":50,"confirm":true})).unwrap(),"synthetic").unwrap()
    }
    fn compensation_post(tx: &Transaction<'_>, id: i64) -> Value {
        crate::invoice_compensation::post(tx,&serde_json::from_value(json!({"invoiceDocumentId":id,"receivableAccountNo":"R","compensationIncomeAccountNo":"F","confirm":true})).unwrap(),"synthetic",Date::parse("2026-01-07").unwrap()).unwrap()
    }
    #[test]
    fn compensation_dkk_and_foreign_principal_then_actual_dkk_claim_receipts() {
        for currency in ["DKK", "EUR"] {
            let (mut db, id) = fixture_currency(currency);
            // Principal calculation must work with a truly read-only transaction.
            db.pragma_update(None, "query_only", true).unwrap();
            {
                let tx = db.transaction().unwrap();
                let (amount, unit) = principal_open(&tx, id).unwrap();
                assert_eq!(amount, Money::parse("100").unwrap());
                assert_eq!(unit, currency);
            }
            db.pragma_update(None, "query_only", false).unwrap();
            let tx = db
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .unwrap();
            let before: i64 = tx
                .query_row("SELECT count(*) FROM journal_entries", [], |r| r.get(0))
                .unwrap();
            compensation_register(&tx, id);
            assert_eq!(
                tx.query_row("SELECT count(*) FROM journal_entries", [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                before
            );
            assert_eq!(
                principal_open(&tx, id).unwrap().0,
                Money::parse("100").unwrap()
            );
            let pending = read(&tx, id, "2026-01-07").unwrap();
            assert_eq!(pending["pendingUnpostedCompensation"], true);
            assert_eq!(pending["pendingUnpostedReminders"], false);
            compensation_post(&tx, id);
            let posted = read(&tx, id, "2026-01-07").unwrap();
            assert_eq!(posted["principalCurrency"], currency);
            assert_eq!(posted["claimCurrency"], "DKK");
            assert_eq!(posted["totalReminderFees"], json!(0.0));
            assert_eq!(posted["totalCompensationDkk"], json!(50.0));
            assert_eq!(posted["ledgerBackedClaimBalance"], json!(50.0));
            assert_eq!(
                posted["claimOpenBalance"],
                if currency == "DKK" {
                    json!(150.0)
                } else {
                    Value::Null
                }
            );
            let latest = Date::parse("2026-01-07").unwrap();
            assert!(
                settle(
                    &tx,
                    &settle_input(&tx, id, "reminder-part"),
                    "synthetic",
                    latest
                )
                .is_err()
            );
            let reference = if currency == "DKK" {
                "reminder-principal"
            } else {
                "foreign-principal"
            };
            crate::invoice_settlement::settle(&tx,&serde_json::from_value(json!({"invoiceDocumentId":id,"bankTransactionReference":reference,"confirm":true})).unwrap(),"synthetic",latest).unwrap();
            assert_eq!(principal_open(&tx, id).unwrap().0, Money::default());
            let part = settle_input(&tx, id, "reminder-part");
            settle(&tx, &part, "synthetic", latest).unwrap();
            assert!(settle(&tx, &part, "synthetic", latest).is_err());
            settle(
                &tx,
                &settle_input(&tx, id, "reminder-final"),
                "synthetic",
                latest,
            )
            .unwrap();
            let final_report = read(&tx, id, "2026-01-07").unwrap();
            assert_eq!(final_report["registeredClaimBalanceDkk"], json!(0.0));
            assert_eq!(final_report["ledgerBackedClaimBalance"], json!(0.0));
            assert_eq!(final_report["settlementAvailable"], false);
            assert!(audit_evidence(&tx).unwrap().is_empty());
            let invoice_audit = crate::audit_invoice::verify(&tx).unwrap();
            assert_eq!(invoice_audit["ok"], true, "{invoice_audit}");
            let close_audit = crate::periods::audit::verify(&tx).unwrap();
            assert!(close_audit.is_empty(), "{close_audit:?}");
            tx.commit().unwrap();
            let tx = db.transaction().unwrap();
            assert_eq!(read(&tx, id, "2026-01-07").unwrap(), final_report);
        }
    }
    #[test]
    fn compensation_tampered_amount_and_reused_journal_are_refused() {
        let (mut db, id) = fixture();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        compensation_register(&tx, id);
        compensation_post(&tx, id);
        let reminder = register(&tx, &reg(id), "synthetic").unwrap()["reminderId"]
            .as_i64()
            .unwrap();
        let triggers:Vec<String>=tx.prepare("SELECT name FROM sqlite_master WHERE type='trigger' AND tbl_name='invoice_compensation_claims'").unwrap().query_map([],|r|r.get(0)).unwrap().collect::<Result<_,_>>().unwrap();
        for trigger in triggers {
            tx.execute_batch(&format!(
                "DROP TRIGGER \"{}\"",
                trigger.replace('"', "\"\"")
            ))
            .unwrap();
        }
        tx.execute(
            "UPDATE invoice_compensation_claims SET amount_dkk=51 WHERE invoice_document_id=?1",
            [id],
        )
        .unwrap();
        assert!(validated_claims(&tx, id).is_err());
        tx.execute(
            "UPDATE invoice_compensation_claims SET amount_dkk=50 WHERE invoice_document_id=?1",
            [id],
        )
        .unwrap();
        let claim_journal:i64=tx.query_row("SELECT p.journal_entry_id FROM invoice_compensation_postings p JOIN invoice_compensation_claims c ON c.id=p.compensation_claim_id WHERE c.invoice_document_id=?1",[id],|r|r.get(0)).unwrap();
        // A second family must never give a claim journal a second owner/exclusion.
        let triggers:Vec<String>=tx.prepare("SELECT name FROM sqlite_master WHERE type='trigger' AND tbl_name='invoice_reminder_postings'").unwrap().query_map([],|r|r.get(0)).unwrap().collect::<Result<_,_>>().unwrap();
        for trigger in triggers {
            tx.execute_batch(&format!(
                "DROP TRIGGER \"{}\"",
                trigger.replace('"', "\"\"")
            ))
            .unwrap();
        }
        tx.execute(
            "INSERT INTO invoice_reminder_postings(reminder_id,journal_entry_id)VALUES(?1,?2)",
            params![reminder, claim_journal],
        )
        .unwrap();
        assert!(validated_claims(&tx, id).is_err());
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
        let packet = crate::periods::compute(&tx, "2026-01-01", "2026-01-31", Ok(vec![])).unwrap();
        let audit = packet["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["code"] == "LEDGER_AUDIT_CHAIN")
            .unwrap();
        assert_eq!(
            audit["status"], "passed",
            "validated reminder claims must allow audited period close: {audit}"
        );
        tx.execute_batch("SAVEPOINT corrupt_claim_audit; INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) SELECT event_type,entity_type,entity_id,message,actor FROM audit_log WHERE event_type='invoice_claim_payment_apply' LIMIT 1;").unwrap();
        assert!(!audit_evidence(&tx).unwrap().is_empty());
        tx.execute_batch("ROLLBACK TO corrupt_claim_audit; RELEASE corrupt_claim_audit;")
            .unwrap();
        tx.execute_batch("SAVEPOINT blank_claim_audit; INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) SELECT event_type,entity_type,entity_id,message,'' FROM audit_log WHERE event_type='invoice_claim_payment_apply' LIMIT 1;").unwrap();
        assert!(!audit_evidence(&tx).unwrap().is_empty());
        tx.execute_batch("ROLLBACK TO blank_claim_audit; RELEASE blank_claim_audit;")
            .unwrap();
        tx.execute_batch("SAVEPOINT missing_claim_audit; DROP TRIGGER audit_log_no_delete; DELETE FROM audit_log WHERE event_type='invoice_claim_payment_apply';").unwrap();
        assert!(!audit_evidence(&tx).unwrap().is_empty());
        tx.execute_batch("ROLLBACK TO missing_claim_audit; RELEASE missing_claim_audit;")
            .unwrap();
        tx.execute_batch("SAVEPOINT corrupt_claim_journal; DROP TRIGGER journal_entries_no_update; UPDATE journal_entries SET status='reversed' WHERE id=(SELECT journal_entry_id FROM invoice_claim_payments LIMIT 1);").unwrap();
        assert!(!audit_evidence(&tx).unwrap().is_empty());
        tx.execute_batch("ROLLBACK TO corrupt_claim_journal; RELEASE corrupt_claim_journal;")
            .unwrap();
        assert!(audit_evidence(&tx).unwrap().is_empty());
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
            ("synthetic", latest, PrincipalKind::User),
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
    #[test]
    fn service_principal_claim_reminder_registration_post_and_receipt_keep_full_audit() {
        let (mut db, id) = fixture();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let latest = Date::parse("2026-01-07").unwrap();
        tx.execute_batch("SAVEPOINT bankless_source;INSERT INTO account_role_mappings(role,account_no,version,confirmed_by)VALUES('bank','A',1,'synthetic')").unwrap();
        crate::invoice_payment_apply::apply(
            &tx,
            &serde_json::from_value(
                json!({"invoiceDocumentId":id,"paymentDate":"2026-01-05","amount":50}),
            )
            .unwrap(),
            "synthetic-service",
            PrincipalKind::ServiceAccount,
            latest,
        )
        .unwrap();
        assert!(required_banks(&tx, id).unwrap().is_empty());
        let bankless_audit = crate::audit_invoice::verify(&tx).unwrap();
        assert_eq!(bankless_audit["ok"], true, "{bankless_audit}");
        tx.execute_batch("ROLLBACK TO bankless_source;RELEASE bankless_source")
            .unwrap();
        let r = register_with_principal(
            &tx,
            &reg(id),
            "synthetic-service",
            PrincipalKind::ServiceAccount,
        )
        .unwrap();
        assert_eq!(
            tx.query_row::<i64, _, _>("SELECT count(*) FROM journal_entries", [], |r| r.get(0))
                .unwrap(),
            1
        );
        let p = posting(id, r["reminderId"].as_i64().unwrap());
        post_with_principal(
            &tx,
            &p,
            "synthetic-service",
            latest,
            PrincipalKind::ServiceAccount,
        )
        .unwrap();
        assert!(
            post_with_principal(
                &tx,
                &p,
                "synthetic-service",
                latest,
                PrincipalKind::ServiceAccount
            )
            .is_err()
        );
        crate::invoice_settlement::settle(&tx,&serde_json::from_value(json!({"invoiceDocumentId":id,"bankTransactionReference":"reminder-principal","confirm":true})).unwrap(),"synthetic",latest).unwrap();
        for reference in ["reminder-part", "reminder-final"] {
            settle_with_principal(
                &tx,
                &settle_input(&tx, id, reference),
                "synthetic-service",
                latest,
                PrincipalKind::ServiceAccount,
            )
            .unwrap();
        }
        assert!(
            settle_with_principal(
                &tx,
                &settle_input(&tx, id, "reminder-part"),
                "synthetic-service",
                latest,
                PrincipalKind::ServiceAccount
            )
            .is_err()
        );
        assert_eq!(
            read(&tx, id, "2026-01-07").unwrap()["ledgerBackedClaimBalance"],
            json!(0.0)
        );
        assert_eq!(tx.query_row::<i64,_,_>("SELECT count(*) FROM audit_log WHERE actor='service_account:synthetic-service' AND event_type IN ('invoice_reminder_register','invoice_reminder_post','invoice_claim_payment_apply')",[],|r|r.get(0)).unwrap(),4);
        assert_eq!(tx.query_row::<i64,_,_>("SELECT count(*) FROM journal_entries WHERE created_by='user:synthetic-service' AND created_by_program='service-principal'",[],|r|r.get(0)).unwrap(),3);
        assert_eq!(required_banks(&tx, id).unwrap().len(), 3);
        assert!(audit_evidence(&tx).unwrap().is_empty());
        let audit = crate::audit_invoice::verify(&tx).unwrap();
        assert_eq!(audit["ok"], true, "{audit}");
    }
}
