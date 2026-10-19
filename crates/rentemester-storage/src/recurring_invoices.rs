//! Immutable recurring specifications; generation is an explicit fenced transaction.
use crate::{invalid, sales};
use rentemester_engine::{
    Error,
    calendar::valid_iso_date,
    fiscal_year::Date,
    sales_invoice::{Input, Line, Party, refused},
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::Deserialize;
use serde_json::{Number, Value, json};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Create {
    pub name: String,
    pub interval: String,
    pub interval_count: Option<u32>,
    pub delivery_channel: Option<String>,
    pub first_issue_date: String,
    pub payment_terms_days: u32,
    pub delivery_period_mode: Option<String>,
    pub notes: Option<String>,
    pub vat_rate_percent: Number,
    pub currency: Option<String>,
    pub customer_id: Option<i64>,
    pub buyer: Option<Party>,
    pub lines: Vec<Line>,
    pub confirm: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Generate {
    pub as_of_date: String,
    pub confirm: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Retire {
    pub reason: Option<String>,
    pub confirm: bool,
}
struct Template {
    id: i64,
    name: String,
    interval: String,
    count: u32,
    first: String,
    next: String,
    terms: u32,
    mode: String,
    payload: Value,
    active: bool,
    channel: String,
}
fn load(tx: &Connection, id: i64) -> Result<Template, Error> {
    tx.query_row("SELECT id,name,interval,interval_count,first_issue_date,next_issue_date,payment_terms_days,delivery_period_mode,payload_json,active,delivery_channel FROM recurring_invoice_templates WHERE id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?,r.get::<_,String>(8)?,r.get(9)?,r.get(10)?))).optional().map_err(invalid)?.ok_or_else(||refused("recurring template not found")).and_then(|(id,name,interval,count,first,next,terms,mode,payload,active,channel)|Ok(Template{id,name,interval,count,first,next,terms,mode,payload:serde_json::from_str(&payload).map_err(invalid)?,active,channel}))
}
fn audit(
    tx: &Transaction<'_>,
    id: i64,
    event: &str,
    message: &str,
    actor: &str,
) -> Result<(), Error> {
    audit_with_principal(
        tx,
        id,
        event,
        message,
        actor,
        crate::idempotency::PrincipalKind::User,
    )
}
fn audit_with_principal(
    tx: &Transaction<'_>,
    id: i64,
    event: &str,
    message: &str,
    actor: &str,
    principal: crate::idempotency::PrincipalKind,
) -> Result<(), Error> {
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES(?1,'recurring_invoice_template',?2,?3,?4)",params![event,id.to_string(),message,format!("{}:{actor}",match principal {crate::idempotency::PrincipalKind::User=>"user",crate::idempotency::PrincipalKind::ServiceAccount=>"service_account"})]).map_err(invalid)?;
    Ok(())
}
fn months(interval: &str) -> Result<u32, Error> {
    match interval {
        "weekly" => Ok(0),
        "monthly" => Ok(1),
        "quarterly" => Ok(3),
        "yearly" => Ok(12),
        _ => Err(refused(
            "interval must be weekly, monthly, quarterly or yearly",
        )),
    }
}
fn add_days(date: &str, days: i64) -> Result<String, Error> {
    let value = rentemester_engine::vat_period::date(date)?
        .checked_add(time::Duration::days(days))
        .ok_or_else(|| refused("recurring date exceeds supported calendar"))?
        .to_string();
    Date::parse(&value)?;
    Ok(value)
}
fn add_months(first: &str, offset: u32) -> Result<String, Error> {
    Date::parse(first)?;
    let year = first[..4].parse::<u32>().unwrap();
    let month = first[5..7].parse::<u32>().unwrap();
    let mut day = first[8..].parse::<u32>().unwrap();
    let target = year * 12 + month - 1 + offset;
    if target / 12 > 9999 {
        return Err(refused("recurring date exceeds supported calendar"));
    }
    loop {
        let value = format!("{:04}-{:02}-{day:02}", target / 12, target % 12 + 1);
        if valid_iso_date(&value) {
            return Ok(value);
        }
        day -= 1;
    }
}
fn issue_date(first: &str, interval: &str, count: u32, index: u32) -> Result<String, Error> {
    if !(1..=120).contains(&count) {
        return Err(refused("interval count must be 1–120"));
    }
    let step = months(interval)?;
    if step == 0 {
        add_days(first, i64::from(count) * i64::from(index) * 7)
    } else {
        add_months(
            first,
            step.checked_mul(count)
                .and_then(|n| n.checked_mul(index))
                .ok_or_else(|| refused("recurring period exceeds supported range"))?,
        )
    }
}
fn period(t: &Template, as_of: &str) -> Result<u32, Error> {
    Date::parse(as_of)?;
    if as_of < t.first.as_str() {
        return Err(refused("no invoice is due yet"));
    }
    if t.interval == "weekly" {
        let days = (rentemester_engine::vat_period::date(as_of)?
            - rentemester_engine::vat_period::date(&t.first)?)
        .whole_days();
        return Ok((days / (i64::from(t.count) * 7)) as u32);
    }
    let serial = |date: &str| {
        date[..4].parse::<u32>().unwrap() * 12 + date[5..7].parse::<u32>().unwrap() - 1
    };
    let mut index = (serial(as_of) - serial(&t.first)) / (months(&t.interval)? * t.count);
    if issue_date(&t.first, &t.interval, t.count, index)?.as_str() > as_of {
        index -= 1;
    }
    Ok(index)
}
fn window(mode: &str, issue: &str, next: &str) -> Result<(Option<String>, Option<String>), Error> {
    match mode {
        "none" => Ok((None, None)),
        "interval_window" => Ok((Some(issue.to_owned()), Some(add_days(next, -1)?))),
        "issue_month" => {
            let start = format!("{}-01", &issue[..7]);
            let mut day = 31;
            let end = loop {
                let d = format!("{}-{day:02}", &issue[..7]);
                if valid_iso_date(&d) {
                    break d;
                }
                day -= 1;
            };
            Ok((Some(start), Some(end)))
        }
        _ => Err(refused("unsupported delivery period mode")),
    }
}
pub fn create(tx: &Transaction<'_>, i: &Create, actor: &str, now: &str) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    let name = rentemester_engine::sales_invoice::text(&i.name, 256)?;
    let count = i.interval_count.unwrap_or(1);
    months(&i.interval)?;
    if !i.confirm
        || i.delivery_channel.as_deref().unwrap_or("manual") != "manual"
        || i.payment_terms_days > 365
        || i.notes.as_ref().is_some_and(|s| s.len() > 4096)
    {
        return Err(refused(
            "confirmed manual template and payment terms 0–365 days required",
        ));
    }
    issue_date(&i.first_issue_date, &i.interval, count, 0)?;
    let mode = i.delivery_period_mode.as_deref().unwrap_or("issue_month");
    window(
        mode,
        &i.first_issue_date,
        &issue_date(&i.first_issue_date, &i.interval, count, 1)?,
    )?;
    let raw = json!({"issueDate":i.first_issue_date,"dueDate":add_days(&i.first_issue_date,i64::from(i.payment_terms_days))?,"customerId":i.customer_id,"buyer":i.buyer.as_ref().map(|b|json!({"name":b.name,"address":b.address,"vatOrCvr":b.vat_or_cvr})),"vatRatePercent":i.vat_rate_percent,"currency":i.currency,"lines":i.lines.iter().map(|l|json!({"description":l.description,"quantity":l.quantity,"unitPriceExVat":l.unit_price_ex_vat})).collect::<Vec<_>>()});
    let input: Input = serde_json::from_value(raw.clone()).map_err(invalid)?;
    let snapshot = sales::prepare(tx, &input, now, false)?;
    // Freeze parties and line specification. New generations still validate current customer/company eligibility.
    let mut stored = raw;
    stored["seller"] = snapshot["seller"].clone();
    stored["buyer"] = snapshot["buyer"].clone();
    stored.as_object_mut().unwrap().remove("issueDate");
    stored.as_object_mut().unwrap().remove("dueDate");
    let id:i64=tx.query_row("INSERT INTO recurring_invoice_templates(name,interval,interval_count,delivery_channel,first_issue_date,next_issue_date,payment_terms_days,delivery_period_mode,payload_json,notes) VALUES(?1,?2,?3,'manual',?4,?4,?5,?6,?7,?8) RETURNING id",params![name,i.interval,count,i.first_issue_date,i.payment_terms_days,mode,stored.to_string(),i.notes],|r|r.get(0)).map_err(invalid)?;
    audit(
        tx,
        id,
        "recurring_invoice_template_create",
        &format!("Created {name}; explicit manual generation"),
        actor,
    )?;
    Ok(
        json!({"templateId":id,"name":name,"interval":i.interval,"intervalCount":count,"deliveryChannel":"manual","firstIssueDate":i.first_issue_date}),
    )
}
pub struct Pending {
    pub template_id: i64,
    pub period_index: u32,
    pub payload: Value,
    next: String,
    start: Option<String>,
    end: Option<String>,
}
pub enum Prepared {
    Existing(Value),
    Issue(Pending),
}
pub fn prepare(
    tx: &Transaction<'_>,
    id: i64,
    i: &Generate,
    today: &str,
    now: &str,
) -> Result<Prepared, Error> {
    crate::journal_post::require_current(tx)?;
    if !i.confirm || Date::parse(&i.as_of_date)? > Date::parse(today)? {
        return Err(refused("confirm an as-of date no later than today"));
    }
    let t = load(tx, id)?;
    if t.channel != "manual" {
        return Err(refused("only explicit manual templates may generate here"));
    }
    if !t.active {
        return Err(refused("recurring template is retired"));
    }
    let index = period(&t, &i.as_of_date)?;
    let existing:Option<Value>=tx.query_row("SELECT g.document_id,g.invoice_number,g.issue_date,g.delivery_period_start,g.delivery_period_end,d.payload_json FROM recurring_invoice_generations g JOIN documents d ON d.id=g.document_id AND d.document_type='issued_invoice' WHERE g.template_id=?1 AND g.period_index=?2",params![id,index],|r|{let payload: String=r.get(5)?;let payload:Value=serde_json::from_str(&payload).map_err(|_|rusqlite::Error::InvalidQuery)?;Ok(json!({"created":false,"templateId":id,"periodIndex":index,"documentId":r.get::<_,i64>(0)?,"invoiceNumber":r.get::<_,String>(1)?,"issueDate":r.get::<_,String>(2)?,"dueDate":payload["dueDate"],"deliveryPeriodStart":r.get::<_,Option<String>>(3)?,"deliveryPeriodEnd":r.get::<_,Option<String>>(4)?}))}).optional().map_err(invalid)?;
    if let Some(existing) = existing {
        return Ok(Prepared::Existing(existing));
    }
    let claimed:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM recurring_invoice_generation_claims WHERE template_id=?1 AND period_index=?2)",params![id,index],|r|r.get(0)).map_err(invalid)?;
    if claimed {
        return Err(refused(
            "generation claim exists without invoice evidence; reconcile before retrying",
        ));
    }
    let issue = issue_date(&t.first, &t.interval, t.count, index)?;
    let next = issue_date(&t.first, &t.interval, t.count, index + 1)?;
    let (start, end) = window(&t.mode, &issue, &next)?;
    let mut raw = t.payload;
    // Historical TS templates also contain computed invoice fields. Select the same bounded native input, without silently changing their tax treatment.
    let rate = raw
        .get("vatRatePercent")
        .cloned()
        .or_else(|| raw["totals"]["vatRate"].as_f64().map(|v| json!(v * 100.0)))
        .ok_or_else(|| refused("template VAT rate unavailable"))?;
    let lines=raw["lines"].as_array().ok_or_else(||refused("template lines unavailable"))?.iter().map(|l|json!({"description":l["description"],"quantity":l["quantity"],"unitPriceExVat":l["unitPriceExVat"]})).collect::<Vec<_>>();
    raw = json!({"issueDate":issue,"dueDate":add_days(&issue,i64::from(t.terms))?,"customerId":raw["customerId"],"buyer":raw["buyer"],"seller":raw["seller"],"vatRatePercent":rate,"currency":raw["currency"],"lines":lines});
    let input: Input = serde_json::from_value(raw).map_err(invalid)?;
    let mut payload = sales::prepare(tx, &input, now, true)?;
    payload["deliveryPeriodStart"] = json!(start);
    payload["deliveryPeriodEnd"] = json!(end);
    payload["recurringTemplateId"] = json!(id);
    payload["recurringPeriodIndex"] = json!(index);
    tx.execute(
        "INSERT INTO recurring_invoice_generation_claims(template_id,period_index) VALUES(?1,?2)",
        params![id, index],
    )
    .map_err(invalid)?;
    Ok(Prepared::Issue(Pending {
        template_id: id,
        period_index: index,
        payload,
        next: next.max(t.next),
        start,
        end,
    }))
}
pub fn finish(
    tx: &Transaction<'_>,
    pending: &Pending,
    issued: &Value,
    actor: &str,
) -> Result<Value, Error> {
    finish_with_principal(
        tx,
        pending,
        issued,
        actor,
        crate::idempotency::PrincipalKind::User,
    )
}
pub fn finish_with_principal(
    tx: &Transaction<'_>,
    pending: &Pending,
    issued: &Value,
    actor: &str,
    principal: crate::idempotency::PrincipalKind,
) -> Result<Value, Error> {
    let id = issued["documentId"]
        .as_i64()
        .ok_or_else(|| refused("missing generated invoice"))?;
    let no = pending.payload["invoiceNumber"]
        .as_str()
        .ok_or_else(|| refused("missing generated number"))?;
    tx.execute("INSERT INTO recurring_invoice_generations(template_id,period_index,document_id,invoice_number,issue_date,delivery_period_start,delivery_period_end) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![pending.template_id,pending.period_index,id,no,pending.payload["issueDate"].as_str(),pending.start,pending.end]).map_err(invalid)?;
    tx.execute(
        "UPDATE recurring_invoice_templates SET next_issue_date=?1 WHERE id=?2",
        params![pending.next, pending.template_id],
    )
    .map_err(invalid)?;
    audit_with_principal(
        tx,
        pending.template_id,
        "recurring_invoice_generate",
        &format!(
            "Generated period {} as invoice {no}, original document {id}",
            pending.period_index
        ),
        actor,
        principal,
    )?;
    Ok(
        json!({"created":true,"templateId":pending.template_id,"periodIndex":pending.period_index,"documentId":id,"invoiceNumber":no,"issueDate":pending.payload["issueDate"],"dueDate":pending.payload["dueDate"],"deliveryPeriodStart":pending.start,"deliveryPeriodEnd":pending.end}),
    )
}
pub fn retire(tx: &Transaction<'_>, id: i64, i: &Retire, actor: &str) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    if !i.confirm || i.reason.as_ref().is_some_and(|s| s.len() > 4096) {
        return Err(refused("confirmation and bounded reason required"));
    }
    load(tx, id)?;
    tx.execute(
        "UPDATE recurring_invoice_templates SET active=0 WHERE id=?1",
        [id],
    )
    .map_err(invalid)?;
    audit(
        tx,
        id,
        "recurring_invoice_template_retire",
        i.reason.as_deref().unwrap_or("Retired by explicit action"),
        actor,
    )?;
    Ok(json!({"id":id,"retired":true}))
}
pub fn list(tx: &Transaction<'_>, slug: &str) -> Result<Value, Error> {
    let mut q = tx
        .prepare("SELECT id,notes,created_at FROM recurring_invoice_templates ORDER BY id")
        .map_err(invalid)?;
    let ids = q
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    let mut rows = vec![];
    for (id, notes, created) in ids {
        let t = load(tx, id)?;
        let mut q=tx.prepare("SELECT id,period_index,document_id,invoice_number,issue_date,delivery_period_start,delivery_period_end FROM recurring_invoice_generations WHERE template_id=?1 ORDER BY period_index").map_err(invalid)?;
        let generations=q.query_map([id],|r|Ok(json!({"id":r.get::<_,i64>(0)?,"periodIndex":r.get::<_,u32>(1)?,"documentId":r.get::<_,i64>(2)?,"invoiceNumber":r.get::<_,String>(3)?,"issueDate":r.get::<_,String>(4)?,"deliveryPeriodStart":r.get::<_,Option<String>>(5)?,"deliveryPeriodEnd":r.get::<_,Option<String>>(6)?}))).map_err(invalid)?.collect::<Result<Vec<_>,_>>().map_err(invalid)?;
        rows.push(json!({"id":t.id,"name":t.name,"interval":t.interval,"intervalCount":t.count,"deliveryChannel":t.channel,"firstIssueDate":t.first,"nextIssueDate":t.next,"paymentTermsDays":t.terms,"deliveryPeriodMode":t.mode,"notes":notes,"active":t.active,"createdAt":created,"generations":generations}));
    }
    Ok(json!({"slug":slug,"nativeRecurring":true,"templates":rows}))
}
/// Frozen specification for generation scheduling; excludes mutable next date/retirement.
pub fn scheduled_spec(tx: &Connection, id: i64) -> Result<Value, Error> {
    let t = load(tx, id)?;
    if t.channel != "manual" {
        return Err(refused("only manual generation supported"));
    }
    Ok(
        json!({"id":t.id,"active":t.active,"specification":{"id":t.id,"name":t.name,"interval":t.interval,"intervalCount":t.count,"firstIssueDate":t.first,"paymentTermsDays":t.terms,"deliveryPeriodMode":t.mode,"payload":t.payload}}),
    )
}
pub fn scheduled_date(tx: &Connection, id: i64, index: u32) -> Result<String, Error> {
    let t = load(tx, id)?;
    issue_date(&t.first, &t.interval, t.count, index)
}
pub fn scheduled_latest(tx: &Connection, id: i64, as_of: &str) -> Result<Option<u32>, Error> {
    let t = load(tx, id)?;
    if as_of < t.first.as_str() {
        Date::parse(as_of)?;
        return Ok(None);
    }
    period(&t, as_of).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cadence_is_anchored_and_bounded() {
        assert_eq!(
            window("interval_window", "2024-02-29", "2024-03-31").unwrap(),
            (Some("2024-02-29".into()), Some("2024-03-30".into()))
        );
        assert_eq!(
            issue_date("2024-01-31", "monthly", 1, 1).unwrap(),
            "2024-02-29"
        );
        assert_eq!(
            issue_date("2024-01-31", "monthly", 1, 2).unwrap(),
            "2024-03-31"
        );
        assert_eq!(
            issue_date("2024-02-29", "yearly", 1, 1).unwrap(),
            "2025-02-28"
        );
        assert_eq!(
            issue_date("2024-12-29", "weekly", 2, 1).unwrap(),
            "2025-01-12"
        );
        assert_eq!(
            issue_date("2024-01-31", "quarterly", 2, 1).unwrap(),
            "2024-07-31"
        );
        assert!(issue_date("9999-12-31", "monthly", 1, 1).is_err());
        assert!(issue_date("2024-01-01", "weekly", 0, 1).is_err());
    }
    #[test]
    fn immutable_templates_generate_once_without_advancing_preview_sequences() {
        let mut control = rusqlite::Connection::open_in_memory().unwrap();
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic Recurring Seller")
            .unwrap();
        let tx = db.transaction().unwrap();
        tx.execute_batch("UPDATE companies SET address='Synthetic1',cvr='87654321',vat_period_type='quarter' WHERE id=1;").unwrap();
        let customer: sales::Customer = serde_json::from_value(
            json!({"name":"Synthetic Buyer","address":"Synthetic2","vatOrCvr":"DK12345678"}),
        )
        .unwrap();
        let customer = sales::create_customer(&tx, &customer, "synthetic").unwrap()["id"]
            .as_i64()
            .unwrap();
        let mut input:Create=serde_json::from_value(json!({"name":"Synthetic subscription","interval":"monthly","intervalCount":1,"deliveryChannel":"manual","firstIssueDate":"2024-01-31","paymentTermsDays":14,"deliveryPeriodMode":"issue_month","vatRatePercent":25,"currency":"DKK","customerId":customer,"lines":[{"description":"Synthetic service","quantity":3,"unitPriceExVat":0.335}],"confirm":true})).unwrap();
        input.delivery_channel = Some("email".into());
        assert!(create(&tx, &input, "synthetic", "2024-02-29T12:00:00Z").is_err());
        input.delivery_channel = Some("manual".into());
        let template = create(&tx, &input, "synthetic", "2024-02-29T12:00:00Z").unwrap();
        let id = template["templateId"].as_i64().unwrap();
        assert_eq!(period(&load(&tx, id).unwrap(), "2024-02-28").unwrap(), 0);
        assert_eq!(period(&load(&tx, id).unwrap(), "2024-02-29").unwrap(), 1);
        assert!(
            tx.execute(
                "UPDATE recurring_invoice_templates SET payload_json='{}' WHERE id=?1",
                [id]
            )
            .is_err()
        );
        sales::update_customer(
            &tx,
            customer,
            &json!({"name":"Synthetic Updated Buyer"}),
            "synthetic",
        )
        .unwrap();
        let mut request: Generate =
            serde_json::from_value(json!({"asOfDate":"2024-01-30","confirm":true})).unwrap();
        assert!(prepare(&tx, id, &request, "2024-02-29", "2024-02-29T12:00:00Z").is_err());
        request.as_of_date = "2024-03-01".into();
        assert!(prepare(&tx, id, &request, "2024-02-29", "2024-02-29T12:00:00Z").is_err());
        request.as_of_date = "2024-02-29".into();
        let Prepared::Issue(pending) =
            prepare(&tx, id, &request, "2024-02-29", "2024-02-29T12:00:00Z").unwrap()
        else {
            panic!("not new")
        };
        assert_eq!(pending.period_index, 1);
        assert_eq!(pending.payload["invoiceNumber"], "2024-0001");
        assert_eq!(pending.payload["issueDate"], "2024-02-29");
        assert_eq!(pending.payload["dueDate"], "2024-03-14");
        assert_eq!(pending.payload["buyer"]["name"], "Synthetic Buyer");
        assert_eq!(pending.payload["totals"]["grossAmount"], 1.26);
        let issued = sales::register(
            &tx,
            &pending.payload,
            sales::Artifact {
                filename: "synthetic.json",
                path: "synthetic.json",
                hash: &"a".repeat(64),
                mime: "application/json",
            },
            sales::Artifact {
                filename: "synthetic.pdf",
                path: "synthetic.pdf",
                hash: &"b".repeat(64),
                mime: "application/pdf",
            },
            "synthetic",
        )
        .unwrap();
        let generated = finish(&tx, &pending, &issued, "synthetic").unwrap();
        let Prepared::Existing(replay) =
            prepare(&tx, id, &request, "2024-02-29", "2024-02-29T12:00:00Z").unwrap()
        else {
            panic!("duplicate issuance")
        };
        assert_eq!(replay["created"], false);
        assert_eq!(replay["documentId"], generated["documentId"]);
        assert!(finish(&tx, &pending, &issued, "synthetic").is_err());
        let listed = list(&tx, "synthetic").unwrap();
        assert_eq!(listed["templates"][0]["nextIssueDate"], "2024-03-31");
        assert_eq!(
            listed["templates"][0]["generations"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(
            tx.execute(
                "DELETE FROM recurring_invoice_generations WHERE template_id=?1",
                [id]
            )
            .is_err()
        );
        let retire_input = Retire {
            confirm: true,
            reason: Some("Synthetic retirement".into()),
        };
        retire(&tx, id, &retire_input, "synthetic").unwrap();
        assert!(prepare(&tx, id, &request, "2024-02-29", "2024-02-29T12:00:00Z").is_err());
        assert!(
            tx.execute(
                "UPDATE recurring_invoice_templates SET active=1 WHERE id=?1",
                [id]
            )
            .is_err()
        );
    }
}
