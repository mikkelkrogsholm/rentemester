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
    create_customer_with_principal(tx, input, actor, crate::idempotency::PrincipalKind::User)
}
pub fn create_customer_with_principal(
    tx: &Transaction<'_>,
    input: &Customer,
    actor: &str,
    principal: crate::idempotency::PrincipalKind,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    validate_customer(input)?;
    let name = input.name.trim();
    crate::gdpr::require_contact_identity(
        tx,
        "customers",
        None,
        name,
        input.vat_or_cvr.as_deref(),
    )?;
    let id:i64=tx.query_row("INSERT INTO customers(name,address,vat_or_cvr,email,phone,website,ean_number,payment_terms_days,default_currency,notes) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10) RETURNING id",params![name,input.address,input.vat_or_cvr,input.email,input.phone,input.website,normalized_ean(input.ean_number.as_deref())?,input.payment_terms_days,input.default_currency.as_deref().unwrap_or("DKK"),input.notes],|r|r.get(0)).map_err(|e|if e.sqlite_error().is_some_and(|v|v.extended_code==rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE){refused("customer already exists")}else{Error::new("contact_storage_unavailable","contact could not be stored")})?;
    audit_with_principal(
        tx,
        "customer_add",
        id,
        &format!("Created customer {name}"),
        actor,
        principal,
    )?;
    Ok(json!({"id":id}))
}
fn normalized_ean(value: Option<&str>) -> Result<Option<String>, Error> {
    let Some(value) = value.filter(|s| !s.trim().is_empty()) else {
        return Ok(None);
    };
    let digits = value
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>();
    if digits.len() != 13 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(refused("eanNumber must be 13 digits"));
    }
    Ok(Some(digits))
}
fn validate_customer(input: &Customer) -> Result<(), Error> {
    sales_invoice::text(&input.name, 256)?;
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
        .is_some_and(|n| !(1..=9_007_199_254_740_991).contains(&n))
        || input
            .default_currency
            .as_deref()
            .is_some_and(|s| s.len() != 3 || !s.bytes().all(|c| c.is_ascii_uppercase()))
    {
        return Err(refused("invalid customer payment terms or currency"));
    }
    normalized_ean(input.ean_number.as_deref())?;
    Ok(())
}
/// Merge a strict partial patch; omitted fields survive, explicit null clears optional fields.
pub fn update_customer(
    tx: &Transaction<'_>,
    id: i64,
    patch: &Value,
    actor: &str,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    let fields = patch
        .as_object()
        .ok_or_else(|| refused("customer patch must be an object"))?;
    if id <= 0 || fields.is_empty() || fields.get("defaultCurrency").is_some_and(Value::is_null) {
        return Err(refused("invalid customer patch"));
    }
    let mut value: Value = tx.query_row("SELECT name,address,vat_or_cvr,email,phone,website,ean_number,payment_terms_days,default_currency,notes FROM customers WHERE id=?1 AND archived=0", [id], |r| Ok(json!({"name":r.get::<_,String>(0)?,"address":r.get::<_,Option<String>>(1)?,"vatOrCvr":r.get::<_,Option<String>>(2)?,"email":r.get::<_,Option<String>>(3)?,"phone":r.get::<_,Option<String>>(4)?,"website":r.get::<_,Option<String>>(5)?,"eanNumber":r.get::<_,Option<String>>(6)?,"paymentTermsDays":r.get::<_,Option<i64>>(7)?,"defaultCurrency":r.get::<_,String>(8)?,"notes":r.get::<_,Option<String>>(9)?}))).optional().map_err(invalid)?.ok_or_else(||refused("customer not found"))?;
    crate::gdpr::require_contact_identity(
        tx,
        "customers",
        Some(id),
        value["name"].as_str().unwrap(),
        value["vatOrCvr"].as_str(),
    )?;
    for (key, item) in fields {
        value
            .as_object_mut()
            .unwrap()
            .insert(key.clone(), item.clone());
    }
    let input: Customer =
        serde_json::from_value(value).map_err(|_| refused("invalid customer field"))?;
    validate_customer(&input)?;
    let name = input.name.trim();
    crate::gdpr::require_contact_identity(
        tx,
        "customers",
        Some(id),
        name,
        input.vat_or_cvr.as_deref(),
    )?;
    tx.execute("UPDATE customers SET name=?1,address=?2,vat_or_cvr=?3,email=?4,phone=?5,website=?6,ean_number=?7,payment_terms_days=?8,default_currency=?9,notes=?10 WHERE id=?11 AND archived=0", params![name,input.address,input.vat_or_cvr,input.email,input.phone,input.website,normalized_ean(input.ean_number.as_deref())?,input.payment_terms_days,input.default_currency,input.notes,id]).map_err(|_|refused("customer already exists or could not be stored"))?;
    audit(
        tx,
        "customer_update",
        id,
        &format!("Updated customer {name}"),
        actor,
    )?;
    Ok(json!({"id":id,"ok":true}))
}

