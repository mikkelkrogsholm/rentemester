//! Source-defined established-loss writeoff. Original currency counters and
//! actual retained DKK carrying/VAT are different units and proofs.
use crate::{invalid, legacy_money};
use rentemester_engine::{
    Error,
    currency::Currency,
    fiscal_year::Date,
    fx::FxRate,
    identifiers::{AccountNo, DocumentId},
    manual_journal::{JournalLine, ManualJournal},
    money::Money,
    sales_invoice::wire,
};
use rusqlite::{Transaction, params};
use serde::Deserialize;
use serde_json::{Number, Value, json};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WriteOff {
    pub invoice_document_id: i64,
    pub write_off_date: String,
    pub gross_amount: Option<Number>,
    pub expense_account_no: String,
    pub note: Option<String>,
    pub confirm: bool,
}
fn refused(message: impl Into<String>) -> Error {
    Error::new("bad_debt_refused", message)
}
#[derive(Default)]
pub(crate) struct History {
    pub gross: Money,
    pub gross_dkk: Money,
    pub entries: Vec<Value>,
}
pub(crate) fn history(tx: &Transaction<'_>, id: i64) -> Result<History, Error> {
    let mut out = History::default();
    let mut q=tx.prepare("SELECT w.id,w.writeoff_date,w.gross_amount,w.net_amount,w.vat_amount,w.journal_entry_id,j.entry_no,e.expected_gross_ore,e.expected_net_ore,e.expected_vat_ore FROM invoice_bad_debt_writeoffs w LEFT JOIN journal_entries j ON j.id=w.journal_entry_id LEFT JOIN invoice_bad_debt_writeoff_journal_evidence e ON e.writeoff_id=w.id WHERE w.invoice_document_id=?1 ORDER BY w.journal_entry_id,w.id").map_err(invalid)?;
    let mut rs = q.query([id]).map_err(invalid)?;
    while let Some(r) = rs.next().map_err(invalid)? {
        let wid: i64 = r.get(0).map_err(invalid)?;
        crate::audit_invoice::bad_debt_evidence(tx, wid)?;
        let date: String = r.get(1).map_err(invalid)?;
        Date::parse(&date)?;
        let gross = legacy_money(r.get_ref(2).map_err(invalid)?)?;
        let net = legacy_money(r.get_ref(3).map_err(invalid)?)?;
        let vat = legacy_money(r.get_ref(4).map_err(invalid)?)?;
        let cents = |n: i64| -> Result<Money, Error> {
            if n < 0 {
                return Err(refused("negative retained writeoff split"));
            }
            Money::parse(&format!("{}.{:02}", n / 100, n % 100))
        };
        let gd = cents(r.get(7).map_err(invalid)?)?;
        let nd = cents(r.get(8).map_err(invalid)?)?;
        let vd = cents(r.get(9).map_err(invalid)?)?;
        out.gross.add(&gross);
        out.gross_dkk.add(&gd);
        out.entries.push(json!({"writeOffId":wid,"writeOffDate":date,"grossAmount":wire(&gross)?,"netAmount":wire(&net)?,"vatAmount":wire(&vat)?,"grossAmountDkk":wire(&gd)?,"netAmountDkk":wire(&nd)?,"vatAmountDkk":wire(&vd)?,"journalEntryId":r.get::<_,i64>(5).map_err(invalid)?,"journalEntryNo":r.get::<_,String>(6).map_err(invalid)?}));
    }
    Ok(out)
}
/// Known principal effects are independently authenticated before comparing
/// them to the complete principal carrying (which also catches unknown lines).
pub(crate) fn adjusted_carrying(
    tx: &Transaction<'_>,
    id: i64,
    debtor: i64,
) -> Result<Money, Error> {
    adjusted_carrying_through(tx, id, debtor, None)
}
/// Actual principal carrying through the source journal transaction date.
/// Claim postings and claim receipts are absent from the canonical effect view.
pub(crate) fn adjusted_carrying_through(
    tx: &Transaction<'_>,
    id: i64,
    debtor: i64,
    as_of: Option<&str>,
) -> Result<Money, Error> {
    if let Some(date) = as_of {
        Date::parse(date)?;
    }
    crate::audit_invoice::adjusted_principal_evidence(tx, id)?;
    let mut q=tx.prepare("SELECT l.debit_amount,l.credit_amount FROM invoice_principal_receivable_effects e JOIN journal_entries j ON j.id=e.journal_entry_id JOIN journal_lines l ON l.journal_entry_id=e.journal_entry_id AND l.account_id=e.receivable_account_id WHERE e.invoice_document_id=?1 AND e.receivable_account_id=?2 AND (?3 IS NULL OR j.transaction_date<=?3) ORDER BY e.journal_entry_id,l.id").map_err(invalid)?;
    let mut rs = q.query(params![id, debtor, as_of]).map_err(invalid)?;
    let mut total = Money::default();
    while let Some(r) = rs.next().map_err(invalid)? {
        total.add(
            &legacy_money(r.get_ref(0).map_err(invalid)?)?
                .subtract(&legacy_money(r.get_ref(1).map_err(invalid)?)?),
        );
    }
    Ok(total)
}
fn collect(tx: &Transaction<'_>, sql: &str, id: i64, date: &str) -> Result<Money, Error> {
    let mut q = tx.prepare(sql).map_err(invalid)?;
    let mut rs = q.query(params![id, date]).map_err(invalid)?;
    let mut total = Money::default();
    while let Some(r) = rs.next().map_err(invalid)? {
        total.add(&legacy_money(r.get_ref(0).map_err(invalid)?)?);
    }
    Ok(total)
}
fn allocation(
    gross: &Money,
    vat: &Money,
    cumulative: &Money,
    prior: &Money,
) -> Result<Money, Error> {
    let total = if cumulative == &Money::default() {
        Money::default()
    } else {
        crate::invoice_fx::portion(gross, vat, &Money::default(), cumulative)?
    };
    let delta = total.subtract(prior);
    if delta.is_negative() {
        return Err(refused("prior VAT relief exceeds cumulative frozen basis"));
    }
    Ok(delta)
}
fn account(
    tx: &Transaction<'_>,
    no: &str,
    kind: &str,
    normal: &str,
    direct: bool,
) -> Result<(), Error> {
    let yes:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM accounts WHERE account_no=?1 AND type=?2 AND normal_balance=?3 AND active=1 AND (?4=0 OR allow_direct_posting=1))",params![no,kind,normal,direct],|r|r.get(0)).map_err(invalid)?;
    if yes {
        Ok(())
    } else {
        Err(refused(
            "active compatible account required for bad-debt posting",
        ))
    }
}
pub fn write_off(
    tx: &Transaction<'_>,
    input: &WriteOff,
    subject: &str,
    latest: Date,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    if !input.confirm || !(1..=9007199254740991).contains(&input.invoice_document_id) {
        return Err(refused(
            "explicit established-loss confirmation and positive exact invoice id required",
        ));
    }
    if subject.trim().is_empty() || subject.len() > 256 || subject.chars().any(char::is_control) {
        return Err(refused("trusted bounded human actor required"));
    }
    if input.note.as_ref().is_some_and(|s| s.len() > 4096) {
        return Err(refused("writeoff note exceeds4096 bytes"));
    }
    let date = Date::parse(&input.write_off_date)?;
    let id = input.invoice_document_id;
    let (no,issued,currency,raw,hash,debtor,debtor_id):(String,String,String,String,String,String,i64)=tx.query_row("SELECT d.invoice_no,d.invoice_date,d.currency,d.payload_json,d.sha256_hash,a.account_no,p.receivable_account_id FROM documents d JOIN issued_invoice_postings p ON p.invoice_document_id=d.id JOIN journal_entries j ON j.id=p.journal_entry_id JOIN accounts a ON a.id=p.receivable_account_id WHERE d.id=?1 AND d.document_type='issued_invoice' AND j.status='posted' AND j.document_id=d.id AND j.transaction_date=d.invoice_date AND j.source_bank_transaction_id IS NULL AND j.reversal_of_entry_id IS NULL AND NOT EXISTS(SELECT 1 FROM journal_entries r WHERE r.reversal_of_entry_id=j.id)",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?))).map_err(invalid)?;
    let payload: Value = serde_json::from_str(&raw).map_err(invalid)?;
    if crate::invoice_email::sha(raw.as_bytes()) != hash
        || payload["invoiceNumber"] != no
        || payload["issueDate"] != issued
        || payload["currency"] != currency
    {
        return Err(refused("immutable invoice snapshot identity/hash differs"));
    }
    if payload["vatTreatment"] != "standard" {
        return Err(refused(
            "bad-debt relief requires a standard-rated issued invoice",
        ));
    }
    if date < Date::parse(&issued)? {
        return Err(refused("writeoff cannot predate invoice"));
    }
    let original = rentemester_engine::sales_invoice::project(&payload)?;
    let frozen = crate::invoice_fx::projection(&payload)?;
    if original.gross == Money::default()
        || original.gross.is_negative()
        || original.vat == Money::default()
        || original.vat.is_negative()
        || frozen.vat == Money::default()
    {
        return Err(refused(
            "positive original gross and VAT with frozen DKK totals required",
        ));
    }
    let (current_open, unit) = crate::reminder_claims::principal_open(tx, id)?;
    if unit != currency {
        return Err(refused("principal currency differs from immutable invoice"));
    }
    crate::audit_invoice::adjusted_principal_evidence(tx, id)?;
    let paid = collect(
        tx,
        "SELECT amount FROM invoice_payments WHERE invoice_document_id=?1 AND payment_date<=?2",
        id,
        &input.write_off_date,
    )?;
    let refunded = collect(
        tx,
        "SELECT amount FROM invoice_refunds WHERE invoice_document_id=?1 AND refund_date<=?2",
        id,
        &input.write_off_date,
    )?;
    let credits = collect(
        tx,
        "SELECT d.amount_inc_vat FROM credit_note_postings p JOIN documents d ON d.id=p.credit_note_document_id WHERE p.original_invoice_document_id=?1 AND d.invoice_date<=?2",
        id,
        &input.write_off_date,
    )?;
    let prior = collect(
        tx,
        "SELECT gross_amount FROM invoice_bad_debt_writeoffs WHERE invoice_document_id=?1 AND writeoff_date<=?2",
        id,
        &input.write_off_date,
    )?;
    let mut open = original
        .gross
        .subtract(&paid)
        .subtract(&credits)
        .subtract(&prior);
    open.add(&refunded);
    let amount = input
        .gross_amount
        .as_ref()
        .map(|n| crate::invoice_fx::amount(&Value::Number(n.clone())))
        .transpose()?
        .unwrap_or_else(|| open.clone());
    if amount == Money::default()
        || amount.is_negative()
        || open.subtract(&amount).is_negative()
        || current_open.subtract(&amount).is_negative()
    {
        return Err(refused(
            "positive writeoff cannot exceed date-based/current open principal",
        ));
    }
    let carrying = crate::invoice_settlement::principal_carrying(tx, id, debtor_id)?;
    let gross_dkk = if currency == "DKK" {
        if carrying != current_open {
            return Err(refused("principal carrying differs"));
        }
        amount.clone()
    } else {
        crate::invoice_fx::portion(&current_open, &carrying, &Money::default(), &amount)?
    };
    let mut relieved = credits;
    relieved.add(&prior);
    let mut cumulative = relieved.clone();
    cumulative.add(&amount);
    if original.gross.subtract(&cumulative).is_negative() {
        return Err(refused("cumulative credit/writeoff relief exceeds invoice"));
    }
    let prior_vat = collect(
        tx,
        "SELECT d.vat_amount FROM credit_note_postings p JOIN documents d ON d.id=p.credit_note_document_id WHERE p.original_invoice_document_id=?1 AND d.invoice_date<=?2 UNION ALL SELECT vat_amount FROM invoice_bad_debt_writeoffs WHERE invoice_document_id=?1 AND writeoff_date<=?2",
        id,
        &input.write_off_date,
    )?;
    let original_output = collect(
        tx,
        "SELECT l.credit_amount FROM issued_invoice_postings p JOIN journal_lines l ON l.journal_entry_id=p.journal_entry_id JOIN accounts a ON a.id=l.account_id WHERE p.invoice_document_id=?1 AND a.type='vat' AND a.normal_balance='credit' AND l.debit_amount=0 AND l.credit_amount>0 AND l.vat_code IS NULL AND ?2 IS NOT NULL",
        id,
        &input.write_off_date,
    )?;
    let prior_output = collect(
        tx,
        "SELECT l.debit_amount FROM credit_note_postings p JOIN documents d ON d.id=p.credit_note_document_id JOIN journal_lines l ON l.journal_entry_id=p.journal_entry_id JOIN accounts a ON a.id=l.account_id WHERE p.original_invoice_document_id=?1 AND d.invoice_date<=?2 AND a.type='vat' AND a.normal_balance='credit' AND l.credit_amount=0 AND l.debit_amount>0 AND l.vat_code IS NULL UNION ALL SELECT l.debit_amount FROM invoice_bad_debt_writeoffs w JOIN journal_lines l ON l.journal_entry_id=w.journal_entry_id JOIN accounts a ON a.id=l.account_id WHERE w.invoice_document_id=?1 AND w.writeoff_date<=?2 AND a.type='vat' AND a.normal_balance='credit' AND l.credit_amount=0 AND l.debit_amount>0 AND l.vat_code IS NULL",
        id,
        &input.write_off_date,
    )?;
    if original_output == Money::default() || original_output.is_negative() {
        return Err(refused("booked output VAT cannot be reconstructed"));
    }
    let vat = allocation(&original.gross, &original.vat, &cumulative, &prior_vat)?;
    let vat_dkk = allocation(
        &original.gross,
        &original_output,
        &cumulative,
        &prior_output,
    )?;
    let net = amount.subtract(&vat);
    let net_dkk = gross_dkk.subtract(&vat_dkk);
    if net.is_negative() || net_dkk.is_negative() {
        return Err(refused("VAT relief exceeds remaining gross carrying"));
    }
    let vat_account:String=tx.query_row("SELECT account_no FROM account_role_mappings WHERE role='output_vat' AND status='confirmed'",[],|r|r.get(0)).map_err(invalid)?;
    account(tx, &input.expense_account_no, "expense", "debit", true)?;
    account(tx, &debtor, "asset", "debit", false)?;
    account(tx, &vat_account, "vat", "credit", false)?;
    let rate = if currency == "DKK" {
        None
    } else {
        let rate = FxRate::from_settlement(&amount, &gross_dkk)?;
        if rate.apply(&amount) != gross_dkk {
            return Err(refused(
                "foreign writeoff carrying cannot be represented by a six-decimal journal rate",
            ));
        }
        Some(rate)
    };
    let mut lines = vec![JournalLine {
        account_no: AccountNo::parse(&input.expense_account_no)?,
        debit_amount: net_dkk.clone(),
        credit_amount: Money::default(),
        vat_code: Some("DK_BAD_DEBT_25".into()),
        text: Some("Established bad-debt loss basis".into()),
    }];
    if vat_dkk != Money::default() {
        lines.push(crate::reminder_claims::line(
            &vat_account,
            vat_dkk.clone(),
            Money::default(),
            "Output VAT relief",
        )?);
    }
    lines.push(crate::reminder_claims::line(
        &debtor,
        Money::default(),
        gross_dkk.clone(),
        "Write off original receivable",
    )?);
    let result = crate::journal_post::post(
        tx,
        &ManualJournal {
            transaction_date: date,
            text: format!("Bad debt writeoff {no}"),
            currency: Currency::parse(&currency)?,
            document_id: Some(DocumentId::from_sqlite(id)),
            source_bank_transaction_id: None,
            amount_foreign: (currency != "DKK").then_some(amount.clone()),
            amount_dkk: (currency != "DKK").then_some(gross_dkk.clone()),
            fx_rate_to_dkk: rate,
            created_by_program: None,
            lines,
        },
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
    let wid:i64=tx.query_row("INSERT INTO invoice_bad_debt_writeoffs(invoice_document_id,writeoff_date,gross_amount,net_amount,vat_amount,note,journal_entry_id)VALUES(?1,?2,?3,?4,?5,?6,?7)RETURNING id",params![id,input.write_off_date,amount.decimal(),net.decimal(),vat.decimal(),input.note,result["entryId"].as_i64()],|r|r.get(0)).map_err(invalid)?;
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor)VALUES('invoice_bad_debt_writeoff','invoice_bad_debt_writeoff',?1,?2,?3)",params![wid.to_string(),format!("Confirmed established loss on invoice {no}"),format!("user:{subject}")]).map_err(invalid)?;
    history(tx, id)?;
    let (remaining, _) = crate::reminder_claims::principal_open(tx, id)?;
    let remaining_dkk = crate::invoice_settlement::principal_carrying(tx, id, debtor_id)?;
    let claims = crate::reminder_claims::validated_claims(tx, id)?;
    let remaining_claim = claims.registered.subtract(&claims.paid);
    Ok(
        json!({"ok":true,"writeOffId":wid,"journalEntryId":result["entryId"],"journalEntryNo":result["entryNo"],"invoiceDocumentId":id,"invoiceNumber":no,"principalCurrency":currency,"claimCurrency":"DKK","grossAmount":wire(&amount)?,"netAmount":wire(&net)?,"vatAmount":wire(&vat)?,"grossAmountDkk":wire(&gross_dkk)?,"netAmountDkk":wire(&net_dkk)?,"vatAmountDkk":wire(&vat_dkk)?,"remainingPrincipal":wire(&remaining)?,"remainingPrincipalDkk":wire(&remaining_dkk)?,"remainingClaimDkk":wire(&remaining_claim)?}),
    )
}
#[cfg(test)]
mod tests;
