//! Immutable credit note evidence and matching original-account journal effects.
mod explicit;
use crate::{invalid, legacy_money, sales::Artifact};
use rentemester_engine::{
    Error,
    credit_note::{self, Counter},
    currency::Currency,
    fiscal_year::{self, Date, LabelStrategy, StartMonth},
    identifiers::{AccountNo, DocumentId},
    manual_journal::{JournalLine, ManualJournal},
    money::Money,
    sales_invoice::{refused, text, wire},
};
use rusqlite::{OptionalExtension, Transaction, params};
use serde::Deserialize;
use serde_json::{Number, Value, json};
use std::collections::BTreeMap;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Input {
    pub invoice_document_id: i64,
    pub issue_date: String,
    pub reason: String,
    pub gross_amount: Option<Number>,
    pub credit_note_number: Option<String>,
    pub confirm: bool,
}
pub struct Prepared {
    pub payload: Value,
    pub lines: Vec<JournalLine>,
    pub original_id: i64,
    pub debtor_id: i64,
    pub gross: Money,
}
/// Read only active linked evidence; never infer a credit from an arbitrary journal shape.
pub fn credited(tx: &Transaction<'_>, id: i64) -> Result<Money, Error> {
    let unresolved:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM documents c JOIN documents d ON d.invoice_no=c.payment_details WHERE d.id=?1 AND c.document_type='credit_note' AND NOT EXISTS(SELECT 1 FROM credit_note_postings p WHERE p.credit_note_document_id=c.id AND p.original_invoice_document_id=d.id))",[id],|r|r.get(0)).map_err(invalid)?;
    if unresolved {
        return Err(refused("unlinked credit note requires evidence resolution"));
    }
    let mut total = Money::default();
    let mut q=tx.prepare("SELECT p.booked_gross_dkk,c.amount_inc_vat,j.status,EXISTS(SELECT 1 FROM journal_entries reversal WHERE reversal.reversal_of_entry_id=j.id),c.currency,COALESCE(c.payload_json,'null'),j.document_id,c.id,(SELECT currency FROM documents WHERE id=p.original_invoice_document_id) FROM credit_note_postings p JOIN documents c ON c.id=p.credit_note_document_id JOIN journal_entries j ON j.id=p.journal_entry_id WHERE p.original_invoice_document_id=?1").map_err(invalid)?;
    let mut rows = q.query([id]).map_err(invalid)?;
    while let Some(r) = rows.next().map_err(invalid)? {
        let value = legacy_money(r.get_ref(0).map_err(invalid)?)?;
        if r.get::<_, String>(2).map_err(invalid)? != "posted"
            || r.get::<_, bool>(3).map_err(invalid)?
            || r.get::<_, Option<i64>>(6).map_err(invalid)?
                != Some(r.get::<_, i64>(7).map_err(invalid)?)
            || r.get::<_, String>(4).map_err(invalid)? != r.get::<_, String>(8).map_err(invalid)?
            || value
                != crate::invoice_fx::credit_dkk(
                    &serde_json::from_str::<Value>(&r.get::<_, String>(5).map_err(invalid)?)
                        .map_err(invalid)?,
                    &r.get::<_, String>(4).map_err(invalid)?,
                    &legacy_money(r.get_ref(1).map_err(invalid)?)?,
                )?
        {
            return Err(refused("credit note evidence requires resolution"));
        }
        total.add(&legacy_money(r.get_ref(1).map_err(invalid)?)?);
    }
    Ok(total)
}
/// Read-only reuse of the credit writer's immutable original and prior-counter proof.
pub(crate) fn verify_existing(tx: &Transaction<'_>, id: i64) -> Result<(), Error> {
    let (journal,debtor_id,debtor,gross,payload):(i64,i64,String,Money,String)=tx.query_row("SELECT p.journal_entry_id,p.receivable_account_id,a.account_no,p.booked_gross_dkk,d.payload_json FROM documents d JOIN issued_invoice_postings p ON p.invoice_document_id=d.id JOIN journal_entries j ON j.id=p.journal_entry_id JOIN accounts a ON a.id=p.receivable_account_id WHERE d.id=?1 AND d.document_type='issued_invoice' AND j.status='posted' AND j.document_id=d.id AND j.source_bank_transaction_id IS NULL AND j.reversal_of_entry_id IS NULL AND NOT EXISTS(SELECT 1 FROM journal_entries r WHERE r.reversal_of_entry_id=j.id)",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,legacy_money(r.get_ref(3)?).map_err(|_|rusqlite::Error::InvalidQuery)?,r.get(4)?))).map_err(invalid)?;
    let payload: Value = serde_json::from_str(&payload).map_err(invalid)?;
    let projection = crate::invoice_fx::projection(&payload)?;
    if projection.gross != gross {
        return Err(refused(
            "original snapshot gross differs from posting identity",
        ));
    }
    let prior = crate::invoice_fx::credited_dkk(tx, id)?;
    // The new advisory reader must identify each original credit document;
    // this does not change the writer's allocation or mutation gates.
    let mut query=tx.prepare("SELECT c.invoice_no,c.payment_details,c.invoice_date,c.currency,c.payload_json,c.sha256_hash FROM credit_note_postings p JOIN documents c ON c.id=p.credit_note_document_id WHERE p.original_invoice_document_id=?1").map_err(invalid)?;
    let mut rows = query.query([id]).map_err(invalid)?;
    while let Some(row) = rows.next().map_err(invalid)? {
        let (number, original_no, date, currency, raw, hash): (
            String,
            String,
            String,
            String,
            String,
            String,
        ) = (
            row.get(0).map_err(invalid)?,
            row.get(1).map_err(invalid)?,
            row.get(2).map_err(invalid)?,
            row.get(3).map_err(invalid)?,
            row.get(4).map_err(invalid)?,
            row.get(5).map_err(invalid)?,
        );
        let credit: Value = serde_json::from_str(&raw).map_err(invalid)?;
        if credit["type"] != "credit_note"
            || credit["creditNoteNumber"] != number
            || credit["originalInvoiceDocumentId"] != id
            || credit["originalInvoiceNumber"] != payload["invoiceNumber"]
            || json!(original_no) != payload["invoiceNumber"]
            || credit["issueDate"] != date
            || credit["currency"] != currency
            || payload["currency"].as_str().unwrap_or("DKK") != currency
            || crate::invoice_email::sha(raw.as_bytes()) != hash
        {
            return Err(refused(
                "immutable prior credit identity differs from its original invoice",
            ));
        }
    }
    if projection.legacy {
        let (_, counters) = legacy_counters(tx, id, journal, debtor_id, &debtor, &gross)?;
        let mut original_sum = Money::default();
        let mut prior_sum = Money::default();
        for counter in counters {
            if counter.original.is_negative()
                || counter.prior.is_negative()
                || counter.original.subtract(&counter.prior).is_negative()
            {
                return Err(refused("invalid original or prior credit allocation"));
            }
            original_sum.add(&counter.original);
            prior_sum.add(&counter.prior);
        }
        if original_sum != gross || prior_sum != prior {
            return Err(refused(
                "credit allocations do not reconcile to their document totals",
            ));
        }
        Ok(())
    } else {
        explicit::verify_existing(tx, id, journal, debtor_id, &debtor, &projection, &prior)
    }
}
pub fn prepare(tx: &Transaction<'_>, input: &Input, now: &str) -> Result<Prepared, Error> {
    crate::journal_post::require_current(tx)?;
    Date::parse(&input.issue_date)?;
    if !input.confirm || input.invoice_document_id <= 0 {
        return Err(refused("invoice and explicit credit confirmation required"));
    }
    let reason = text(&input.reason, 2000)?;
    let id = input.invoice_document_id;
    let row:Option<(String,String,String,i64,i64,String,Money)>=tx.query_row("SELECT d.invoice_no,d.invoice_date,d.payload_json,p.journal_entry_id,p.receivable_account_id,a.account_no,p.booked_gross_dkk FROM documents d JOIN issued_invoice_postings p ON p.invoice_document_id=d.id JOIN journal_entries j ON j.id=p.journal_entry_id JOIN accounts a ON a.id=p.receivable_account_id WHERE d.id=?1 AND d.document_type='issued_invoice' AND j.status='posted' AND j.document_id=d.id AND j.source_bank_transaction_id IS NULL AND NOT EXISTS(SELECT 1 FROM journal_entries reversal WHERE reversal.reversal_of_entry_id=j.id)",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,legacy_money(r.get_ref(6)?).map_err(|_|rusqlite::Error::InvalidQuery)?))).optional().map_err(invalid)?;
    let (original_no, date, payload, journal_id, debtor_id, debtor, gross) =
        row.ok_or_else(|| refused("active booked invoice required for credit note"))?;
    if input.issue_date < date {
        return Err(refused("credit note cannot precede the original invoice"));
    }
    let original: Value = serde_json::from_str(&payload).map_err(invalid)?;
    let projection = rentemester_engine::sales_invoice::project(&original)?;
    let dkk_projection = crate::invoice_fx::projection(&original)?;
    let currency = original["currency"]
        .as_str()
        .ok_or_else(|| refused("invoice currency required"))?;
    if dkk_projection.gross != gross {
        return Err(refused(
            "original snapshot gross differs from posting identity",
        ));
    }
    let prior_total = credited(tx, id)?;
    let remaining = projection.gross.subtract(&prior_total);
    if remaining.is_negative() || remaining == Money::default() {
        return Err(refused("invoice is already fully credited"));
    }
    let amount = input
        .gross_amount
        .as_ref()
        .map(|n| Money::parse(&n.to_string()))
        .transpose()?
        .unwrap_or(remaining);
    let prior_dkk = crate::invoice_fx::credited_dkk(tx, id)?;
    let amount_dkk = crate::invoice_fx::portion(&projection.gross, &gross, &prior_total, &amount)?;
    let (lines, vat_dkk, allocations) = if projection.legacy {
        legacy_lines(
            tx,
            id,
            journal_id,
            debtor_id,
            &debtor,
            &original_no,
            &gross,
            &prior_dkk,
            &amount_dkk,
        )?
    } else {
        explicit::lines(
            tx,
            id,
            journal_id,
            debtor_id,
            &debtor,
            &original_no,
            &dkk_projection,
            &prior_dkk,
            &amount_dkk,
        )?
    };
    let vat = if currency == "DKK" {
        vat_dkk.clone()
    } else if projection.vat == Money::default() {
        Money::default()
    } else {
        crate::invoice_fx::portion(&projection.gross, &projection.vat, &prior_total, &amount)?
    };
    let company = crate::company_setup::profile(tx)?;
    let year = fiscal_year::for_date(
        Date::parse(&input.issue_date)?,
        StartMonth::new(company["fiscalYearStartMonth"].as_i64().unwrap() as i32)?,
        LabelStrategy::try_from(company["fiscalYearLabelStrategy"].as_str().unwrap())?,
    )?
    .identifier_label;
    let prefix = format!("CN-{year}-");
    let mut floor = 0;
    let mut q = tx
        .prepare("SELECT invoice_no FROM documents WHERE document_type='credit_note'")
        .map_err(invalid)?;
    for value in q
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(invalid)?
    {
        let no = value.map_err(invalid)?;
        if let Some(s) = no
            .strip_prefix(&prefix)
            .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
        {
            floor = floor.max(
                s.parse::<i64>()
                    .map_err(|_| refused("credit number exceeds supported range"))?,
            );
        }
    }
    let scope = crate::sequences::company_scope(company["cvr"].as_str(), 1, &format!("CN-{year}"));
    let next = crate::sequences::current(tx, "credit_note", &scope, floor)?
        .checked_add(1)
        .filter(|n| *n <= 9_007_199_254_740_991)
        .ok_or_else(|| refused("credit note sequence exhausted"))?;
    if let Some(no) = &input.credit_note_number
        && no.strip_prefix(&prefix).and_then(|s| s.parse::<i64>().ok()) != Some(next)
    {
        return Err(refused(
            "credit note number must match the next consecutive number",
        ));
    }
    let number = format!("{prefix}{next:04}");
    crate::sequences::next(tx, "credit_note", &scope, floor)?;
    let mut cumulative = prior_total.clone();
    cumulative.add(&amount);
    let mut payload = json!({"type":"credit_note","creditNoteNumber":number,"originalInvoiceNumber":original_no,"originalInvoiceDocumentId":id,"issueDate":input.issue_date,"reason":reason,"currency":currency,"seller":original["seller"],"buyer":original["buyer"],"grossAmount":wire(&amount)?,"vatAmount":wire(&vat)?,"netAmount":wire(&amount.subtract(&vat))?,"creditedSoFar":wire(&prior_total)?,"remainingAfterThisCredit":wire(&projection.gross.subtract(&cumulative))?,"issuedAt":now});
    if currency != "DKK" {
        // The original market quote remains frozen. The journal's explicitly
        // derived carrying quote represents this cumulative cent allocation.
        let posting_rate = rentemester_engine::fx::FxRate::from_settlement(&amount, &amount_dkk)?;
        if posting_rate.apply(&amount) != amount_dkk {
            return Err(refused(
                "credit carrying valuation cannot be represented exactly",
            ));
        }
        payload["journalFxRateToDkk"] = json!(crate::journal_post::rate(&posting_rate)?);
        payload["fxValuationVersion"] = json!(1);
        payload["fxRateToDkk"] = original["totals"]["fxRateToDkk"].clone();
        payload["grossAmountDkk"] = wire(&amount_dkk)?;
        payload["vatAmountDkk"] = wire(&vat_dkk)?;
        payload["netAmountDkk"] = wire(&amount_dkk.subtract(&vat_dkk))?;
        payload["vatTreatment"] = original["vatTreatment"].clone();
        payload["reverseChargeBasis"] = original["reverseChargeBasis"].clone();
        payload["viesValidation"] = original["viesValidation"].clone();
    }
    if !projection.legacy {
        payload["creditAllocations"] = allocations;
    }
    Ok(Prepared {
        payload,
        lines,
        original_id: id,
        debtor_id,
        gross: amount_dkk,
    })
}
type LegacyCounters = BTreeMap<(String, Option<String>), (Money, bool)>;
fn legacy_counters(
    tx: &Transaction<'_>,
    id: i64,
    journal_id: i64,
    debtor_id: i64,
    debtor: &str,
    gross: &Money,
) -> Result<(LegacyCounters, Vec<Counter>), Error> {
    let mut original_lines = BTreeMap::<(String, Option<String>), (Money, bool)>::new();
    let mut receivable = Money::default();
    let mut q=tx.prepare("SELECT a.account_no,a.type,jl.debit_amount,jl.credit_amount,jl.vat_code FROM journal_lines jl JOIN accounts a ON a.id=jl.account_id WHERE jl.journal_entry_id=?1 ORDER BY jl.id").map_err(invalid)?;
    let mut rows = q.query([journal_id]).map_err(invalid)?;
    while let Some(r) = rows.next().map_err(invalid)? {
        let no: String = r.get(0).map_err(invalid)?;
        let kind: String = r.get(1).map_err(invalid)?;
        let debit = legacy_money(r.get_ref(2).map_err(invalid)?)?;
        let credit = legacy_money(r.get_ref(3).map_err(invalid)?)?;
        let code: Option<String> = r.get(4).map_err(invalid)?;
        if no == debtor {
            if credit != Money::default() {
                return Err(refused("invalid original receivable direction"));
            }
            receivable.add(&debit);
            continue;
        }
        if debit != Money::default()
            || credit == Money::default()
            || !((kind == "income" && code.as_deref() == Some("DK_SALE_25"))
                || (kind == "vat" && code.is_none()))
        {
            return Err(refused(
                "original journal cannot be credited as a standard domestic sale",
            ));
        }
        let entry = original_lines
            .entry((no, code))
            .or_insert((Money::default(), kind == "income"));
        entry.0.add(&credit);
    }
    if &receivable != gross {
        return Err(refused(
            "original invoice receivable does not match its posting identity",
        ));
    }
    let mut prior = BTreeMap::<(String, Option<String>), Money>::new();
    let mut q=tx.prepare("SELECT a.account_no,jl.vat_code,jl.debit_amount,jl.credit_amount FROM credit_note_postings p JOIN journal_lines jl ON jl.journal_entry_id=p.journal_entry_id JOIN accounts a ON a.id=jl.account_id WHERE p.original_invoice_document_id=?1 AND jl.account_id<>?2 ORDER BY jl.id").map_err(invalid)?;
    let mut rows = q.query(params![id, debtor_id]).map_err(invalid)?;
    while let Some(r) = rows.next().map_err(invalid)? {
        let key = (
            r.get::<_, String>(0).map_err(invalid)?,
            r.get::<_, Option<String>>(1).map_err(invalid)?,
        );
        if !original_lines.contains_key(&key) {
            return Err(refused(
                "prior credit uses accounts outside the original invoice",
            ));
        }
        prior.entry(key).or_default().add(
            &legacy_money(r.get_ref(2).map_err(invalid)?)?
                .subtract(&legacy_money(r.get_ref(3).map_err(invalid)?)?),
        );
    }
    let counters: Vec<_> = original_lines
        .iter()
        .map(|(key, (original, revenue))| Counter {
            original: original.clone(),
            prior: prior.get(key).cloned().unwrap_or_default(),
            revenue: *revenue,
        })
        .collect();
    Ok((original_lines, counters))
}
#[allow(clippy::too_many_arguments)]
fn legacy_lines(
    tx: &Transaction<'_>,
    id: i64,
    journal_id: i64,
    debtor_id: i64,
    debtor: &str,
    original_no: &str,
    gross: &Money,
    prior_total: &Money,
    amount: &Money,
) -> Result<(Vec<JournalLine>, Money, Value), Error> {
    let (original_lines, counters) = legacy_counters(tx, id, journal_id, debtor_id, debtor, gross)?;
    let allocations = credit_note::allocate(gross, prior_total, amount, &counters)?;
    let mut lines = vec![JournalLine {
        account_no: AccountNo::parse(debtor)?,
        debit_amount: Money::default(),
        credit_amount: amount.clone(),
        vat_code: None,
        text: Some(format!("Kreditering af {original_no}")),
    }];
    let mut vat = Money::default();
    for (((account, code), (_, revenue)), debit) in original_lines.iter().zip(allocations) {
        if debit == Money::default() {
            continue;
        }
        if !revenue {
            vat.add(&debit);
        }
        lines.push(JournalLine {
            account_no: AccountNo::parse(account)?,
            debit_amount: debit,
            credit_amount: Money::default(),
            vat_code: code.clone(),
            text: Some(format!("Kreditering af {original_no}")),
        });
    }
    Ok((lines, vat, Value::Null))
}