fn audit(
    tx: &Transaction<'_>,
    event: &str,
    id: i64,
    message: &str,
    actor: &str,
) -> Result<(), Error> {
    audit_with_principal(
        tx,
        event,
        id,
        message,
        actor,
        crate::idempotency::PrincipalKind::User,
    )
}
fn audit_with_principal(
    tx: &Transaction<'_>,
    event: &str,
    id: i64,
    message: &str,
    actor: &str,
    principal: crate::idempotency::PrincipalKind,
) -> Result<(), Error> {
    let entity = if event.starts_with("customer_") {
        "customer"
    } else {
        "document"
    };
    let reference = if entity == "customer" {
        crate::gdpr::contact_audit_reference("customers", id)
    } else {
        id.to_string()
    };
    let message = if entity == "customer" {
        format!("{event} {reference}")
    } else {
        message.to_owned()
    };
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES(?1,?2,?3,?4,?5)",params![event,entity,reference,message,format!("{}:{actor}",match principal {crate::idempotency::PrincipalKind::User=>"user",crate::idempotency::PrincipalKind::ServiceAccount=>"service_account"})]).map_err(invalid)?;
    Ok(())
}
pub(crate) fn context(
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
    let mut invoice_query = tx.prepare("SELECT d.amount_inc_vat,d.payload_json,d.currency,d.id FROM documents d WHERE d.document_type='issued_invoice'").map_err(invalid)?;
    let mut invoice_rows = invoice_query.query([]).map_err(invalid)?;
    let mut invoices = Vec::new();
    while let Some(row) = invoice_rows.next().map_err(invalid)? {
        let payload: String = row.get(1).map_err(invalid)?;
        let payload: Value = serde_json::from_str(&payload).map_err(invalid)?;
        validate_foreign_buyer(&payload)?;
        let id: i64 = row.get(3).map_err(invalid)?;
        let currency = payload["currency"].as_str().unwrap_or("DKK");
        let gross = legacy_money(row.get_ref(0).map_err(invalid)?)?;
        let paid = crate::invoice_fx::movements(tx, id, currency, false)?;
        let credited = crate::credit_note::credited(tx, id)?;
        let refunds = crate::invoice_settlement::refunded(tx, id)?;
        let mut open = gross.subtract(&paid).subtract(&credited);
        open.add(&refunds);
        let open = receivable_dkk(tx, id, &payload, &open)?;
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
        customers.push(json!({"id":id,"name":r.get::<_,String>(1).map_err(invalid)?,"address":r.get::<_,Option<String>>(2).map_err(invalid)?,"vatOrCvr":r.get::<_,Option<String>>(3).map_err(invalid)?,"email":r.get::<_,Option<String>>(4).map_err(invalid)?,"phone":r.get::<_,Option<String>>(5).map_err(invalid)?,"website":r.get::<_,Option<String>>(6).map_err(invalid)?,"eanNumber":r.get::<_,Option<String>>(7).map_err(invalid)?,"paymentTermsDays":r.get::<_,Option<i64>>(8).map_err(invalid)?.unwrap_or(14),"defaultCurrency":r.get::<_,String>(9).map_err(invalid)?,"notes":r.get::<_,Option<String>>(10).map_err(invalid)?,"openBalance":wire(&balance)?,"openBalanceCurrency":"DKK","openInvoiceCount":count,"overdueCount":overdue}));
    }
    let mut q=tx.prepare("SELECT id,name,address,vat_or_cvr,country_code,identifier_kind,identity_status,email,phone,website,default_expense_account,default_vat_treatment,notes FROM vendors WHERE archived=0 ORDER BY name,id").map_err(invalid)?;
    let vendors=q.query_map([],|r|Ok(json!({"id":r.get::<_,i64>(0)?,"name":r.get::<_,String>(1)?,"address":r.get::<_,Option<String>>(2)?,"vatOrCvr":r.get::<_,Option<String>>(3)?,"countryCode":r.get::<_,Option<String>>(4)?,"identifierKind":r.get::<_,Option<String>>(5)?,"identityStatus":r.get::<_,String>(6)?,"email":r.get::<_,Option<String>>(7)?,"phone":r.get::<_,Option<String>>(8)?,"website":r.get::<_,Option<String>>(9)?,"defaultExpenseAccount":r.get::<_,Option<String>>(10)?,"defaultVatTreatment":r.get::<_,Option<String>>(11)?,"notes":r.get::<_,Option<String>>(12)?}))).map_err(invalid)?.collect::<Result<Vec<_>,_>>().map_err(invalid)?;
    for customer in &mut customers {
        crate::gdpr::overlay_contact(tx, "customers", customer)?;
    }
    let mut vendors = vendors;
    for vendor in &mut vendors {
        crate::gdpr::overlay_contact(tx, "vendors", vendor)?;
    }
    base["customers"] = json!(customers);
    base["vendors"] = json!(vendors);
    Ok(base)
}
fn party(input: Option<&Party>, default: &Value, seller: bool) -> Result<Value, Error> {
    party_kind(input, default, seller, false, false, false)
}
fn party_kind(
    input: Option<&Party>,
    default: &Value,
    seller: bool,
    foreign: bool,
    require_buyer_vat: bool,
    simplified: bool,
) -> Result<Value, Error> {
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
    let ean = if seller {
        None
    } else {
        match sales_invoice::recipient_ean(input.and_then(|p| p.ean_number.as_deref()))? {
            Some(ean) => Some(ean),
            None => sales_invoice::recipient_ean(default["eanNumber"].as_str())?,
        }
    };
    let public = input
        .and_then(|p| p.public_recipient)
        .unwrap_or(ean.is_some());
    if seller
        && input.is_some_and(|p| {
            p.ean_number.is_some()
                || p.public_recipient.is_some()
                || p.buyer_reference.is_some()
                || p.order_reference.is_some()
        })
    {
        return Err(refused("recipient routing fields belong to buyer only"));
    }
    if !seller && public && ean.is_none() {
        return Err(refused(
            "public-recipient invoice requires buyer.eanNumber as 13 digits",
        ));
    }
    let vat = input
        .and_then(|p| p.vat_or_cvr.as_deref())
        .filter(|s| !s.trim().is_empty())
        .or_else(|| default["vatOrCvr"].as_str())
        .filter(|s| !s.trim().is_empty())
        .map(|s| sales_invoice::text(s, 1024))
        .transpose()?;
    if let Some(vat) = &vat {
        if seller || (!foreign && (require_buyer_vat || public || ean.is_some())) {
            let digits = vat.strip_prefix("DK").unwrap_or(vat);
            if digits.len() != 8 || !digits.bytes().all(|b| b.is_ascii_digit()) {
                return Err(refused(
                    "native domestic invoice VAT/CVR identifiers must be Danish 8-digit values",
                ));
            }
        }
    } else if seller || foreign || (require_buyer_vat && ean.is_none()) {
        return Err(refused(
            "native domestic invoice requires seller CVR and buyer CVR or public EAN",
        ));
    }
    if simplified && (public || ean.is_some()) {
        return Err(refused(
            "simplified invoice cannot address a public recipient or EAN",
        ));
    }
    let mut result = json!({});
    for (key, value) in [
        ("name", input.and_then(|p| p.name.as_ref())),
        ("address", input.and_then(|p| p.address.as_ref())),
    ] {
        if !simplified || seller || value.is_some() || default[key].as_str().is_some() {
            result[key] = json!(field(value, key)?);
        }
    }
    if let Some(vat) = vat {
        result["vatOrCvr"] = json!(vat);
    }
    if !seller {
        if let Some(ean) = ean {
            result["eanNumber"] = json!(ean);
        }
        result["publicRecipient"] = json!(public);
        for (key, value) in [
            (
                "buyerReference",
                input.and_then(|p| p.buyer_reference.as_deref()),
            ),
            (
                "orderReference",
                input.and_then(|p| p.order_reference.as_deref()),
            ),
        ] {
            if let Some(value) = sales_invoice::recipient_reference(value)? {
                result[key] = json!(value);
            }
        }
    }
    Ok(result)
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
            "domestic invoice requires a DKK company registered for VAT",
        ));
    }
    let seller = party(
        input.seller.as_ref(),
        &json!({"name":company["name"],"address":company["address"],"vatOrCvr":company["cvr"]}),
        true,
    )?;
    let mut terms = company["paymentTermsDays"].as_i64().unwrap_or(14);
    let customer = if let Some(id) = input.customer_id {
        if id <= 0 {
            return Err(refused("invalid customer id"));
        }
        let customer:Value=tx.query_row("SELECT name,address,vat_or_cvr,payment_terms_days,ean_number FROM customers WHERE id=?1 AND archived=0",[id],|r|Ok(json!({"name":r.get::<_,String>(0)?,"address":r.get::<_,Option<String>>(1)?,"vatOrCvr":r.get::<_,Option<String>>(2)?,"terms":r.get::<_,Option<i64>>(3)?,"eanNumber":r.get::<_,Option<String>>(4)?}))).optional().map_err(invalid)?.ok_or_else(||refused("active customer required"))?;
        crate::gdpr::require_contact_identity(
            tx,
            "customers",
            Some(id),
            customer["name"].as_str().unwrap(),
            customer["vatOrCvr"].as_str(),
        )?;
        if let Some(days) = customer["terms"].as_i64() {
            terms = days;
        }
        customer
    } else {
        Value::Null
    };
    let foreign = computed["vatTreatment"] == "foreign_reverse_charge";
    let simplified = computed["invoiceType"] == "simplified";
    let buyer = party_kind(
        input.buyer.as_ref(),
        &customer,
        false,
        foreign,
        computed["vatTreatment"] == "domestic_reverse_charge",
        simplified,
    )?;
    if let Some(name) = buyer["name"].as_str() {
        crate::gdpr::require_contact_identity(
            tx,
            "customers",
            input.customer_id,
            name,
            buyer["vatOrCvr"].as_str(),
        )?;
    }
    let validation = if foreign {
        let record = crate::vies::require(tx, buyer["vatOrCvr"].as_str().unwrap_or(""), now)?;
        if record["countryCode"] == "DK" {
            return Err(refused(
                "EU services buyer must be a foreign EU VAT registrant",
            ));
        }
        Some(record)
    } else {
        None
    };
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
    .identifier_label;
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
    let mut payload = json!({"invoiceType":computed["invoiceType"],"vatTreatment":computed["vatTreatment"],"issueDate":input.issue_date,"dueDate":due,"invoiceNumber":format!("{year}-{next:04}"),"status":"issued","issuedAt":now,"currency":computed["currency"],"seller":seller,"buyer":buyer,"customerId":input.customer_id,"lines":computed["lines"],"totals":computed["totals"],"payment":company["payment"]});
    if simplified && input.buyer.is_none() && input.customer_id.is_none() {
        payload.as_object_mut().unwrap().remove("buyer");
    }
    for key in [
        "reverseChargeBasis",
        "reverseChargeNote",
        "deliveryDate",
        "deliveryPeriodStart",
        "deliveryPeriodEnd",
        "unitCode",
    ] {
        if let Some(value) = computed.get(key).filter(|v| !v.is_null()) {
            payload[key] = value.clone();
        }
    }
    if let Some(record) = validation {
        payload["viesValidation"] = record;
    }
    if let Some(version) = computed.get("fxValuationVersion") {
        payload["fxValuationVersion"] = version.clone();
    }
    // Validate the frozen allocation before reserving a consecutive number.
    rentemester_engine::sales_fx::project(&payload)?;
    if allocate {
        crate::journal_post::require_current(tx)?;
        crate::sequences::next(tx, "issued_invoice", &scope, floor)?;
    }
    Ok(payload)
}
fn validate_foreign_buyer(payload: &Value) -> Result<(), Error> {
    if payload["vatTreatment"] == "foreign_reverse_charge" {
        rentemester_engine::vies::require_record(
            &payload["viesValidation"],
            payload["buyer"]["vatOrCvr"].as_str().unwrap_or(""),
            payload["issuedAt"].as_str().unwrap_or(""),
        )?;
    }
    Ok(())
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
    register_with_principal(
        tx,
        payload,
        json_file,
        pdf_file,
        actor,
        crate::idempotency::PrincipalKind::User,
    )
}
pub fn register_with_principal(
    tx: &Transaction<'_>,
    payload: &Value,
    json_file: Artifact<'_>,
    pdf_file: Artifact<'_>,
    actor: &str,
    principal: crate::idempotency::PrincipalKind,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    let projection = sales_invoice::project(payload)?;
    rentemester_engine::sales_fx::project(payload)?;
    validate_foreign_buyer(payload)?;
    let vat_amount = wire(&projection.vat)?;
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
        let inserted:i64=tx.query_row("INSERT INTO documents(document_no,source,original_filename,stored_path,mime_type,sha256_hash,supplier_name,invoice_no,invoice_date,amount_inc_vat,currency,status,document_type,delivery_description,sender_name,sender_address,sender_vat_cvr,recipient_name,recipient_address,recipient_vat_cvr,vat_amount,payload_json,retain_until) VALUES(?1,'rentemester',?2,?3,?4,?5,?6,?7,?8,?9,?20,'issued',?10,?11,?6,?12,?13,?14,?15,?16,?17,?18,?19) RETURNING id",params![no,file.filename,file.path,file.mime,file.hash,payload["seller"]["name"].as_str(),payload["invoiceNumber"].as_str(),payload["issueDate"].as_str(),payload["totals"]["grossAmount"].as_f64(),kind,payload["lines"].to_string(),payload["seller"]["address"].as_str(),payload["seller"]["vatOrCvr"].as_str(),payload["buyer"]["name"].as_str(),payload["buyer"]["address"].as_str(),payload["buyer"]["vatOrCvr"].as_str(),vat_amount.as_f64(),serialized,retention,payload["currency"].as_str().unwrap_or("DKK")],|r|r.get(0)).map_err(invalid)?;
        if kind == "issued_invoice" {
            id = inserted;
        }
        audit_with_principal(
            tx,
            if kind == "issued_invoice" {
                "invoice_issue"
            } else {
                "invoice_render_pdf"
            },
            inserted,
            "Issued immutable sales invoice evidence",
            actor,
            principal,
        )?;
    }
    let mut result = json!({"documentId":id,"invoiceType":payload["invoiceType"],"currency":payload["currency"],"totals":payload["totals"],"invoiceNumber":payload["invoiceNumber"],"netAmount":payload["totals"]["netAmount"],"vatRate":payload["totals"]["vatRate"],"vatTreatment":payload["vatTreatment"],"reverseChargeBasis":payload["reverseChargeBasis"],"reverseChargeNote":payload["reverseChargeNote"],"vatAmount":vat_amount,"grossAmount":payload["totals"]["grossAmount"],"fxRateToDkk":payload["totals"]["fxRateToDkk"],"netAmountDkk":payload["totals"]["netAmountDkk"],"vatAmountDkk":payload["totals"]["vatAmountDkk"],"grossAmountDkk":payload["totals"]["grossAmountDkk"],"lines":payload["lines"]});
    for key in [
        "deliveryDate",
        "deliveryPeriodStart",
        "deliveryPeriodEnd",
        "unitCode",
    ] {
        if let Some(value) = payload.get(key) {
            result[key] = value.clone();
        }
    }
    Ok(result)
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
    post_with_principal(
        tx,
        input,
        subject,
        latest,
        crate::idempotency::PrincipalKind::User,
    )
}
pub fn post_with_principal(
    tx: &Transaction<'_>,
    input: &Post,
    subject: &str,
    latest: Date,
    principal: crate::idempotency::PrincipalKind,
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
    validate_foreign_buyer(&payload)?;
    let original_projection = sales_invoice::project(&payload)?;
    let projection = rentemester_engine::sales_fx::project(&payload)?;
    let foreign = payload["currency"] != "DKK";
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
    let gross = projection.gross.clone();
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
    let mut lines = vec![line(&debtors, gross.clone(), Money::default(), None)?];
    // Retain each source base separately: rounding evidence and VAT category
    // must survive even when all lines use the same selected revenue account.
    for source_line in &projection.lines {
        if source_line.net != Money::default() {
            lines.push(line(
                revenue,
                Money::default(),
                source_line.net.clone(),
                Some(source_line.vat_code),
            )?);
        }
    }
    if projection.vat != Money::default() {
        let output = role(tx, "output_vat", "vat", "credit")?;
        lines.push(line(
            &output,
            Money::default(),
            projection.vat.clone(),
            None,
        )?);
    }
    let request = ManualJournal {
        transaction_date: Date::parse(
            payload["issueDate"]
                .as_str()
                .ok_or_else(|| refused("missing invoice date"))?,
        )?,
        text: format!("Salg {}", payload["invoiceNumber"].as_str().unwrap_or("")),
        currency: Currency::parse(payload["currency"].as_str().unwrap_or("DKK"))?,
        document_id: Some(DocumentId::from_sqlite(input.invoice_document_id)),
        source_bank_transaction_id: None,
        amount_foreign: foreign.then_some(original_projection.gross),
        amount_dkk: foreign.then_some(gross.clone()),
        fx_rate_to_dkk: if foreign {
            // Both native v1 and historical aggregates were verified above.
            // Historical originals retain a raw quote that TS rounded to six decimals.
            Some(rentemester_engine::fx::FxRate::parse(
                &payload["totals"]["fxRateToDkk"]
                    .as_number()
                    .ok_or_else(|| refused("verified frozen invoice quote missing"))?
                    .to_string(),
            )?)
        } else {
            None
        },
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
            program: match principal {
                crate::idempotency::PrincipalKind::User => "rentemester-rust",
                crate::idempotency::PrincipalKind::ServiceAccount => "service-principal",
            },
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
    audit_with_principal(
        tx,
        "invoice_post",
        input.invoice_document_id,
        "Booked issued invoice with explicit receivable identity",
        subject,
        principal,
    )?;
    Ok(result)
}
/// Original-currency amounts never enter company DKK aggregates.
fn receivable_dkk(
    tx: &Transaction<'_>,
    id: i64,
    payload: &Value,
    open: &Money,
) -> Result<Money, Error> {
    if payload["currency"] == "DKK" {
        return Ok(open.clone());
    }
    let valuation = rentemester_engine::sales_fx::project(payload)?;
    let debtor:Option<i64>=tx.query_row("SELECT p.receivable_account_id FROM issued_invoice_postings p JOIN journal_entries j ON j.id=p.journal_entry_id WHERE p.invoice_document_id=?1 AND j.status='posted' AND NOT EXISTS(SELECT 1 FROM journal_entries r WHERE r.reversal_of_entry_id=j.id)",[id],|r|r.get(0)).optional().map_err(invalid)?;
    if let Some(debtor) = debtor {
        crate::invoice_settlement::principal_carrying(tx, id, debtor)
    } else {
        let linked: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM issued_invoice_postings WHERE invoice_document_id=?1)",
                [id],
                |r| r.get(0),
            )
            .map_err(invalid)?;
        if linked {
            return Err(refused("foreign invoice posting is no longer active"));
        }
        if open != &sales_invoice::project(payload)?.gross {
            return Err(refused(
                "unposted foreign invoice has unexplained movements",
            ));
        }
        Ok(valuation.gross)
    }
}
/// Shared current original-currency counters and frozen DKK carrying; no date cutoff.
pub(crate) struct InvoiceProjection {
    pub paid: Money,
    pub credited: Money,
    pub refunds: Money,
    pub open: Money,
    pub creditable: Money,
    pub gross_dkk: Money,
    pub open_dkk: Money,
    pub creditable_dkk: Money,
    pub written: crate::invoice_bad_debt::History,
}
pub(crate) fn project_invoice(
    tx: &Transaction<'_>,
    id: i64,
    payload: &Value,
    gross: &Money,
) -> Result<InvoiceProjection, Error> {
    let currency = payload["currency"].as_str().unwrap_or("DKK");
    let written = crate::invoice_bad_debt::history(tx, id)?;
    let paid = crate::invoice_fx::movements(tx, id, currency, false)?;
    let credited = crate::credit_note::credited(tx, id)?;
    let creditable = gross.subtract(&credited);
    let refunds = crate::invoice_settlement::refunded(tx, id)?;
    let mut open = creditable.subtract(&paid);
    open.add(&refunds);
    open = open.subtract(&written.gross);
    let valuation = rentemester_engine::sales_fx::project(payload)?;
    let open_dkk = receivable_dkk(tx, id, payload, &open)?;
    let credited_dkk = if currency == "DKK" {
        credited.clone()
    } else {
        crate::invoice_fx::credited_dkk(tx, id)?
    };
    let creditable_dkk = valuation.gross.subtract(&credited_dkk);
    let gross_dkk = valuation.gross;
    Ok(InvoiceProjection {
        paid,
        credited,
        refunds,
        open,
        creditable,
        gross_dkk,
        open_dkk,
        creditable_dkk,
        written,
    })
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
        validate_foreign_buyer(&payload)?;
        let gross = legacy_money(r.get_ref(5).map_err(invalid)?)?;
        let InvoiceProjection {
            paid,
            credited,
            refunds,
            open,
            creditable,
            gross_dkk,
            open_dkk,
            creditable_dkk,
            written,
        } = project_invoice(tx, id, &payload, &gross)?;
        total.add(&gross_dkk);
        open_total.add(&open_dkk);
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
        if overdue > 0 && !open.is_negative() && open != Money::default() {
            overdue_count += 1;
        }
        rows.push(json!({"documentId":id,"badDebtWriteOffs":written.entries,"totalBadDebtWrittenOff":wire(&written.gross)?,"totalBadDebtWrittenOffDkk":wire(&written.gross_dkk)?,"invoiceType":payload["invoiceType"],"invoiceNo":r.get::<_,String>(1).map_err(invalid)?,"invoiceDate":r.get::<_,String>(2).map_err(invalid)?,"customerName":r.get::<_,Option<String>>(3).map_err(invalid)?,"customerEmail":null,"customerId":payload["customerId"],"buyerEanNumber":sales_invoice::recipient_ean(payload["buyer"]["eanNumber"].as_str()).ok().flatten(),"buyerPublicRecipient":payload["buyer"]["publicRecipient"]==true || sales_invoice::recipient_ean(payload["buyer"]["eanNumber"].as_str()).ok().flatten().is_some(),"peppolStatus":crate::public_einvoice::get(tx,id)?,"lastEmailedAt":null,"lastReminderAt":null,"lastReminderSequence":0,"grossAmount":wire(&gross)?,"openBalance":wire(&open)?,"grossAmountDkk":wire(&gross_dkk)?,"openBalanceDkk":wire(&open_dkk)?,"creditableBalanceDkk":wire(&creditable_dkk)?,"fxRateToDkk":payload["totals"]["fxRateToDkk"],"currency":r.get::<_,String>(4).map_err(invalid)?,"creditableBalance":wire(&creditable)?,"status":if open.is_negative(){"overpaid"}else if open==Money::default(){if written.gross!=Money::default(){"written_off"}else if refunds!=Money::default(){"refunded"}else if credited==gross && paid==Money::default(){"credited"}else{"paid"}}else if overdue>0{"overdue"}else{"open"},"effectiveDueDate":due,"overdueDays":overdue,"posted":r.get::<_,Option<i64>>(7).map_err(invalid)?.is_some()}));
    }
    base["invoices"] = json!(rows);
    base["totalGross"] = wire(&total)?;
    base["totalOpen"] = wire(&open_total)?;
    base["totalGrossDkk"] = wire(&total)?;
    base["totalOpenDkk"] = wire(&open_total)?;
    base["totalsCurrency"] = json!("DKK");
    base["overdueCount"] = json!(overdue_count);
    let mut q=tx.prepare("SELECT account_no,name FROM accounts WHERE active=1 AND type='income' AND normal_balance='credit' ORDER BY account_no").map_err(invalid)?;
    let accounts = q
        .query_map([], |r| {
            Ok(json!({"accountNo":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?}))
        })
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    base["nativeSales"] = json!({"revenueAccounts":accounts,"settlementAvailable":true,"creditAvailable":true,"refundAvailable":true});
    Ok(base)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ordinary_foreign_buyer_and_metadata_are_immutable_without_tax_reclassification() {
        let mut control = rusqlite::Connection::open_in_memory().unwrap();
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic Seller").unwrap();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        tx.execute_batch("UPDATE companies SET address='Synthetic Street 1',cvr='87654321',vat_period_type='quarter' WHERE id=1").unwrap();
        let source = json!({"issueDate":"2026-10-20","buyer":{"name":"Synthetic Foreign Buyer","address":"Synthetic Street 2","vatOrCvr":"DE123456789"},"deliveryPeriodStart":"2026-10-01","deliveryPeriodEnd":"2026-10-31","unitCode":"HUR","lines":[{"description":"Synthetic service","quantity":2,"unitPriceExVat":100,"unitCode":"H87"}]});
        let input: Input = serde_json::from_value(source.clone()).unwrap();
        let payload = prepare(&tx, &input, "2026-10-20T12:00:00Z", true).unwrap();
        assert_eq!(payload["buyer"]["vatOrCvr"], "DE123456789");
        assert_eq!(payload["vatTreatment"], "standard");
        assert_eq!(payload["totals"]["grossAmount"], 250.0);
        assert!(payload.get("viesValidation").is_none());
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
        let original: String = tx
            .query_row(
                "SELECT payload_json FROM documents WHERE id=?1",
                [result["documentId"].as_i64().unwrap()],
                |r| r.get(0),
            )
            .unwrap();
        let saved: Value = serde_json::from_str(&original).unwrap();
        assert_eq!(saved, payload);
        for key in ["deliveryPeriodStart", "deliveryPeriodEnd", "unitCode"] {
            assert_eq!(result[key], saved[key]);
        }
        for key in ["deliveryPeriodStart", "deliveryPeriodEnd", "unitCode"] {
            assert_eq!(saved[key], source[key]);
        }
        assert_eq!(saved["lines"][0]["unitCode"], "H87");
        assert!(pdf_id(&tx, result["documentId"].as_i64().unwrap()).is_ok());
        for overrides in [
            json!({"seller":{"name":"Synthetic Seller","address":"Synthetic 1","vatOrCvr":"DE123456789"}}),
            json!({"vatTreatment":"domestic_reverse_charge","reverseChargeBasis":"DK_MOMSLOVEN_§46_STK_1_NR_6","reverseChargeNote":"Synthetic reverse charge"}),
            json!({"buyer":{"name":"Synthetic Public Buyer","address":"Synthetic 2","vatOrCvr":"DE123456789","eanNumber":"5790000000000"}}),
            json!({"vatTreatment":"foreign_reverse_charge","reverseChargeBasis":"EU_B2B_SERVICES","reverseChargeNote":"Synthetic reverse charge"}),
        ] {
            let mut invalid = source.clone();
            invalid
                .as_object_mut()
                .unwrap()
                .extend(overrides.as_object().unwrap().clone());
            assert!(
                prepare(
                    &tx,
                    &serde_json::from_value(invalid).unwrap(),
                    "2026-10-20T12:00:00Z",
                    false
                )
                .is_err(),
                "{overrides}"
            );
        }
    }
    #[test]
    fn recipient_overrides_ean_only_identity_and_invalid_fields_are_explicit() {
        let mut control = rusqlite::Connection::open_in_memory().unwrap();
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic Public Seller")
            .unwrap();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        tx.execute_batch("UPDATE companies SET address='Synthetic Street 74',cvr='87654321',vat_period_type='quarter' WHERE id=1").unwrap();
        let customer:Customer=serde_json::from_value(json!({"name":"Synthetic Public Buyer","address":"Synthetic Public Street","eanNumber":"5790000000000"})).unwrap();
        let c = create_customer(&tx, &customer, "synthetic").unwrap();
        let base = json!({"issueDate":"2026-10-18","customerId":c["id"],"buyer":{"eanNumber":"5790 0000 00001","publicRecipient":false,"buyerReference":" Reviewed æ reference ","orderReference":" Reviewed order "},"lines":[{"description":"Synthetic service","quantity":1,"unitPriceExVat":100}]});
        let input: Input = serde_json::from_value(base.clone()).unwrap();
        let snapshot = prepare(&tx, &input, "2026-10-18T12:00:00Z", false).unwrap();
        assert_eq!(snapshot["buyer"]["name"], "Synthetic Public Buyer");
        assert_eq!(snapshot["buyer"]["eanNumber"], "5790000000001");
        assert_eq!(snapshot["buyer"]["publicRecipient"], false);
        assert_eq!(snapshot["buyer"]["buyerReference"], "Reviewed æ reference");
        assert_eq!(snapshot["buyer"]["orderReference"], "Reviewed order");
        assert!(snapshot["buyer"].get("vatOrCvr").is_none());
        for (field, value) in [
            ("eanNumber", json!("bad-route")),
            ("buyerReference", json!(" ")),
            ("orderReference", json!("bad\nreference")),
            ("buyerReference", json!("x".repeat(1025))),
        ] {
            let mut invalid = base.clone();
            invalid["buyer"][field] = value;
            let input: Input = serde_json::from_value(invalid).unwrap();
            assert!(prepare(&tx, &input, "2026-10-18T12:00:00Z", false).is_err());
        }
        let mut no_ean = base.clone();
        no_ean["customerId"] = Value::Null;
        no_ean["buyer"] = json!({"name":"Synthetic public buyer","address":"Synthetic public street","publicRecipient":true});
        assert!(
            prepare(
                &tx,
                &serde_json::from_value(no_ean).unwrap(),
                "2026-10-18T12:00:00Z",
                false
            )
            .is_err()
        );
        tx.execute("UPDATE companies SET cvr=NULL WHERE id=1", [])
            .unwrap();
        assert!(prepare(&tx, &input, "2026-10-18T12:00:00Z", false).is_err());
        let domestic = party(
            None,
            &json!({"name":"Synthetic buyer","address":"Synthetic street","vatOrCvr":"12345678"}),
            false,
        )
        .unwrap();
        assert_eq!(domestic["publicRecipient"], false);
        assert!(domestic.get("eanNumber").is_none());
    }
    #[test]
    fn issue_number_preview_and_explicit_post_keep_the_ledger_balanced() {
        let mut control = rusqlite::Connection::open_in_memory().unwrap();
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic Seller").unwrap();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        tx.execute_batch("UPDATE companies SET address='Synthetic Street 1',cvr='87654321',vat_period_type='quarter' WHERE id=1; INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('R','Synthetic receivable','asset','debit'),('S','Synthetic sale','income','credit'),('V','Synthetic output VAT','vat','credit');INSERT INTO account_role_mappings(role,account_no,version,confirmed_by) VALUES('debtors','R',1,'synthetic'),('output_vat','V',1,'synthetic');").unwrap();
        let customer:Customer=serde_json::from_value(json!({"name":"Synthetic Buyer","address":"Synthetic Street 2","vatOrCvr":"DK12345678","eanNumber":"5790000000000","paymentTermsDays":30})).unwrap();
        let customer = create_customer(&tx, &customer, "synthetic").unwrap();
        let input:Input=serde_json::from_value(json!({"issueDate":"2026-10-18","customerId":customer["id"],"buyer":{"buyerReference":"Synthetic reviewed buyer ref","orderReference":"Synthetic order 74"},"lines":[{"description":"Synthetic service","quantity":2,"unitPriceExVat":500}]})).unwrap();
        let preview = prepare(&tx, &input, "2026-10-18T12:00:00Z", false).unwrap();
        assert_eq!(preview["invoiceNumber"], "2026-0001");
        assert_eq!(preview["dueDate"], "2026-11-17");
        assert_eq!(preview["buyer"]["eanNumber"], "5790000000000");
        assert_eq!(preview["buyer"]["publicRecipient"], true);
        assert_eq!(
            preview["buyer"]["buyerReference"],
            "Synthetic reviewed buyer ref"
        );
        assert_eq!(preview["buyer"]["orderReference"], "Synthetic order 74");
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
        let original: String = tx
            .query_row(
                "SELECT payload_json FROM documents WHERE id=?1",
                [id],
                |r| r.get(0),
            )
            .unwrap();
        let customer_id = customer["id"].as_i64().unwrap();
        update_customer(&tx,customer_id,&json!({"name":"Synthetic Renamed Buyer","address":"Synthetic New Street 3","paymentTermsDays":7,"email":"synthetic@example.invalid","eanNumber":"5790000000001"}),"synthetic").unwrap();
        let next = prepare(&tx, &input, "2026-10-18T12:00:00Z", false).unwrap();
        assert_eq!(next["buyer"]["name"], "Synthetic Renamed Buyer");
        assert_eq!(next["buyer"]["address"], "Synthetic New Street 3");
        assert_eq!(next["dueDate"], "2026-10-25");
        assert_eq!(next["buyer"]["vatOrCvr"], "DK12345678");
        assert_eq!(next["buyer"]["eanNumber"], "5790000000001");
        let listed = list(&tx, "synthetic", "2026-10-18", Some("2026")).unwrap();
        assert_eq!(listed["invoices"][0]["buyerEanNumber"], "5790000000000");
        assert_eq!(listed["invoices"][0]["buyerPublicRecipient"], true);
        assert!(listed["invoices"][0]["peppolStatus"].is_null());
        assert_eq!(
            tx.query_row::<String, _, _>(
                "SELECT payload_json FROM documents WHERE id=?1",
                [id],
                |r| r.get(0)
            )
            .unwrap(),
            original
        );
        for invalid_patch in [
            json!({"name":null}),
            json!({"name":" "}),
            json!({"unknownField":true}),
            json!({"paymentTermsDays":0}),
            json!({"defaultCurrency":null}),
        ] {
            assert!(update_customer(&tx, customer_id, &invalid_patch, "synthetic").is_err());
        }
        assert!(update_customer(&tx, 999, &json!({"name":"Missing"}), "synthetic").is_err());
        update_customer(&tx, customer_id, &json!({"email":null}), "synthetic").unwrap();
        assert_eq!(
            contacts(&tx, "synthetic", "2026-10-18").unwrap()["customers"][0]["email"],
            Value::Null
        );
        assert_eq!(tx.query_row::<i64,_,_>("SELECT count(*) FROM audit_log WHERE event_type='customer_update' AND entity_type='customer'",[],|r|r.get(0)).unwrap(),2);

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

#[cfg(test)]
#[path = "sales/vat_tests.rs"]
mod vat_tests;

#[cfg(test)]
#[path = "sales/foreign_tests.rs"]
mod foreign_tests;

#[cfg(test)]
#[path = "sales/private_tests.rs"]
mod private_tests;
