//! Supplier bill recognition and later bank settlement on the admitted company transaction.
use crate::{
    expense::{self, line},
    idempotency::PrincipalKind,
    invalid, legacy_money,
};
use rentemester_engine::{
    Error,
    currency::Currency,
    document::money,
    fiscal_year::Date,
    fx::FxRate,
    identifiers::{BankTransactionId, DocumentId},
    manual_journal::ManualJournal,
    money::Money,
    sales_invoice::{refused, wire},
};
use rusqlite::{OptionalExtension, Transaction, params};
use serde::Deserialize;
use serde_json::{Number, Value, json};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Register {
    pub document_id: i64,
    pub bill_date: String,
    pub due_date: String,
    pub expense_account_no: String,
    pub vat_treatment: Option<String>,
    pub vendor_id: Option<i64>,
    pub note: Option<String>,
    pub confirm: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Payment {
    pub bank_transaction_id: i64,
    pub payment_date: Option<String>,
    pub amount: Option<Number>,
    pub payment_account_no: Option<String>,
    pub note: Option<String>,
    pub confirm: bool,
}
fn audit(
    tx: &Transaction<'_>,
    event: &str,
    id: i64,
    actor: &str,
    principal: PrincipalKind,
) -> Result<(), Error> {
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES(?1,'payable',?2,?3,?4)",params![event,id.to_string(),format!("{event} {id}"),format!("{}:{actor}",match principal {PrincipalKind::User=>"user",PrincipalKind::ServiceAccount=>"service_account"})]).map_err(invalid)?;
    Ok(())
}
fn post(
    tx: &Transaction<'_>,
    request: &ManualJournal,
    actor: &str,
    latest: Date,
    principal: PrincipalKind,
) -> Result<Value, Error> {
    let result = crate::journal_post::post(
        tx,
        request,
        &crate::journal_preview::manual_journal_rule_version()?,
        latest,
        crate::journal_post::Actor {
            subject: &format!("user:{actor}"),
            program: match principal {
                PrincipalKind::User => "rentemester-rust",
                PrincipalKind::ServiceAccount => "service-principal",
            },
        },
    )?;
    if result["ok"] != true {
        return Err(refused(result["errors"].to_string()));
    }
    Ok(result)
}
fn request(date: &str, document: i64, text: String) -> Result<ManualJournal, Error> {
    Ok(ManualJournal {
        transaction_date: Date::parse(date)?,
        text,
        currency: Currency::parse("DKK")?,
        document_id: Some(DocumentId::from_sqlite(document)),
        source_bank_transaction_id: None,
        amount_foreign: None,
        amount_dkk: None,
        fx_rate_to_dkk: None,
        created_by_program: None,
        lines: vec![],
    })
}
pub(crate) struct PreparedBill {
    pub journal: ManualJournal,
    pub gross: Money,
    pub vat: Money,
    pub treatment: String,
    supplier: Option<String>,
    invoice: Option<String>,
}
pub(crate) fn prepare_bill(
    tx: &Transaction<'_>,
    input: &Register,
    proof: Option<&crate::direct_bank_payable_correction::PurchaseProof>,
) -> Result<PreparedBill, Error> {
    if !input.confirm || input.document_id <= 0 || input.vendor_id.is_some_and(|v| v <= 0) {
        return Err(refused(
            "positive document/vendor reference and confirmation required",
        ));
    }
    let bill_date = Date::parse(&input.bill_date)?;
    let due_date = Date::parse(&input.due_date)?;
    if due_date < bill_date {
        return Err(refused("due date cannot precede bill date"));
    }
    let meta = expense::metadata(tx, input.document_id)?;
    if !["purchase_sale", "cash_register_receipt"].contains(&meta.kind())
        || meta.currency() != "DKK"
    {
        return Err(refused(
            "DKK purchase document required for payable registration",
        ));
    }
    if meta.issue_date.as_deref() != Some(input.bill_date.as_str()) {
        return Err(refused(
            "bill date must equal the immutable purchase document date",
        ));
    }
    let used:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM payables WHERE document_id=?1) OR EXISTS(SELECT 1 FROM journal_entries WHERE document_id=?1)",[input.document_id],|r|r.get(0)).map_err(invalid)?;
    if let Some(proof) = proof {
        proof.document_history(tx, input.document_id)?;
    } else if used {
        return Err(refused(
            "purchase document is already booked; use its existing payable or correction workflow",
        ));
    }
    if let Some(id) = input.vendor_id {
        let vendor: Option<(String, Option<String>)> = tx
            .query_row(
                "SELECT name,vat_or_cvr FROM vendors WHERE id=?1 AND archived=0",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(invalid)?;
        let (name, vat) = vendor.ok_or_else(|| refused("active vendor required"))?;
        let sender = meta
            .sender
            .as_ref()
            .ok_or_else(|| refused("purchase supplier identity required for vendor link"))?;
        let normalized = |v: Option<&str>| {
            v.unwrap_or("")
                .split_whitespace()
                .collect::<String>()
                .to_uppercase()
        };
        let recorded: Option<String> = tx
            .query_row(
                "SELECT sender_vat_cvr FROM documents WHERE id=?1",
                [input.document_id],
                |r| r.get(0),
            )
            .map_err(invalid)?;
        let matches = if recorded.as_deref().is_some_and(|s| !s.is_empty()) {
            normalized(recorded.as_deref()) == normalized(vat.as_deref())
        } else {
            vat.as_deref().is_none_or(|v| v.trim().is_empty())
                && sender
                    .name
                    .as_deref()
                    .is_some_and(|n| n.trim().eq_ignore_ascii_case(name.trim()))
        };
        if !matches {
            return Err(refused(
                "selected vendor does not match the purchase document supplier",
            ));
        }
    }
    let gross = meta
        .amount_inc_vat
        .as_ref()
        .map(money)
        .transpose()?
        .ok_or_else(|| refused("purchase gross amount required"))?;
    let vat = meta
        .vat_amount
        .as_ref()
        .map(money)
        .transpose()?
        .unwrap_or_default();
    let registered: Option<String> = tx
        .query_row(
            "SELECT vat_period_type FROM companies WHERE id=1",
            [],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    let treatment = input
        .vat_treatment
        .as_deref()
        .unwrap_or(if vat == Money::default() {
            "exempt"
        } else if registered.is_some() {
            "standard"
        } else {
            "non_deductible"
        });
    let allocation = expense::purchase(
        tx,
        input.document_id,
        &meta,
        &input.expense_account_no,
        Some(treatment),
        &gross,
        &FxRate::parse("1")?,
    )?;
    let creditor = expense::role(tx, "creditors")?;
    let supplier = meta.sender.as_ref().and_then(|p| p.name.as_deref());
    let mut journal = request(
        &input.bill_date,
        input.document_id,
        format!(
            "Leverandørfaktura {}",
            meta.invoice_no.as_deref().unwrap_or("")
        ),
    )?;
    journal.lines = allocation.lines;
    journal.lines.push(line(
        &creditor,
        Money::default(),
        gross.clone(),
        None,
        "Leverandørgæld".into(),
    )?);
    Ok(PreparedBill {
        journal,
        gross,
        vat,
        treatment: treatment.to_owned(),
        supplier: supplier.map(str::to_owned),
        invoice: meta.invoice_no.clone(),
    })
}
pub fn register(
    tx: &Transaction<'_>,
    input: &Register,
    actor: &str,
    latest: Date,
) -> Result<Value, Error> {
    register_with_principal(tx, input, actor, latest, PrincipalKind::User)
}
pub fn register_with_principal(
    tx: &Transaction<'_>,
    input: &Register,
    actor: &str,
    latest: Date,
    principal: PrincipalKind,
) -> Result<Value, Error> {
    register_inner(tx, input, actor, latest, None, principal)
}
pub(crate) fn register_corrected(
    tx: &Transaction<'_>,
    input: &Register,
    actor: &str,
    latest: Date,
    proof: &crate::direct_bank_payable_correction::PurchaseProof,
) -> Result<Value, Error> {
    proof.require_reversed(tx)?;
    register_inner(tx, input, actor, latest, Some(proof), PrincipalKind::User)
}
fn register_inner(
    tx: &Transaction<'_>,
    input: &Register,
    actor: &str,
    latest: Date,
    proof: Option<&crate::direct_bank_payable_correction::PurchaseProof>,
    principal: PrincipalKind,
) -> Result<Value, Error> {
    register_result(tx, input, actor, latest, proof, principal).map(|(value, _)| value)
}
pub(crate) fn register_original(
    tx: &Transaction<'_>,
    input: &Register,
    actor: &str,
    latest: Date,
    principal: PrincipalKind,
) -> Result<Value, Error> {
    let (mut value, posted) = register_result(tx, input, actor, latest, None, principal)?;
    value["entryHash"] = posted["entryHash"].clone();
    let mut rules = vec![json!("DK-PAYABLE-001")];
    if let Some(native) = posted["appliedRules"].as_array() {
        rules.extend(native.iter().cloned());
    }
    value["appliedRules"] = json!(rules);
    Ok(value)
}
fn register_result(
    tx: &Transaction<'_>,
    input: &Register,
    actor: &str,
    latest: Date,
    proof: Option<&crate::direct_bank_payable_correction::PurchaseProof>,
    principal: PrincipalKind,
) -> Result<(Value, Value), Error> {
    crate::journal_post::require_current(tx)?;
    let PreparedBill {
        journal,
        gross,
        vat,
        treatment: _,
        supplier,
        invoice,
    } = prepare_bill(tx, input, proof)?;
    let result = post(tx, &journal, actor, latest, principal)?;
    let id:i64=tx.query_row("INSERT INTO payables(document_id,vendor_id,supplier_name,bill_no,bill_date,due_date,gross_amount,net_amount,vat_amount,currency,journal_entry_id,note) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,'DKK',?10,?11) RETURNING id",params![input.document_id,input.vendor_id,supplier,invoice,input.bill_date,input.due_date,wire(&gross)?.as_f64(),wire(&gross.subtract(&vat))?.as_f64(),wire(&vat)?.as_f64(),result["entryId"].as_i64(),input.note],|r|r.get(0)).map_err(invalid)?;
    audit(tx, "payable_register", id, actor, principal)?;
    Ok((
        json!({"payableId":id,"documentId":input.document_id,"supplierName":supplier,"billNo":invoice,"grossAmount":wire(&gross)?,"netAmount":wire(&gross.subtract(&vat))?,"vatAmount":wire(&vat)?,"dueDate":input.due_date,"entryId":result["entryId"],"entryNo":result["entryNo"]}),
        result,
    ))
}

pub fn document_id(tx: &Transaction<'_>, id: i64) -> Result<i64, Error> {
    tx.query_row("SELECT document_id FROM payables WHERE id=?1", [id], |r| {
        r.get(0)
    })
    .optional()
    .map_err(invalid)?
    .ok_or_else(|| refused("payable not found"))
}
struct Bill {
    document: i64,
    gross: Money,
    bill_date: String,
    entry: i64,
}
fn bill(tx: &Transaction<'_>, id: i64) -> Result<Bill, Error> {
    let (document,gross,bill_date,entry,currency,status,reversed):(i64,Money,String,i64,String,String,bool)=tx.query_row("SELECT p.document_id,p.gross_amount,p.bill_date,p.journal_entry_id,p.currency,j.status,EXISTS(SELECT 1 FROM journal_entries r WHERE r.reversal_of_entry_id=j.id) FROM payables p JOIN journal_entries j ON j.id=p.journal_entry_id WHERE p.id=?1",[id],|r|Ok((r.get(0)?,legacy_money(r.get_ref(1)?).map_err(|_|rusqlite::Error::InvalidQuery)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?))).optional().map_err(invalid)?.ok_or_else(||refused("payable not found"))?;
    if currency != "DKK" || status != "posted" || reversed {
        return Err(refused("payable original evidence requires resolution"));
    }
    Ok(Bill {
        document,
        gross,
        bill_date,
        entry,
    })
}
fn paid(tx: &Transaction<'_>, id: i64, as_of: Option<&str>) -> Result<Money, Error> {
    let mut total = Money::default();
    let mut q=tx.prepare("SELECT p.amount,p.currency,j.status,EXISTS(SELECT 1 FROM journal_entries r WHERE r.reversal_of_entry_id=j.id),p.payment_date FROM payable_payments p JOIN journal_entries j ON j.id=p.journal_entry_id WHERE p.payable_id=?1").map_err(invalid)?;
    let mut rows = q.query([id]).map_err(invalid)?;
    while let Some(r) = rows.next().map_err(invalid)? {
        if r.get::<_, String>(1).map_err(invalid)? != "DKK"
            || r.get::<_, String>(2).map_err(invalid)? != "posted"
            || r.get::<_, bool>(3).map_err(invalid)?
        {
            return Err(refused("payable payment evidence requires resolution"));
        }
        if as_of.is_none_or(|date| r.get::<_, String>(4).is_ok_and(|d| d.as_str() <= date)) {
            total.add(&legacy_money(r.get_ref(0).map_err(invalid)?)?);
        }
    }
    Ok(total)
}
pub fn pay(
    tx: &Transaction<'_>,
    id: i64,
    input: &Payment,
    actor: &str,
    latest: Date,
) -> Result<Value, Error> {
    pay_with_principal(tx, id, input, actor, latest, PrincipalKind::User)
}
pub fn pay_with_principal(
    tx: &Transaction<'_>,
    id: i64,
    input: &Payment,
    actor: &str,
    latest: Date,
    principal: PrincipalKind,
) -> Result<Value, Error> {
    pay_inner(tx, id, input, actor, latest, None, principal)
}
pub(crate) fn pay_corrected(
    tx: &Transaction<'_>,
    id: i64,
    input: &Payment,
    actor: &str,
    latest: Date,
    proof: &crate::direct_bank_payable_correction::PurchaseProof,
) -> Result<Value, Error> {
    proof.require_reversed(tx)?;
    pay_inner(
        tx,
        id,
        input,
        actor,
        latest,
        Some(proof),
        PrincipalKind::User,
    )
}
fn pay_inner(
    tx: &Transaction<'_>,
    id: i64,
    input: &Payment,
    actor: &str,
    latest: Date,
    proof: Option<&crate::direct_bank_payable_correction::PurchaseProof>,
    principal: PrincipalKind,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    if !input.confirm || id <= 0 || input.bank_transaction_id <= 0 {
        return Err(refused(
            "positive payable/bank reference and confirmation required",
        ));
    }
    let original = bill(tx, id)?;
    let open = original.gross.subtract(&paid(tx, id, None)?);
    let (date,signed,currency,mapping,bank_id,bank_currency):(String,Money,String,Option<String>,Option<i64>,Option<String>)=tx.query_row("SELECT b.transaction_date,b.amount,b.currency,a.ledger_account_no,b.bank_account_id,a.currency FROM bank_transactions b LEFT JOIN bank_accounts a ON a.id=b.bank_account_id WHERE b.id=?1",[input.bank_transaction_id],|r|Ok((r.get(0)?,crate::bank::signed_money(r.get_ref(1)?).map_err(|_|rusqlite::Error::InvalidQuery)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).map_err(invalid)?;
    if currency != "DKK"
        || !signed.is_negative()
        || bank_id.is_some() && (mapping.is_none() || bank_currency.as_deref() != Some("DKK"))
    {
        return Err(refused(
            "outgoing DKK bank transaction with compatible mapped account required",
        ));
    }
    let amount = Money::default().subtract(&signed);
    if open == Money::default() || open.is_negative() || open.subtract(&amount).is_negative() {
        return Err(refused("payment exceeds the payable's open balance"));
    }
    if input
        .amount
        .as_ref()
        .map(money)
        .transpose()?
        .is_some_and(|a| a != amount)
        || input.payment_date.as_deref().is_some_and(|d| d != date)
        || date < original.bill_date
    {
        return Err(refused(
            "payment amount/date must equal the bank transaction and not precede the bill",
        ));
    }
    let used:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM bank_journal_reconciliations WHERE bank_transaction_id=?1) OR EXISTS(SELECT 1 FROM payable_payments WHERE bank_transaction_id=?1)",[input.bank_transaction_id],|r|r.get(0)).map_err(invalid)?;
    if let Some(proof) = proof {
        proof.payment_history(tx, original.document, input.bank_transaction_id, id)?;
    } else if used {
        return Err(refused("bank transaction is already reconciled"));
    }
    let bank = match mapping {
        Some(account) => account,
        None => expense::role(tx, "bank")?,
    };
    if input
        .payment_account_no
        .as_deref()
        .is_some_and(|no| no.trim() != bank)
    {
        return Err(refused(
            "payment account must match the imported bank account",
        ));
    }
    let valid:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM accounts WHERE account_no=?1 AND type='asset' AND normal_balance='debit' AND active=1)",[&bank],|r|r.get(0)).map_err(invalid)?;
    if !valid {
        return Err(refused("active mapped bank ledger account required"));
    }
    let mut q=tx.prepare("SELECT a.account_no,j.credit_amount FROM journal_lines j JOIN accounts a ON a.id=j.account_id WHERE j.journal_entry_id=?1 AND a.type='liability' AND a.normal_balance='credit' AND a.active=1 AND j.credit_amount>0").map_err(invalid)?;
    let mut rows = q.query([original.entry]).map_err(invalid)?;
    let row = rows
        .next()
        .map_err(invalid)?
        .ok_or_else(|| refused("original payable creditor identity unavailable"))?;
    let creditor: String = row.get(0).map_err(invalid)?;
    if legacy_money(row.get_ref(1).map_err(invalid)?)? != original.gross
        || rows.next().map_err(invalid)?.is_some()
    {
        return Err(refused("original payable creditor identity is ambiguous"));
    }
    let mut carrying = Money::default();
    let mut q=tx.prepare("SELECT l.debit_amount,l.credit_amount FROM journal_lines l JOIN accounts a ON a.id=l.account_id WHERE a.account_no=?1 AND (l.journal_entry_id=?2 OR l.journal_entry_id IN (SELECT journal_entry_id FROM payable_payments WHERE payable_id=?3))").map_err(invalid)?;
    let mut rows = q
        .query(params![creditor, original.entry, id])
        .map_err(invalid)?;
    while let Some(r) = rows.next().map_err(invalid)? {
        carrying.add(&legacy_money(r.get_ref(1).map_err(invalid)?)?);
        carrying = carrying.subtract(&legacy_money(r.get_ref(0).map_err(invalid)?)?);
    }
    if carrying != open {
        return Err(refused("payable ledger and open-item evidence disagree"));
    }
    let mut journal = request(
        &date,
        original.document,
        format!("Betaling af leverandørfaktura {id}"),
    )?;
    if proof.is_none() {
        journal.source_bank_transaction_id =
            Some(BankTransactionId::from_sqlite(input.bank_transaction_id));
    }
    journal.lines = vec![
        line(
            &creditor,
            amount.clone(),
            Money::default(),
            None,
            "Udligning af leverandørgæld".into(),
        )?,
        line(
            &bank,
            Money::default(),
            amount.clone(),
            None,
            "Bankbetaling".into(),
        )?,
    ];
    let result = post(tx, &journal, actor, latest, principal)?;
    let payment:i64=tx.query_row("INSERT INTO payable_payments(payable_id,bank_transaction_id,journal_entry_id,payment_date,amount,currency,note) VALUES(?1,?2,?3,?4,?5,'DKK',?6) RETURNING id",params![id,input.bank_transaction_id,result["entryId"].as_i64(),date,wire(&amount)?.as_f64(),input.note],|r|r.get(0)).map_err(invalid)?;
    audit(tx, "payable_payment_apply", id, actor, principal)?;
    Ok(
        json!({"paymentId":payment,"journalEntryId":result["entryId"],"payableId":id,"openBalance":wire(&open.subtract(&amount))?}),
    )
}
pub(crate) fn dated_rows(tx: &Transaction<'_>, as_of: &str, status: &str) -> Result<Value, Error> {
    Date::parse(as_of)?;
    if !["open", "paid", "overdue", "all"].contains(&status) {
        return Err(refused("invalid payable status filter"));
    }
    let mut q=tx.prepare("SELECT id,document_id,bill_no,bill_date,due_date,supplier_name,vendor_id,gross_amount FROM payables WHERE bill_date<=?1 ORDER BY due_date,id").map_err(invalid)?;
    let mut result = q.query([as_of]).map_err(invalid)?;
    let mut rows = vec![];
    let mut total = Money::default();
    let mut overdue = Money::default();
    while let Some(r) = result.next().map_err(invalid)? {
        let id: i64 = r.get(0).map_err(invalid)?;
        let original = bill(tx, id)?;
        let paid = paid(tx, id, Some(as_of))?;
        let open = original.gross.subtract(&paid);
        if open.is_negative() {
            return Err(refused("payable payments exceed original gross"));
        }
        let due: String = r.get(4).map_err(invalid)?;
        Date::parse(&due)?;
        let days: i64 = if open == Money::default() {
            0
        } else {
            tx.query_row(
                "SELECT MAX(0,CAST(julianday(?1)-julianday(?2) AS INTEGER))",
                params![as_of, due],
                |r| r.get(0),
            )
            .map_err(invalid)?
        };
        let closed = open == Money::default();
        if status == "open" && closed
            || status == "paid" && !closed
            || status == "overdue" && days == 0
        {
            continue;
        }
        total.add(&open);
        if days > 0 {
            overdue.add(&open);
        }
        rows.push(json!({"payableId":id,"documentId":r.get::<_,i64>(1).map_err(invalid)?,"billNo":r.get::<_,Option<String>>(2).map_err(invalid)?,"billDate":r.get::<_,String>(3).map_err(invalid)?,"dueDate":due,"supplierName":r.get::<_,Option<String>>(5).map_err(invalid)?,"vendorId":r.get::<_,Option<i64>>(6).map_err(invalid)?,"grossAmount":wire(&original.gross)?,"currency":"DKK","paidAmount":wire(&paid)?,"openBalance":wire(&open)?,"status":if closed{"paid"}else{"open"},"isOverdue":days>0,"overdueDays":days,"agingBucket":if days==0{"not-due"}else if days<=30{"0-30"}else if days<=60{"31-60"}else if days<=90{"61-90"}else{"90+"}}));
    }
    Ok(
        json!({"asOfDate":as_of,"status":status,"count":rows.len(),"rows":rows,"totalOpenBalance":wire(&total)?,"overdueOpenBalance":wire(&overdue)?,"notYetDueOpenBalance":wire(&total.subtract(&overdue))?}),
    )
}
pub fn list(tx: &Transaction<'_>, slug: &str, as_of: &str, status: &str) -> Result<Value, Error> {
    let financial = dated_rows(tx, as_of, status)?;
    let mut base = crate::sales::context(tx, slug, as_of, None)?;
    for (key, value) in financial.as_object().expect("dated rows object") {
        base[key] = value.clone();
    }
    let mut q=tx.prepare("SELECT d.id,d.document_no,d.invoice_no,d.invoice_date,d.supplier_name,d.amount_inc_vat,d.vat_amount,d.currency FROM documents d WHERE d.document_type IN ('purchase_sale','cash_register_receipt') AND d.currency='DKK' AND NOT EXISTS(SELECT 1 FROM payables p WHERE p.document_id=d.id) AND NOT EXISTS(SELECT 1 FROM journal_entries j WHERE j.document_id=d.id) ORDER BY d.invoice_date DESC,d.id DESC LIMIT 200").map_err(invalid)?;
    let docs=q.query_map([],|r|Ok(json!({"id":r.get::<_,i64>(0)?,"documentNo":r.get::<_,Option<String>>(1)?,"invoiceNo":r.get::<_,Option<String>>(2)?,"invoiceDate":r.get::<_,Option<String>>(3)?,"supplierName":r.get::<_,Option<String>>(4)?,"amountIncVat":r.get::<_,Option<f64>>(5)?,"vatAmount":r.get::<_,Option<f64>>(6)?,"currency":r.get::<_,String>(7)?}))).map_err(invalid)?.collect::<Result<Vec<_>,_>>().map_err(invalid)?;
    let mut q=tx.prepare("SELECT account_no,name,default_vat_code FROM accounts WHERE type='expense' AND active=1 ORDER BY account_no").map_err(invalid)?;
    let accounts=q.query_map([],|r|Ok(json!({"accountNo":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"defaultVatCode":r.get::<_,Option<String>>(2)?}))).map_err(invalid)?.collect::<Result<Vec<_>,_>>().map_err(invalid)?;
    let vendors = crate::sales::contacts(tx, slug, as_of)?["vendors"].clone();
    for (key, value) in [
        ("unregisteredDocuments", json!(docs)),
        (
            "directBankCorrectionCandidates",
            crate::direct_bank_payable_correction::candidates(tx)?,
        ),
        ("expenseAccounts", json!(accounts)),
        ("vendors", vendors),
    ] {
        base[key] = value;
    }
    Ok(base)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recognize_then_partially_and_fully_settle_without_repeated_expense_or_vat() {
        recognition_flow(PrincipalKind::User);
    }
    #[test]
    fn service_payable_partial_full_payment_preserves_original_creditor_expense_vat_and_attribution()
     {
        recognition_flow(PrincipalKind::ServiceAccount);
    }
    fn recognition_flow(principal: PrincipalKind) {
        let register =
            |tx: &Transaction<'_>, input: &Register, actor: &str, latest: Date| match principal {
                PrincipalKind::User => super::register(tx, input, actor, latest),
                PrincipalKind::ServiceAccount => {
                    register_with_principal(tx, input, actor, latest, principal)
                }
            };
        let pay = |tx: &Transaction<'_>, id: i64, input: &Payment, actor: &str, latest: Date| {
            match principal {
                PrincipalKind::User => super::pay(tx, id, input, actor, latest),
                PrincipalKind::ServiceAccount => {
                    pay_with_principal(tx, id, input, actor, latest, principal)
                }
            }
        };
        let mut control = rusqlite::Connection::open_in_memory().unwrap();
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic Company").unwrap();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        tx.execute_batch("UPDATE companies SET cvr='87654321',address='Synthetic 1',vat_period_type='quarter' WHERE id=1; INSERT INTO accounts(account_no,name,type,normal_balance,default_vat_code) VALUES('E','Synthetic expense','expense','debit','DK_PURCHASE_25'),('V','Synthetic input VAT','vat','debit',NULL),('C','Synthetic creditor','liability','credit',NULL),('C2','Synthetic later creditor','liability','credit',NULL),('B','Synthetic bank','asset','debit',NULL);INSERT INTO account_role_mappings(role,account_no,version,confirmed_by) VALUES('creditors','C',1,'synthetic'),('input_vat','V',1,'synthetic');INSERT INTO bank_accounts(slug,name,ledger_account_no) VALUES('synthetic','Synthetic bank','B');").unwrap();
        let meta:rentemester_engine::document::Metadata=serde_json::from_value(json!({"source":"synthetic","documentType":"purchase_sale","issueDate":"2026-10-17","invoiceNo":"SYN-UNPAID-1","deliveryDescription":"Synthetic supplies","amountIncVat":1250,"vatAmount":250,"sender":{"name":"Synthetic Supplier","address":"Synthetic 2","vatOrCvr":"DK12345678"},"recipient":{"name":"Synthetic Company","address":"Synthetic 1","vatOrCvr":"DK87654321"}})).unwrap();
        let document = crate::documents::register(
            &tx,
            crate::documents::Original {
                filename: "synthetic.txt",
                path: "synthetic.txt",
                hash: &"e".repeat(64),
                mime: "text/plain",
            },
            &meta,
            "synthetic",
            "2026-10-18",
        )
        .unwrap()["document"]["id"]
            .as_i64()
            .unwrap();
        let original: String = tx
            .query_row(
                "SELECT payload_json FROM documents WHERE id=?1",
                [document],
                |r| r.get(0),
            )
            .unwrap();
        let register_input = |date: &str| {
            serde_json::from_value::<Register>(json!({"documentId":document,"billDate":date,"dueDate":"2026-10-17","expenseAccountNo":"E","confirm":true})).unwrap()
        };
        let latest = Date::parse("2026-10-19").unwrap();
        assert!(register(&tx, &register_input("2026-10-18"), "synthetic", latest).is_err());
        let result = register(&tx, &register_input("2026-10-17"), "synthetic", latest).unwrap();
        let id = result["payableId"].as_i64().unwrap();
        assert!(register(&tx, &register_input("2026-10-17"), "synthetic", latest).is_err());
        let listed = list(&tx, "synthetic", "2026-10-18", "overdue").unwrap();
        assert_eq!(listed["totalOpenBalance"].as_f64(), Some(1250.0));
        assert_eq!(listed["rows"][0]["overdueDays"], 1);
        assert_eq!(listed["rows"][0]["agingBucket"], "0-30");
        assert_eq!(listed["unregisteredDocuments"], json!([]));
        let csv = "date;text;amount;currency;reference\n2026-10-18;Synthetic partial;-500;DKK;first\n2026-10-19;Synthetic final;-750;DKK;second\n2026-10-19;Synthetic excess;-2000;DKK;too-large\n2026-10-19;Synthetic incoming;1;DKK;incoming\n";
        crate::bank::import(
            &tx,
            csv,
            &rentemester_engine::bank::parse(csv).unwrap(),
            Some("synthetic"),
            "synthetic",
            "2026-10-19T12:00:00Z",
            "synthetic-original",
        )
        .unwrap();
        tx.execute_batch("UPDATE account_role_mappings SET status='superseded' WHERE role='creditors';INSERT INTO account_role_mappings(role,account_no,version,confirmed_by) VALUES('creditors','C2',2,'synthetic');").unwrap();
        let payment = |reference: &str, amount: Option<i64>| {
            let bank: i64 = tx
                .query_row(
                    "SELECT id FROM bank_transactions WHERE reference=?1",
                    [reference],
                    |r| r.get(0),
                )
                .unwrap();
            serde_json::from_value::<Payment>(
                json!({"bankTransactionId":bank,"amount":amount,"confirm":true}),
            )
            .unwrap()
        };
        for input in [
            payment("too-large", None),
            payment("incoming", None),
            payment("first", Some(499)),
        ] {
            assert!(pay(&tx, id, &input, "synthetic", latest).is_err());
        }
        let first = pay(&tx, id, &payment("first", None), "synthetic", latest).unwrap();
        assert_eq!(first["openBalance"].as_f64(), Some(750.0));
        assert!(pay(&tx, id, &payment("first", None), "synthetic", latest).is_err());
        assert_eq!(
            pay(&tx, id, &payment("second", None), "synthetic", latest).unwrap()["openBalance"]
                .as_f64(),
            Some(0.0)
        );
        assert_eq!(
            list(&tx, "synthetic", "2026-10-18", "all").unwrap()["rows"][0]["openBalance"].as_f64(),
            Some(750.0)
        );
        assert_eq!(
            list(&tx, "synthetic", "2026-10-19", "paid").unwrap()["rows"][0]["status"],
            "paid"
        );
        let snapshot = crate::cockpit::read(&tx, "synthetic", "2026-10-19", None, None).unwrap();
        assert_eq!(
            snapshot.income_statement["totalExpense"].as_f64(),
            Some(1000.0)
        );
        assert_eq!(snapshot.trial_balance["balanced"], true);
        let creditor = snapshot.trial_balance["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["accountNo"] == "C")
            .unwrap();
        assert_eq!(creditor["balance"].as_f64(), Some(0.0));
        let vat = crate::vat_report::read(&tx, "synthetic", "2026-10-19", None).unwrap();
        assert_eq!(vat["inputVat"].as_f64(), Some(250.0));
        assert_eq!(vat["vatReportErrors"], json!([]));
        assert_eq!(
            tx.query_row::<String, _, _>(
                "SELECT payload_json FROM documents WHERE id=?1",
                [document],
                |r| r.get(0)
            )
            .unwrap(),
            original
        );
        assert_eq!(
            tx.query_row::<i64, _, _>("SELECT count(*) FROM journal_entries", [], |r| r.get(0))
                .unwrap(),
            3
        );
        assert_eq!(
            tx.query_row::<i64, _, _>(
                "SELECT count(*) FROM bank_journal_reconciliations",
                [],
                |r| r.get(0)
            )
            .unwrap(),
            2
        );
        let program = match principal {
            PrincipalKind::User => "rentemester-rust",
            PrincipalKind::ServiceAccount => "service-principal",
        };
        assert_eq!(tx.query_row::<i64,_,_>("SELECT count(*) FROM journal_entries WHERE created_by='user:synthetic' AND created_by_program=?1",[program],|r|r.get(0)).unwrap(),3);
        let actor = match principal {
            PrincipalKind::User => "user:synthetic",
            PrincipalKind::ServiceAccount => "service_account:synthetic",
        };
        assert_eq!(tx.query_row::<i64,_,_>("SELECT count(*) FROM audit_log WHERE event_type IN ('payable_register','payable_payment_apply') AND actor=?1",[actor],|r|r.get(0)).unwrap(),3);
        for entry in [
            result["entryId"].as_i64().unwrap(),
            first["journalEntryId"].as_i64().unwrap(),
        ] {
            let reverse=serde_json::from_value::<crate::journal_reversal::Input>(json!({"entryId":entry.to_string(),"transactionDate":"2026-10-19","reason":"Synthetic refusal"})).unwrap();
            assert_eq!(
                crate::journal_reversal::reverse(
                    &tx,
                    &reverse,
                    &crate::journal_preview::manual_journal_rule_version().unwrap(),
                    latest,
                    crate::journal_post::Actor {
                        subject: "user:synthetic",
                        program: "synthetic"
                    }
                )
                .unwrap()["ok"],
                false
            );
        }
    }
}
