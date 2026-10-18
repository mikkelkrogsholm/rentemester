//! Customer → immutable issued invoice → explicit sales posting, on the admitted transaction.
use crate::{invalid, legacy_money};
use rentemester_engine::{
    Error,
    currency::Currency,
    fiscal_year::{self, Date, LabelStrategy, StartMonth},
    identifiers::{AccountNo, DocumentId},
    manual_journal::{JournalLine, ManualJournal},
    money::Money,
    sales_invoice::{self, Input, Party, refused, wire},
};
use rusqlite::{OptionalExtension, Transaction, params};
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Customer {
    pub name: String,
    pub address: Option<String>,
    pub vat_or_cvr: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub website: Option<String>,
    pub ean_number: Option<String>,
    pub payment_terms_days: Option<i64>,
    pub default_currency: Option<String>,
    pub notes: Option<String>,
}
pub fn create_customer(
    tx: &Transaction<'_>,
    input: &Customer,
    actor: &str,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    let name = sales_invoice::text(&input.name, 256)?;
    for text in [
        &input.address,
        &input.vat_or_cvr,
        &input.email,
        &input.phone,
        &input.website,
        &input.notes,
    ]
    .into_iter()
    .flatten()
    {
        if text.len() > 4096 || text.chars().any(char::is_control) {
            return Err(refused("invalid customer field"));
        }
    }
    if input
        .payment_terms_days
        .is_some_and(|n| !(1..=365).contains(&n))
        || input
            .default_currency
            .as_deref()
            .is_some_and(|s| s.len() != 3 || !s.bytes().all(|c| c.is_ascii_uppercase()))
    {
        return Err(refused("invalid customer payment terms or currency"));
    }
    if input.ean_number.as_deref().is_some_and(|s| !s.is_empty()) {
        return Err(refused("public-recipient customer setup is not ported yet"));
    }
    let id:i64=tx.query_row("INSERT INTO customers(name,address,vat_or_cvr,email,phone,website,payment_terms_days,default_currency,notes) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9) RETURNING id",params![name,input.address,input.vat_or_cvr,input.email,input.phone,input.website,input.payment_terms_days,input.default_currency.as_deref().unwrap_or("DKK"),input.notes],|r|r.get(0)).map_err(|_|refused("customer already exists or could not be stored"))?;
    audit(
        tx,
        "customer_add",
        id,
        &format!("Created customer {name}"),
        actor,
    )?;
    Ok(json!({"id":id}))
}
fn audit(
    tx: &Transaction<'_>,
    event: &str,
    id: i64,
    message: &str,
    actor: &str,
) -> Result<(), Error> {
    let entity = if event == "customer_add" {
        "customer"
    } else {
        "document"
    };
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES(?1,?2,?3,?4,?5)",params![event,entity,id.to_string(),message,format!("user:{actor}")]).map_err(invalid)?;
    Ok(())
}
fn context(
    tx: &Transaction<'_>,
    slug: &str,
    today: &str,
    year: Option<&str>,
) -> Result<Value, Error> {
    let snap = crate::cockpit::read(tx, slug, today, year, None)?;
    let mut base = snap.trial_balance;
    let obj = base.as_object_mut().unwrap();
    for field in ["rows", "totalDebit", "totalCredit", "balanced"] {
        obj.remove(field);
    }
    Ok(base)
}
pub fn contacts(tx: &Transaction<'_>, slug: &str, today: &str) -> Result<Value, Error> {
    let mut base = context(tx, slug, today, None)?;
    let mut q=tx.prepare("SELECT id,name,address,vat_or_cvr,email,phone,website,ean_number,payment_terms_days,default_currency,notes FROM customers WHERE archived=0 ORDER BY name,id").map_err(invalid)?;
    let mut rows = q.query([]).map_err(invalid)?;
    let mut customers = Vec::new();
    let mut invoice_query = tx.prepare("SELECT d.amount_inc_vat,d.payload_json,COALESCE((SELECT SUM(p.amount) FROM invoice_payments p WHERE p.invoice_document_id=d.id),0),d.id FROM documents d WHERE d.document_type='issued_invoice'").map_err(invalid)?;
    let mut invoice_rows = invoice_query.query([]).map_err(invalid)?;
    let mut invoices = Vec::new();
    while let Some(row) = invoice_rows.next().map_err(invalid)? {
        let payload: String = row.get(1).map_err(invalid)?;
        let payload: Value = serde_json::from_str(&payload).map_err(invalid)?;
        if payload["currency"] != "DKK" {
            return Err(refused(
                "foreign invoice receivable reports are not ported yet",
            ));
        }
        let open = legacy_money(row.get_ref(0).map_err(invalid)?)?
            .subtract(&legacy_money(row.get_ref(2).map_err(invalid)?)?)
            .subtract(&crate::credit_note::credited(
                tx,
                row.get(3).map_err(invalid)?,
            )?);
        let overdue = payload["dueDate"].as_str().is_some_and(|due| due < today);
        invoices.push((payload["customerId"].as_i64(), open, overdue));
    }
    while let Some(r) = rows.next().map_err(invalid)? {
        let id: i64 = r.get(0).map_err(invalid)?;
        let mut balance = Money::default();
        let mut count = 0;
        let mut overdue = 0;
        for (customer, open, is_overdue) in &invoices {
            if *customer == Some(id) && !open.is_negative() && *open != Money::default() {
                balance.add(open);
                count += 1;
                overdue += usize::from(*is_overdue);
            }
        }
        customers.push(json!({"id":id,"name":r.get::<_,String>(1).map_err(invalid)?,"address":r.get::<_,Option<String>>(2).map_err(invalid)?,"vatOrCvr":r.get::<_,Option<String>>(3).map_err(invalid)?,"email":r.get::<_,Option<String>>(4).map_err(invalid)?,"phone":r.get::<_,Option<String>>(5).map_err(invalid)?,"website":r.get::<_,Option<String>>(6).map_err(invalid)?,"eanNumber":r.get::<_,Option<String>>(7).map_err(invalid)?,"paymentTermsDays":r.get::<_,Option<i64>>(8).map_err(invalid)?.unwrap_or(14),"defaultCurrency":r.get::<_,String>(9).map_err(invalid)?,"notes":r.get::<_,Option<String>>(10).map_err(invalid)?,"openBalance":wire(&balance)?,"openInvoiceCount":count,"overdueCount":overdue}));
    }
    let mut q=tx.prepare("SELECT id,name,address,vat_or_cvr,country_code,identifier_kind,identity_status,email,phone,website,default_expense_account,default_vat_treatment,notes FROM vendors WHERE archived=0 ORDER BY name,id").map_err(invalid)?;
    let vendors=q.query_map([],|r|Ok(json!({"id":r.get::<_,i64>(0)?,"name":r.get::<_,String>(1)?,"address":r.get::<_,Option<String>>(2)?,"vatOrCvr":r.get::<_,Option<String>>(3)?,"countryCode":r.get::<_,Option<String>>(4)?,"identifierKind":r.get::<_,Option<String>>(5)?,"identityStatus":r.get::<_,String>(6)?,"email":r.get::<_,Option<String>>(7)?,"phone":r.get::<_,Option<String>>(8)?,"website":r.get::<_,Option<String>>(9)?,"defaultExpenseAccount":r.get::<_,Option<String>>(10)?,"defaultVatTreatment":r.get::<_,Option<String>>(11)?,"notes":r.get::<_,Option<String>>(12)?}))).map_err(invalid)?.collect::<Result<Vec<_>,_>>().map_err(invalid)?;
    base["customers"] = json!(customers);
    base["vendors"] = json!(vendors);
    Ok(base)
}
fn party(input: Option<&Party>, default: &Value) -> Result<Value, Error> {
    let field = |value: Option<&String>, key: &str| {
        sales_invoice::text(
            value
                .map(String::as_str)
                .filter(|s| !s.trim().is_empty())
                .or_else(|| default[key].as_str())
                .unwrap_or(""),
            1024,
        )
    };
    let vat = field(input.and_then(|p| p.vat_or_cvr.as_ref()), "vatOrCvr")?;
    let digits = vat.strip_prefix("DK").unwrap_or(&vat);
    if digits.len() != 8 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(refused(
            "native full domestic invoice requires Danish seller and buyer VAT/CVR identifiers",
        ));
    }
    Ok(
        json!({"name":field(input.and_then(|p|p.name.as_ref()),"name")?,"address":field(input.and_then(|p|p.address.as_ref()),"address")?,"vatOrCvr":vat}),
    )
}
pub fn prepare(
    tx: &Transaction<'_>,
    input: &Input,
    now: &str,
    allocate: bool,
) -> Result<Value, Error> {
    let computed = sales_invoice::compute(input)?;
    let company = crate::company_setup::profile(tx)?;
    if company["currency"] != "DKK" || company["vatPeriodType"].is_null() {
        return Err(refused(
            "standard domestic invoice requires a DKK company registered for VAT",
        ));
    }
    let seller = party(
        input.seller.as_ref(),
        &json!({"name":company["name"],"address":company["address"],"vatOrCvr":company["cvr"]}),
    )?;
    let mut terms = company["paymentTermsDays"].as_i64().unwrap_or(14);
    let customer = if let Some(id) = input.customer_id {
        if id <= 0 {
            return Err(refused("invalid customer id"));
        }
        let customer:Value=tx.query_row("SELECT name,address,vat_or_cvr,payment_terms_days FROM customers WHERE id=?1 AND archived=0",[id],|r|Ok(json!({"name":r.get::<_,String>(0)?,"address":r.get::<_,Option<String>>(1)?,"vatOrCvr":r.get::<_,Option<String>>(2)?,"terms":r.get::<_,Option<i64>>(3)?}))).optional().map_err(invalid)?.ok_or_else(||refused("active customer required"))?;
        if let Some(days) = customer["terms"].as_i64() {
            terms = days;
        }
        customer
    } else {
        Value::Null
    };
    let buyer = party(input.buyer.as_ref(), &customer)?;
    let issue = rentemester_engine::vat_period::date(&input.issue_date)?;
    let due = match &input.due_date {
        Some(date) => date.clone(),
        None => issue
            .checked_add(time::Duration::days(terms))
            .ok_or_else(|| refused("due date exceeds supported calendar"))?
            .to_string(),
    };
    if rentemester_engine::vat_period::date(&due)? < issue {
        return Err(refused("due date precedes invoice date"));
    }
    let year = fiscal_year::for_date(
        Date::parse(&input.issue_date)?,
        StartMonth::new(company["fiscalYearStartMonth"].as_i64().unwrap() as i32)?,
        LabelStrategy::try_from(company["fiscalYearLabelStrategy"].as_str().unwrap())?,
    )?
    .display_label;
    let prefix = format!("{year}-");
    let mut floor = 0;
    let mut q = tx
        .prepare("SELECT invoice_no FROM documents WHERE document_type='issued_invoice'")
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
                    .map_err(|_| refused("invoice sequence exceeds range"))?,
            );
        }
    }
    let scope = crate::sequences::company_scope(company["cvr"].as_str(), 1, &year);
    let next = crate::sequences::current(tx, "issued_invoice", &scope, floor)?
        .checked_add(1)
        .filter(|n| *n <= 9_007_199_254_740_991)
        .ok_or_else(|| refused("invoice sequence exhausted"))?;
    if let Some(no) = &input.invoice_number {
        let suffix = no
            .strip_prefix(&prefix)
            .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
            .ok_or_else(|| refused("invoice number must match fiscal scope and next sequence"))?;
        if suffix.parse::<i64>().ok() != Some(next) {
            return Err(refused(
                "invoice number must be the next consecutive number",
            ));
        }
    }
    if allocate {
        crate::journal_post::require_current(tx)?;
        crate::sequences::next(tx, "issued_invoice", &scope, floor)?;
    }
    Ok(
        json!({"invoiceType":"full","vatTreatment":"standard","issueDate":input.issue_date,"dueDate":due,"invoiceNumber":format!("{year}-{next:04}"),"status":"issued","issuedAt":now,"currency":"DKK","seller":seller,"buyer":buyer,"customerId":input.customer_id,"lines":computed["lines"],"totals":computed["totals"],"payment":company["payment"]}),
    )
}
pub struct Artifact<'a> {
    pub filename: &'a str,
    pub path: &'a str,
    pub hash: &'a str,
    pub mime: &'a str,
}
pub fn register(
    tx: &Transaction<'_>,
    payload: &Value,
    json_file: Artifact<'_>,
    pdf_file: Artifact<'_>,
    actor: &str,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    let serialized = serde_json::to_string_pretty(payload).map_err(invalid)?;
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
    let mut id = 0;
    for (kind, file) in [
        ("issued_invoice", json_file),
        ("issued_invoice_pdf", pdf_file),
    ] {
        let no = if kind == "issued_invoice" {
            payload["invoiceNumber"].as_str().unwrap().to_owned()
        } else {
            format!("{}-PDF", payload["invoiceNumber"].as_str().unwrap())
        };
        let inserted:i64=tx.query_row("INSERT INTO documents(document_no,source,original_filename,stored_path,mime_type,sha256_hash,supplier_name,invoice_no,invoice_date,amount_inc_vat,currency,status,document_type,delivery_description,sender_name,sender_address,sender_vat_cvr,recipient_name,recipient_address,recipient_vat_cvr,vat_amount,payload_json,retain_until) VALUES(?1,'rentemester',?2,?3,?4,?5,?6,?7,?8,?9,'DKK','issued',?10,?11,?6,?12,?13,?14,?15,?16,?17,?18,?19) RETURNING id",params![no,file.filename,file.path,file.mime,file.hash,payload["seller"]["name"].as_str(),payload["invoiceNumber"].as_str(),payload["issueDate"].as_str(),payload["totals"]["grossAmount"].as_f64(),kind,payload["lines"].to_string(),payload["seller"]["address"].as_str(),payload["seller"]["vatOrCvr"].as_str(),payload["buyer"]["name"].as_str(),payload["buyer"]["address"].as_str(),payload["buyer"]["vatOrCvr"].as_str(),payload["totals"]["vatAmount"].as_f64(),serialized,retention],|r|r.get(0)).map_err(invalid)?;
        if kind == "issued_invoice" {
            id = inserted;
        }
        audit(
            tx,
            if kind == "issued_invoice" {
                "invoice_issue"
            } else {
                "invoice_render_pdf"
            },
            inserted,
            "Issued immutable sales invoice evidence",
            actor,
        )?;
    }
    Ok(
        json!({"documentId":id,"invoiceNumber":payload["invoiceNumber"],"netAmount":payload["totals"]["netAmount"],"vatRate":payload["totals"]["vatRate"],"vatAmount":payload["totals"]["vatAmount"],"grossAmount":payload["totals"]["grossAmount"],"lines":payload["lines"]}),
    )
}
pub fn pdf_id(tx: &Transaction<'_>, id: i64) -> Result<i64, Error> {
    tx.query_row("SELECT pdf.id FROM documents d JOIN documents pdf ON pdf.document_type='issued_invoice_pdf' AND pdf.invoice_no=d.invoice_no AND pdf.payload_json=d.payload_json WHERE d.id=?1 AND d.document_type='issued_invoice'",[id],|r|r.get(0)).optional().map_err(invalid)?.ok_or_else(||refused("immutable invoice PDF unavailable"))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Post {
    pub invoice_document_id: i64,
    pub revenue_account_no: String,
    pub confirm: bool,
}
fn role(tx: &Transaction<'_>, name: &str, kind: &str, normal: &str) -> Result<String, Error> {
    tx.query_row("SELECT a.account_no FROM account_role_mappings m JOIN accounts a ON a.account_no=m.account_no WHERE m.role=?1 AND m.status='confirmed' AND a.active=1 AND a.type=?2 AND a.normal_balance=?3",[name,kind,normal],|r|r.get(0)).optional().map_err(invalid)?.ok_or_else(||refused(format!("confirmed account role {name} required")))
}
pub fn post(
    tx: &Transaction<'_>,
    input: &Post,
    subject: &str,
    latest: Date,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    if !input.confirm || input.invoice_document_id <= 0 {
        return Err(refused(
            "invoice and explicit posting confirmation required",
        ));
    }
    let payload: String = tx
        .query_row(
            "SELECT payload_json FROM documents WHERE id=?1 AND document_type='issued_invoice'",
            [input.invoice_document_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(invalid)?
        .ok_or_else(|| refused("issued invoice required"))?;
    let payload: Value = serde_json::from_str(&payload).map_err(invalid)?;
    if payload["currency"] != "DKK"
        || payload["vatTreatment"] != "standard"
        || payload["totals"]["vatRate"].as_f64() != Some(0.25)
    {
        return Err(refused(
            "native invoice posting requires standard domestic DKK invoice",
        ));
    }
    let existing: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM journal_entries WHERE document_id=?1)",
            [input.invoice_document_id],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    if existing {
        return Err(refused(
            "invoice already has journal entry; cannot be booked twice",
        ));
    }
    let revenue = &input.revenue_account_no;
    let compatible:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM accounts WHERE account_no=?1 AND type='income' AND normal_balance='credit' AND active=1)",[revenue],|r|r.get(0)).map_err(invalid)?;
    if !compatible {
        return Err(refused(
            "active income account with credit normal balance required",
        ));
    }
    let debtors = role(tx, "debtors", "asset", "debit")?;
    let output = role(tx, "output_vat", "vat", "credit")?;
    let money = |key: &str| Money::parse(&payload["totals"][key].to_string());
    let net = money("netAmount")?;
    let vat = money("vatAmount")?;
    let gross = money("grossAmount")?;
    let mut total = net.clone();
    total.add(&vat);
    if total != gross || net.percent("25")? != vat {
        return Err(refused("invoice amounts do not reconcile"));
    }
    let line = |account: &str,
                debit: Money,
                credit: Money,
                code: Option<&str>|
     -> Result<JournalLine, Error> {
        Ok(JournalLine {
            account_no: AccountNo::parse(account)?,
            debit_amount: debit,
            credit_amount: credit,
            vat_code: code.map(str::to_owned),
            text: Some(format!(
                "Faktura {}",
                payload["invoiceNumber"].as_str().unwrap_or("")
            )),
        })
    };
    let request = ManualJournal {
        transaction_date: Date::parse(
            payload["issueDate"]
                .as_str()
                .ok_or_else(|| refused("missing invoice date"))?,
        )?,
        text: format!("Salg {}", payload["invoiceNumber"].as_str().unwrap_or("")),
        currency: Currency::parse("DKK")?,
        document_id: Some(DocumentId::from_sqlite(input.invoice_document_id)),
        source_bank_transaction_id: None,
        amount_foreign: None,
        amount_dkk: None,
        fx_rate_to_dkk: None,
        created_by_program: None,
        lines: vec![
            line(&debtors, gross.clone(), Money::default(), None)?,
            line(revenue, Money::default(), net, Some("DK_SALE_25"))?,
            line(&output, Money::default(), vat, None)?,
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
    let id: i64 = tx
        .query_row(
            "SELECT id FROM accounts WHERE account_no=?1",
            [debtors],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    tx.execute("INSERT INTO issued_invoice_postings(invoice_document_id,journal_entry_id,receivable_account_id,booked_gross_dkk) VALUES(?1,?2,?3,?4)",params![input.invoice_document_id,result["entryId"].as_i64(),id,gross.decimal().parse::<f64>().map_err(invalid)?]).map_err(invalid)?;
    audit(
        tx,
        "invoice_post",
        input.invoice_document_id,
        "Booked issued invoice with explicit receivable identity",
        subject,
    )?;
    Ok(result)
}
pub fn list(
    tx: &Transaction<'_>,
    slug: &str,
    today: &str,
    year: Option<&str>,
) -> Result<Value, Error> {
    let mut base = context(tx, slug, today, year)?;
    let mut rows = Vec::new();
    let mut total = Money::default();
    let mut open_total = Money::default();
    let mut overdue_count = 0;
    let mut q=tx.prepare("SELECT d.id,d.invoice_no,d.invoice_date,d.recipient_name,d.currency,d.amount_inc_vat,d.payload_json,p.journal_entry_id FROM documents d LEFT JOIN issued_invoice_postings p ON p.invoice_document_id=d.id WHERE d.document_type='issued_invoice' AND d.invoice_date>=?1 AND d.invoice_date<=?2 ORDER BY d.invoice_date DESC,d.id DESC").map_err(invalid)?;
    let mut result = q
        .query(params![
            base["periodStart"].as_str(),
            base["periodEnd"].as_str()
        ])
        .map_err(invalid)?;
    while let Some(r) = result.next().map_err(invalid)? {
        let id: i64 = r.get(0).map_err(invalid)?;
        let payload: String = r.get(6).map_err(invalid)?;
        let payload: Value = serde_json::from_str(&payload).map_err(invalid)?;
        let gross = legacy_money(r.get_ref(5).map_err(invalid)?)?;
        let paid = tx
            .query_row(
                "SELECT COALESCE(SUM(amount),0) FROM invoice_payments WHERE invoice_document_id=?1",
                [id],
                |r| legacy_money(r.get_ref(0)?).map_err(|_| rusqlite::Error::InvalidQuery),
            )
            .map_err(invalid)?;
        let credited = crate::credit_note::credited(tx, id)?;
        let creditable = gross.subtract(&credited);
        let open = creditable.subtract(&paid);
        total.add(&gross);
        open_total.add(&open);
        let due = payload["dueDate"].as_str();
        let overdue = due
            .map(|d| {
                rentemester_engine::vat_period::date(today).and_then(|t| {
                    Ok((t - rentemester_engine::vat_period::date(d)?)
                        .whole_days()
                        .max(0))
                })
            })
            .transpose()?
            .unwrap_or(0);
        if overdue > 0 && open != Money::default() {
            overdue_count += 1;
        }
        rows.push(json!({"documentId":id,"invoiceNo":r.get::<_,String>(1).map_err(invalid)?,"invoiceDate":r.get::<_,String>(2).map_err(invalid)?,"customerName":r.get::<_,Option<String>>(3).map_err(invalid)?,"customerEmail":null,"customerId":payload["customerId"],"buyerEanNumber":null,"buyerPublicRecipient":false,"peppolStatus":null,"lastEmailedAt":null,"lastReminderAt":null,"lastReminderSequence":0,"grossAmount":wire(&gross)?,"openBalance":wire(&open)?,"currency":r.get::<_,String>(4).map_err(invalid)?,"creditableBalance":wire(&creditable)?,"status":if open.is_negative(){"overpaid"}else if open==Money::default(){if credited==gross && paid==Money::default(){"credited"}else{"paid"}}else if overdue>0{"overdue"}else{"open"},"effectiveDueDate":due,"overdueDays":overdue,"posted":r.get::<_,Option<i64>>(7).map_err(invalid)?.is_some()}));
    }
    base["invoices"] = json!(rows);
    base["totalGross"] = wire(&total)?;
    base["totalOpen"] = wire(&open_total)?;
    base["overdueCount"] = json!(overdue_count);
    let mut q=tx.prepare("SELECT account_no,name FROM accounts WHERE active=1 AND type='income' AND normal_balance='credit' ORDER BY account_no").map_err(invalid)?;
    let accounts = q
        .query_map([], |r| {
            Ok(json!({"accountNo":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?}))
        })
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    base["nativeSales"] =
        json!({"revenueAccounts":accounts,"settlementAvailable":true,"creditAvailable":true});
    Ok(base)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn issue_number_preview_and_explicit_post_keep_the_ledger_balanced() {
        let mut control = rusqlite::Connection::open_in_memory().unwrap();
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic Seller").unwrap();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        tx.execute_batch("UPDATE companies SET address='Synthetic Street 1',cvr='87654321',vat_period_type='quarter' WHERE id=1; INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('R','Synthetic receivable','asset','debit'),('S','Synthetic sale','income','credit'),('V','Synthetic output VAT','vat','credit');INSERT INTO account_role_mappings(role,account_no,version,confirmed_by) VALUES('debtors','R',1,'synthetic'),('output_vat','V',1,'synthetic');").unwrap();
        let customer:Customer=serde_json::from_value(json!({"name":"Synthetic Buyer","address":"Synthetic Street 2","vatOrCvr":"DK12345678","paymentTermsDays":30})).unwrap();
        let customer = create_customer(&tx, &customer, "synthetic").unwrap();
        let input:Input=serde_json::from_value(json!({"issueDate":"2026-10-18","customerId":customer["id"],"lines":[{"description":"Synthetic service","quantity":2,"unitPriceExVat":500}]})).unwrap();
        let preview = prepare(&tx, &input, "2026-10-18T12:00:00Z", false).unwrap();
        assert_eq!(preview["invoiceNumber"], "2026-0001");
        assert_eq!(preview["dueDate"], "2026-11-17");
        let payload = prepare(&tx, &input, "2026-10-18T12:00:00Z", true).unwrap();
        assert_eq!(payload["invoiceNumber"], preview["invoiceNumber"]);
        let result = register(
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
        assert_eq!(result["grossAmount"].as_f64(), Some(1250.0));
        let id = result["documentId"].as_i64().unwrap();
        assert!(pdf_id(&tx, id).is_ok());
        assert_eq!(
            prepare(&tx, &input, "2026-10-18T12:00:00Z", false).unwrap()["invoiceNumber"],
            "2026-0002"
        );
        let posting = Post {
            invoice_document_id: id,
            revenue_account_no: "S".into(),
            confirm: true,
        };
        let result = post(
            &tx,
            &posting,
            "synthetic",
            Date::parse("2026-10-18").unwrap(),
        )
        .unwrap();
        assert_eq!(result["ok"], true);
        assert!(
            post(
                &tx,
                &posting,
                "synthetic",
                Date::parse("2026-10-18").unwrap()
            )
            .is_err()
        );
        let report = crate::cockpit::read(&tx, "synthetic", "2026-10-18", None, None).unwrap();
        assert_eq!(
            report.income_statement["totalIncome"].as_f64(),
            Some(1000.0)
        );
        assert_eq!(report.trial_balance["balanced"], true);
        let vat = crate::vat_report::read(&tx, "synthetic", "2026-10-18", None).unwrap();
        assert_eq!(vat["outputVat"].as_f64(), Some(250.0));
        assert_eq!(vat["vatReportErrors"], json!([]));
        assert_eq!(
            contacts(&tx, "synthetic", "2026-10-18").unwrap()["customers"][0]["openBalance"]
                .as_f64(),
            Some(1250.0)
        );
        // A later role change must not strand this invoice's original receivable.
        tx.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('A','Synthetic bank','asset','debit'),('R2','Synthetic new receivables','asset','debit');UPDATE account_role_mappings SET status='superseded' WHERE role='debtors';INSERT INTO account_role_mappings(role,account_no,version,confirmed_by) VALUES('debtors','R2',2,'synthetic');INSERT INTO bank_accounts(slug,name,ledger_account_no) VALUES('synthetic','Synthetic bank','A');").unwrap();
        let csv = "date;text;amount;currency;reference\n2026-10-18;Synthetic part one;500;DKK;part-one\n2026-10-18;Synthetic part two;750;DKK;part-two\n2026-10-18;Synthetic excess;2000;DKK;too-large\n2026-10-18;Synthetic ambiguous A;1;DKK;ambiguous\n2026-10-18;Synthetic ambiguous B;2;DKK;ambiguous\n";
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
        let payment = |reference: &str, amount: Option<i32>| {
            serde_json::from_value::<crate::invoice_settlement::Input>(json!({"invoiceDocumentId":id,"bankTransactionReference":reference,"amount":amount,"confirm":true})).unwrap()
        };
        let latest = Date::parse("2026-10-18").unwrap();
        assert!(
            crate::invoice_settlement::settle(
                &tx,
                &payment("too-large", None),
                "synthetic",
                latest
            )
            .is_err()
        );
        assert!(
            crate::invoice_settlement::settle(
                &tx,
                &payment("ambiguous", None),
                "synthetic",
                latest
            )
            .is_err()
        );
        assert!(
            crate::invoice_settlement::settle(
                &tx,
                &payment("part-one", Some(499)),
                "synthetic",
                latest
            )
            .is_err()
        );
        assert_eq!(
            crate::invoice_settlement::settle(&tx, &payment("part-one", None), "synthetic", latest)
                .unwrap()["openBalance"]
                .as_f64(),
            Some(750.0)
        );
        assert!(
            crate::invoice_settlement::settle(&tx, &payment("part-one", None), "synthetic", latest)
                .is_err()
        );
        assert_eq!(
            crate::invoice_settlement::settle(&tx, &payment("part-two", None), "synthetic", latest)
                .unwrap()["openBalance"]
                .as_f64(),
            Some(0.0)
        );
        let report = crate::cockpit::read(&tx, "synthetic", "2026-10-18", None, None).unwrap();
        let debtor = report.trial_balance["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["accountNo"] == "R")
            .unwrap();
        assert_eq!(debtor["balance"].as_f64(), Some(0.0));
        assert_eq!(
            report.income_statement["totalIncome"].as_f64(),
            Some(1000.0)
        );
        assert_eq!(
            crate::vat_report::read(&tx, "synthetic", "2026-10-18", None).unwrap()["outputVat"]
                .as_f64(),
            Some(250.0)
        );
        assert_eq!(
            contacts(&tx, "synthetic", "2026-10-18").unwrap()["customers"][0]["openBalance"]
                .as_f64(),
            Some(0.0)
        );
        assert_eq!(
            tx.query_row::<i64, _, _>("SELECT count(*) FROM journal_entries", [], |r| r.get(0))
                .unwrap(),
            3
        );
    }
}