pub fn register_and_post(
    tx: &Transaction<'_>,
    prepared: Prepared,
    file: Artifact<'_>,
    subject: &str,
    latest: Date,
) -> Result<Value, Error> {
    let Prepared {
        payload,
        lines,
        original_id,
        debtor_id,
        gross,
    } = prepared;
    let serialized = serde_json::to_string_pretty(&payload).map_err(invalid)?;
    let month: i32 = tx
        .query_row(
            "SELECT fiscal_year_start_month FROM companies WHERE id=1",
            [],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    let retention = rentemester_engine::calendar::retention_deadline(
        payload["issueDate"].as_str().unwrap(),
        month as u32,
    )?;
    let id:i64=tx.query_row("INSERT INTO documents(document_no,source,original_filename,stored_path,mime_type,sha256_hash,supplier_name,invoice_no,invoice_date,amount_inc_vat,currency,status,document_type,delivery_description,sender_name,sender_address,sender_vat_cvr,recipient_name,recipient_address,recipient_vat_cvr,vat_amount,payment_details,payload_json,retain_until) VALUES(?1,'rentemester',?2,?3,?4,?5,?6,?1,?7,?8,?19,'issued','credit_note',?9,?6,?10,?11,?12,?13,?14,?15,?16,?17,?18) RETURNING id",params![payload["creditNoteNumber"].as_str(),file.filename,file.path,file.mime,file.hash,payload["seller"]["name"].as_str(),payload["issueDate"].as_str(),payload["grossAmount"].as_f64(),format!("Kreditnota for {}: {}",payload["originalInvoiceNumber"].as_str().unwrap(),payload["reason"].as_str().unwrap()),payload["seller"]["address"].as_str(),payload["seller"]["vatOrCvr"].as_str(),payload["buyer"]["name"].as_str(),payload["buyer"]["address"].as_str(),payload["buyer"]["vatOrCvr"].as_str(),payload["vatAmount"].as_f64(),payload["originalInvoiceNumber"].as_str(),serialized,retention,payload["currency"].as_str()],|r|r.get(0)).map_err(invalid)?;
    let foreign = payload["currency"] != "DKK";
    let source_amount = crate::invoice_fx::amount(&payload["grossAmount"])?;
    let posting_rate = if foreign {
        Some(rentemester_engine::sales_fx::rate(
            payload["currency"].as_str().unwrap(),
            payload["journalFxRateToDkk"].as_number(),
        )?)
    } else {
        None
    };
    let request = ManualJournal {
        transaction_date: Date::parse(payload["issueDate"].as_str().unwrap())?,
        text: format!(
            "Kreditnota {} for {}",
            payload["creditNoteNumber"].as_str().unwrap(),
            payload["originalInvoiceNumber"].as_str().unwrap()
        ),
        currency: Currency::parse(payload["currency"].as_str().unwrap_or("DKK"))?,
        document_id: Some(DocumentId::from_sqlite(id)),
        source_bank_transaction_id: None,
        amount_foreign: foreign.then_some(source_amount),
        amount_dkk: foreign.then_some(gross.clone()),
        fx_rate_to_dkk: posting_rate,
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
    tx.execute("INSERT INTO credit_note_postings(credit_note_document_id,original_invoice_document_id,journal_entry_id,receivable_account_id,booked_gross_dkk) VALUES(?1,?2,?3,?4,?5)",params![id,original_id,result["entryId"].as_i64(),debtor_id,wire(&gross)?.as_f64()]).map_err(invalid)?;
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES('credit_note_issue','document',?1,?2,?3)",params![id.to_string(),format!("Issued credit note {} for {}: {}",payload["creditNoteNumber"].as_str().unwrap(),payload["originalInvoiceNumber"].as_str().unwrap(),payload["reason"].as_str().unwrap()),format!("user:{subject}")]).map_err(invalid)?;
    Ok(
        json!({"documentId":id,"creditNoteNumber":payload["creditNoteNumber"],"originalInvoiceNumber":payload["originalInvoiceNumber"],"journalEntryId":result["entryId"],"journalEntryNo":result["entryNo"]}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn fixture() -> rusqlite::Connection {
        fixture_with(
            json!({"issueDate":"2026-10-18","buyer":{"name":"Synthetic Buyer","address":"Synthetic Street 2","vatOrCvr":"DK12345678"},"lines":[{"description":"Synthetic service","quantity":1,"unitPriceExVat":1000}]}),
            false,
        )
    }
    pub(super) fn fixture_with(mut source: Value, legacy: bool) -> rusqlite::Connection {
        source["issueDate"] = json!("2026-10-18");
        source["buyer"] = json!({"name":"Synthetic Buyer","address":"Synthetic Street 2","vatOrCvr":"DK12345678"});
        let mut control = rusqlite::Connection::open_in_memory().unwrap();
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic Seller").unwrap();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        tx.execute_batch("UPDATE companies SET address='Synthetic Street 1',cvr='87654321',vat_period_type='quarter' WHERE id=1; INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('R','Synthetic receivables','asset','debit'),('S','Synthetic sales','income','credit'),('V','Synthetic VAT','vat','credit'),('B','Synthetic bank','asset','debit'),('R2','Synthetic next receivables','asset','debit');INSERT INTO account_role_mappings(role,account_no,version,confirmed_by) VALUES('debtors','R',1,'synthetic'),('output_vat','V',1,'synthetic');INSERT INTO bank_accounts(slug,name,ledger_account_no) VALUES('synthetic','Synthetic bank','B');").unwrap();
        let input: rentemester_engine::sales_invoice::Input =
            serde_json::from_value(source).unwrap();
        let mut payload = crate::sales::prepare(&tx, &input, "2026-10-18T12:00:00Z", true).unwrap();
        if legacy {
            for line in payload["lines"].as_array_mut().unwrap() {
                let object = line.as_object_mut().unwrap();
                object.remove("taxClassification");
                object.remove("vatAmount");
                object.remove("vatRate");
            }
        }
        let invoice = crate::sales::register(
            &tx,
            &payload,
            Artifact {
                filename: "synthetic.json",
                path: "synthetic.json",
                hash: &"a".repeat(64),
                mime: "application/json",
            },
            Artifact {
                filename: "synthetic.pdf",
                path: "synthetic.pdf",
                hash: &"b".repeat(64),
                mime: "application/pdf",
            },
            "synthetic",
        )
        .unwrap();
        assert_eq!(invoice["documentId"], 1);
        crate::sales::post(
            &tx,
            &crate::sales::Post {
                invoice_document_id: 1,
                revenue_account_no: "S".into(),
                confirm: true,
            },
            "synthetic",
            Date::parse("2026-10-18").unwrap(),
        )
        .unwrap();
        tx.execute_batch("UPDATE account_role_mappings SET status='superseded' WHERE role='debtors';INSERT INTO account_role_mappings(role,account_no,version,confirmed_by) VALUES('debtors','R2',2,'synthetic');").unwrap();
        tx.commit().unwrap();
        db
    }
    fn issue(tx: &Transaction<'_>, gross: Option<i32>, hash: char) -> Result<Value, Error> {
        let input:Input=serde_json::from_value(json!({"invoiceDocumentId":1,"issueDate":"2026-10-18","reason":"Synthetic correction","grossAmount":gross,"confirm":true})).unwrap();
        let prepared = prepare(tx, &input, "2026-10-18T12:00:00Z")?;
        register_and_post(
            tx,
            prepared,
            Artifact {
                filename: "synthetic-credit.json",
                path: "synthetic-credit.json",
                hash: &hash.to_string().repeat(64),
                mime: "application/json",
            },
            "synthetic",
            Date::parse("2026-10-18")?,
        )
    }
    #[test]
    fn partial_then_final_credit_preserves_original_and_reduces_exact_original_accounts() {
        let mut db = fixture();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let original: String = tx
            .query_row("SELECT payload_json FROM documents WHERE id=1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(issue(&tx, Some(1251), 'c').is_err());
        assert_eq!(
            issue(&tx, Some(250), 'c').unwrap()["creditNoteNumber"],
            "CN-2026-0001"
        );
        let state = crate::sales::list(&tx, "synthetic", "2026-10-18", None).unwrap();
        assert_eq!(state["invoices"][0]["openBalance"].as_f64(), Some(1000.0));
        assert_eq!(
            issue(&tx, None, 'd').unwrap()["creditNoteNumber"],
            "CN-2026-0002"
        );
        assert!(issue(&tx, None, 'e').is_err());
        let state = crate::sales::list(&tx, "synthetic", "2026-10-18", None).unwrap();
        assert_eq!(state["invoices"][0]["status"], "credited");
        assert_eq!(state["invoices"][0]["openBalance"].as_f64(), Some(0.0));
        assert_eq!(
            tx.query_row::<String, _, _>(
                "SELECT payload_json FROM documents WHERE id=1",
                [],
                |r| r.get(0)
            )
            .unwrap(),
            original
        );
        let report = crate::cockpit::read(&tx, "synthetic", "2026-10-18", None, None).unwrap();
        assert_eq!(report.income_statement["totalIncome"].as_f64(), Some(0.0));
        assert_eq!(report.trial_balance["balanced"], true);
        let vat = crate::vat_report::read(&tx, "synthetic", "2026-10-18", None).unwrap();
        assert_eq!(vat["outputVat"].as_f64(), Some(0.0));
        assert_eq!(vat["vatReportErrors"], json!([]));
    }
    #[test]
    fn receipt_after_partial_credit_relieves_original_debtor_and_not_new_role() {
        let mut db = fixture();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        issue(&tx, Some(250), 'c').unwrap();
        let csv = "date;text;amount;currency;reference\n2026-10-18;Synthetic remaining payment;1000;DKK;synthetic-ref\n";
        let rows = rentemester_engine::bank::parse(csv).unwrap();
        crate::bank::import(
            &tx,
            csv,
            &rows,
            Some("synthetic"),
            "synthetic",
            "2026-10-18T12:00:00Z",
            "synthetic-original",
        )
        .unwrap();
        let input=serde_json::from_value(json!({"invoiceDocumentId":1,"bankTransactionReference":"synthetic-ref","confirm":true})).unwrap();
        let paid = crate::invoice_settlement::settle(
            &tx,
            &input,
            "synthetic",
            Date::parse("2026-10-18").unwrap(),
        )
        .unwrap();
        assert_eq!(paid["openBalance"].as_f64(), Some(0.0));
        let report = crate::cockpit::read(&tx, "synthetic", "2026-10-18", None, None).unwrap();
        assert_eq!(report.income_statement["totalIncome"].as_f64(), Some(800.0));
        let debtor = report.trial_balance["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["accountNo"] == "R")
            .unwrap();
        assert_eq!(debtor["balance"].as_f64(), Some(0.0));
        assert_eq!(
            crate::vat_report::read(&tx, "synthetic", "2026-10-18", None).unwrap()["outputVat"]
                .as_f64(),
            Some(200.0)
        );
    }
    #[test]
    fn paid_invoice_credit_and_partial_refunds_close_original_receivable_without_new_tax() {
        let mut db = fixture();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let csv = "date;text;amount;currency;reference\n2026-10-18;Synthetic receipt;1250;DKK;received\n2026-10-18;Synthetic refund one;-500;DKK;refund-one\n2026-10-18;Synthetic refund two;-750;DKK;refund-two\n2026-10-18;Synthetic excessive refund;-2000;DKK;too-much\n2026-10-18;Synthetic wrong direction;2;DKK;wrong-direction\n";
        let rows = rentemester_engine::bank::parse(csv).unwrap();
        crate::bank::import(
            &tx,
            csv,
            &rows,
            Some("synthetic"),
            "synthetic",
            "2026-10-18T12:00:00Z",
            "synthetic-original",
        )
        .unwrap();
        let receipt = serde_json::from_value(
            json!({"invoiceDocumentId":1,"bankTransactionReference":"received","confirm":true}),
        )
        .unwrap();
        let latest = Date::parse("2026-10-18").unwrap();
        crate::invoice_settlement::settle(&tx, &receipt, "synthetic", latest).unwrap();
        issue(&tx, Some(500), 'c').unwrap();
        issue(&tx, None, 'd').unwrap();
        let state = crate::sales::list(&tx, "synthetic", "2026-12-01", None).unwrap();
        assert_eq!(state["invoices"][0]["status"], "overpaid");
        assert_eq!(state["overdueCount"], 0);
        let refund = |reference: &str, amount: Option<i32>| {
            serde_json::from_value::<crate::invoice_settlement::RefundInput>(json!({"invoiceDocumentId":1,"bankTransactionReference":reference,"amount":amount,"confirm":true})).unwrap()
        };
        for request in [
            refund("wrong-direction", None),
            refund("too-much", None),
            refund("refund-one", Some(499)),
        ] {
            assert!(crate::invoice_settlement::refund(&tx, &request, "synthetic", latest).is_err());
        }
        assert_eq!(
            crate::invoice_settlement::refund(
                &tx,
                &refund("refund-one", None),
                "synthetic",
                latest
            )
            .unwrap()["remainingCreditBalance"]
                .as_f64(),
            Some(750.0)
        );
        assert!(
            crate::invoice_settlement::refund(
                &tx,
                &refund("refund-one", None),
                "synthetic",
                latest
            )
            .is_err()
        );
        assert_eq!(
            crate::invoice_settlement::refund(
                &tx,
                &refund("refund-two", None),
                "synthetic",
                latest
            )
            .unwrap()["remainingCreditBalance"]
                .as_f64(),
            Some(0.0)
        );
        let state = crate::sales::list(&tx, "synthetic", "2026-10-18", None).unwrap();
        assert_eq!(state["invoices"][0]["status"], "refunded");
        assert_eq!(state["totalOpen"].as_f64(), Some(0.0));
        let report = crate::cockpit::read(&tx, "synthetic", "2026-10-18", None, None).unwrap();
        assert_eq!(report.income_statement["totalIncome"].as_f64(), Some(0.0));
        assert_eq!(report.trial_balance["balanced"], true);
        let debtor = report.trial_balance["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["accountNo"] == "R")
            .unwrap();
        assert_eq!(debtor["balance"].as_f64(), Some(0.0));
        let vat = crate::vat_report::read(&tx, "synthetic", "2026-10-18", None).unwrap();
        assert_eq!(vat["outputVat"].as_f64(), Some(0.0));
        assert_eq!(vat["vatReportErrors"], json!([]));
        assert_eq!(
            tx.query_row::<i64, _, _>("SELECT count(*) FROM journal_entries", [], |r| r.get(0))
                .unwrap(),
            6
        );
    }
}

#[cfg(test)]
mod tests75;

#[cfg(test)]
mod tests77;
