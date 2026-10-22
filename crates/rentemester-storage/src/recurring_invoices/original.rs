//! Original recurring inputs over the shared immutable template and generation tables.
use super::*;
use crate::{
    idempotency::PrincipalKind,
    invoice_lifecycle::{self, Kind},
};
const TEMPLATE: &str = "DK-RECURRING-INVOICE-TEMPLATE-001";
const GENERATE: &str = "DK-RECURRING-INVOICE-GENERATE-001";
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Create {
    pub name: String,
    pub interval: String,
    pub interval_count: Option<u32>,
    pub delivery_channel: Option<String>,
    pub first_issue_date: String,
    pub invoice: Value,
    pub payment_terms_days: Option<u32>,
    pub delivery_period_mode: Option<String>,
    pub notes: Option<String>,
    pub confirm: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Generate {
    pub template_id: i64,
    pub as_of_date: String,
    pub confirm: bool,
}
fn stripped(mut payload: Value) -> Result<Value, Error> {
    let map = payload
        .as_object_mut()
        .ok_or_else(|| refused("invoice payload is required"))?;
    for key in [
        "issueDate",
        "invoiceNumber",
        "dueDate",
        "deliveryDate",
        "deliveryPeriodStart",
        "deliveryPeriodEnd",
    ] {
        map.remove(key);
    }
    Ok(payload)
}
pub fn create(
    tx: &Transaction<'_>,
    input: &Create,
    actor: &str,
    principal: PrincipalKind,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    if !input.confirm {
        return Err(Error::new("confirmation_required", "confirm:true required"));
    }
    let name = rentemester_engine::sales_invoice::text(&input.name, 256)?;
    let count = input.interval_count.unwrap_or(1);
    let channel = input.delivery_channel.as_deref().unwrap_or("manual");
    let terms = input.payment_terms_days.unwrap_or(30);
    let mode = input
        .delivery_period_mode
        .as_deref()
        .unwrap_or("issue_month");
    let mut errors = vec![];
    if months(&input.interval).is_err() {
        errors.push("interval must be one of weekly, monthly, quarterly, yearly".to_owned());
    }
    if !(1..=120).contains(&count) {
        errors.push("intervalCount must be a positive integer no greater than 120".into());
    }
    if !["manual", "email", "digisense"].contains(&channel) {
        errors.push("deliveryChannel must be one of manual, email, digisense".into());
    }
    if !valid_iso_date(&input.first_issue_date) {
        errors.push("firstIssueDate must be a YYYY-MM-DD date".into());
    }
    if !["issue_month", "interval_window", "none"].contains(&mode) {
        errors.push("deliveryPeriodMode must be one of issue_month, interval_window, none".into());
    }
    if terms > 365 {
        errors.push("paymentTermsDays must be an integer between 0 and 365 when present".into());
    }
    if input.notes.as_ref().is_some_and(|v| v.len() > 4096) {
        errors.push("notes exceeds4096 bytes".into());
    }
    if !errors.is_empty() {
        return Ok(json!({"ok":false,"appliedRules":[TEMPLATE],"errors":errors}));
    }
    let stored = stripped(input.invoice.clone())?;
    let mut probe = stored.clone();
    probe["issueDate"] = input
        .invoice
        .get("issueDate")
        .cloned()
        .unwrap_or_else(|| json!(input.first_issue_date));
    let verdict = rentemester_engine::invoice_validation::validate(&probe);
    if verdict["ok"] != true {
        let mut rules = vec![json!(TEMPLATE)];
        if let Some(extra) = verdict["appliedRules"].as_array() {
            rules.extend(extra.iter().cloned());
        }
        return Ok(json!({"ok":false,"appliedRules":rules,"errors":verdict["errors"]}));
    }
    // Apply the original issuance schema before persisting an immutable specification.
    invoice_lifecycle::validate(Kind::Issue, &json!({"payload":probe,"confirm":true}))?;
    let id:i64=tx.query_row("INSERT INTO recurring_invoice_templates(name,interval,interval_count,delivery_channel,first_issue_date,next_issue_date,payment_terms_days,delivery_period_mode,payload_json,notes) VALUES(?1,?2,?3,?4,?5,?5,?6,?7,?8,?9) RETURNING id",params![name,input.interval,count,channel,input.first_issue_date,terms,mode,stored.to_string(),input.notes.as_ref().map(|s|s.trim()).filter(|s|!s.is_empty())],|r|r.get(0)).map_err(invalid)?;
    audit_with_principal(
        tx,
        id,
        "recurring_invoice_template_create",
        &format!(
            "Created recurring invoice template {id} {name} ({})",
            input.interval
        ),
        actor,
        principal,
    )?;
    Ok(json!({"ok":true,"templateId":id,"appliedRules":[TEMPLATE],"errors":[]}))
}
pub fn list(tx: &Transaction<'_>, slug: &str, include_inactive: bool) -> Result<Value, Error> {
    let native = super::list(tx, slug)?;
    let rows = native["templates"]
        .as_array()
        .ok_or_else(|| refused("template list unavailable"))?
        .iter()
        .filter(|v| include_inactive || v["active"] == true)
        .map(|v| {
            let mut v = v.clone();
            v.as_object_mut().unwrap().remove("generations");
            v
        })
        .collect::<Vec<_>>();
    Ok(json!({"ok":true,"count":rows.len(),"rows":rows,"errors":[]}))
}
/// Uses the existing date, number, allocation and immutable issuance engine. No transport occurs here.
pub fn prepare(
    tx: &Transaction<'_>,
    input: &Generate,
    today: &str,
    now: &str,
) -> Result<Prepared, Error> {
    prepare_period(tx, input, today, now, None)
}
/// Internal runner selector; validated against the same due boundary, never a public decoder.
pub fn prepare_period(
    tx: &Transaction<'_>,
    input: &Generate,
    today: &str,
    now: &str,
    selected: Option<u32>,
) -> Result<Prepared, Error> {
    crate::journal_post::require_current(tx)?;
    if !input.confirm
        || input.template_id <= 0
        || input.template_id > 9_007_199_254_740_991
        || Date::parse(&input.as_of_date)? > Date::parse(today)?
    {
        return Err(refused(
            "positive templateId and confirmed asOfDate no later than today required",
        ));
    }
    let t = load(tx, input.template_id)?;
    if !t.active {
        return Err(refused("recurring template is retired"));
    }
    let latest = period(&t, &input.as_of_date)?;
    let index = selected.unwrap_or(latest);
    if index > latest {
        return Err(refused("periodIndex must be a due non-negative integer"));
    }
    // Native templates continue through their existing input and calendar contract.
    if t.payload.get("invoiceType").is_none() {
        return super::prepare(
            tx,
            input.template_id,
            &super::Generate {
                as_of_date: if selected.is_some() {
                    issue_date(&t.first, &t.interval, t.count, index)?
                } else {
                    input.as_of_date.clone()
                },
                confirm: true,
            },
            today,
            now,
        );
    }
    let existing:Option<Value>=tx.query_row("SELECT g.document_id,g.invoice_number,g.issue_date,g.delivery_period_start,g.delivery_period_end,d.payload_json FROM recurring_invoice_generations g JOIN documents d ON d.id=g.document_id AND d.document_type='issued_invoice' WHERE g.template_id=?1 AND g.period_index=?2",params![t.id,index],|r|{
        let payload:String=r.get(5)?;let payload:Value=serde_json::from_str(&payload).map_err(|_|rusqlite::Error::InvalidQuery)?;
        let mut value=json!({"created":false,"templateId":t.id,"periodIndex":index,"documentId":r.get::<_,i64>(0)?,"invoiceNumber":r.get::<_,String>(1)?,"issueDate":r.get::<_,String>(2)?,"dueDate":payload["dueDate"]});
        for (key,col) in [("deliveryPeriodStart",3),("deliveryPeriodEnd",4)] { if let Some(s)=r.get::<_,Option<String>>(col)? { value[key]=json!(s); } }
        Ok(value)
    }).optional().map_err(invalid)?;
    if let Some(value) = existing {
        return Ok(Prepared::Existing(value));
    }
    let claimed:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM recurring_invoice_generation_claims WHERE template_id=?1 AND period_index=?2)",params![t.id,index],|r|r.get(0)).map_err(invalid)?;
    if claimed {
        return Err(refused(
            "generation claim exists without invoice evidence; reconcile before retrying",
        ));
    }
    let issue = issue_date(&t.first, &t.interval, t.count, index)?;
    let next = issue_date(&t.first, &t.interval, t.count, index + 1)?;
    let (start, end) = match t.mode.as_str() {
        "none" => (None, None),
        "issue_month" => window(&t.mode, &issue, &next)?,
        "interval_window" => (
            Some(issue.clone()),
            Some(if t.interval == "weekly" {
                add_days(&issue, i64::from(t.count) * 7)?
            } else {
                add_months(&issue, months(&t.interval)? * t.count)?
            }),
        ),
        _ => return Err(refused("unsupported delivery period mode")),
    };
    let mut source = stripped(t.payload)?;
    source["issueDate"] = json!(issue);
    source["dueDate"] = json!(add_days(&issue, i64::from(t.terms))?);
    if let Some(s) = &start {
        source["deliveryPeriodStart"] = json!(s);
    }
    if let Some(s) = &end {
        source["deliveryPeriodEnd"] = json!(s);
    }
    let body = json!({"payload":source,"confirm":true});
    invoice_lifecycle::validate(Kind::Issue, &body)?;
    let (mut payload, _, _) = invoice_lifecycle::prepare(tx, Kind::Issue, &body, now)?;
    payload["recurringTemplateId"] = json!(t.id);
    payload["recurringPeriodIndex"] = json!(index);
    tx.execute(
        "INSERT INTO recurring_invoice_generation_claims(template_id,period_index) VALUES(?1,?2)",
        params![t.id, index],
    )
    .map_err(invalid)?;
    Ok(Prepared::Issue(Pending {
        template_id: t.id,
        period_index: index,
        payload,
        next: next.max(t.next),
        start,
        end,
    }))
}
pub fn generation_result(mut value: Value, payload: Option<&Value>) -> Value {
    let map = value.as_object_mut().unwrap();
    for key in ["deliveryPeriodStart", "deliveryPeriodEnd"] {
        if map.get(key).is_some_and(Value::is_null) {
            map.remove(key);
        }
    }
    value["ok"] = json!(true);
    let mut rules = vec![json!(GENERATE)];
    if let Some(payload) = payload {
        if let Some(extra) =
            rentemester_engine::invoice_validation::validate(payload)["appliedRules"].as_array()
        {
            rules.extend(extra.iter().cloned());
        }
        rules.push(json!("DK-INVOICE-ISSUE-001"));
        rules.push(json!("DK-INVOICE-LOCK-001"));
    }
    value["appliedRules"] = json!(rules);
    value["errors"] = json!([]);
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::TransactionBehavior;
    fn fixture() -> Connection {
        let mut control = Connection::open_in_memory().unwrap();
        let mut db = Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic recurring seller")
            .unwrap();
        db.execute_batch("UPDATE companies SET address='Synthetic1',cvr='87654321',vat_period_type='quarter' WHERE id=1").unwrap();
        db
    }
    fn input() -> Create {
        serde_json::from_value(json!({"name":"Synthetic original subscription","interval":"monthly","firstIssueDate":"2024-01-31","deliveryPeriodMode":"interval_window","invoice":{"invoiceType":"full","seller":{"name":"Frozen synthetic seller","address":"Frozen1","vatOrCvr":"DK87654321"},"buyer":{"name":"Frozen synthetic buyer","address":"Frozen2","vatOrCvr":"DK12345678"},"lines":[{"description":"Synthetic service","quantity":2,"unitPriceExVat":50,"lineTotalExVat":100}],"totals":{"netAmount":100,"vatRate":0.25,"vatAmount":25,"grossAmount":125}},"confirm":true})).unwrap()
    }
    #[test]
    fn original_structured_defaults_source_window_and_native_input_share_one_engine() {
        let mut db = fixture();
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let i = input();
        let before = crate::sequences::current(&tx, "issued_invoice", "no-scope", 0).unwrap();
        let id = create(&tx, &i, "agent", PrincipalKind::ServiceAccount).unwrap()["templateId"]
            .as_i64()
            .unwrap();
        let t = load(&tx, id).unwrap();
        assert_eq!(t.channel, "manual");
        assert_eq!(t.terms, 30);
        assert!(t.payload.get("issueDate").is_none());
        assert_eq!(
            before,
            crate::sequences::current(&tx, "issued_invoice", "no-scope", 0).unwrap()
        );
        assert_eq!(list(&tx, "a", false).unwrap()["count"], 1);
        let request = Generate {
            template_id: id,
            as_of_date: "2024-02-29".into(),
            confirm: true,
        };
        let Prepared::Issue(pending) =
            prepare(&tx, &request, "2024-03-31", "2024-03-31T12:00:00Z").unwrap()
        else {
            panic!("new period")
        };
        assert_eq!(pending.period_index, 1);
        assert_eq!(pending.payload["issueDate"], "2024-02-29");
        assert_eq!(pending.payload["dueDate"], "2024-03-30");
        assert_eq!(pending.payload["deliveryPeriodEnd"], "2024-03-29");
        assert_eq!(pending.payload["seller"]["name"], "Frozen synthetic seller");
        assert_eq!(pending.payload["totals"]["grossAmount"], 125.0);
        assert_eq!(
            tx.query_row(
                "SELECT count(*) FROM recurring_invoice_generation_claims",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        let actor: String = tx
            .query_row(
                "SELECT actor FROM audit_log WHERE event_type='recurring_invoice_template_create'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(actor, "service_account:agent");
        tx.rollback().unwrap();
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM recurring_invoice_generation_claims",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
    }
    #[test]
    fn source_validation_and_tax_profiles_do_not_silently_downgrade() {
        let mut db = fixture();
        for (kind, invoice) in [
            (
                "mixed",
                json!({"invoiceType":"full","seller":{"name":"Seller","address":"S1","vatOrCvr":"DK87654321"},"buyer":{"name":"Buyer","address":"B1"},"lines":[{"description":"Taxed","quantity":1,"unitPriceExVat":100,"lineTotalExVat":100,"taxClassification":"taxable","vatRate":0.25},{"description":"Exempt","quantity":1,"unitPriceExVat":50,"lineTotalExVat":50,"taxClassification":"exempt"}],"totals":{"netAmount":150,"vatAmount":25,"grossAmount":175}}),
            ),
            (
                "reduced",
                json!({"invoiceType":"simplified","seller":{"name":"Synthetic reduced seller","address":"S1","vatOrCvr":"DK87654321"},"lines":[{"description":"Synthetic reduced service"}],"totals":{"grossAmount":125,"vatRate":0.25,"vatAmount":25}}),
            ),
            (
                "reverse",
                json!({"invoiceType":"full","vatTreatment":"domestic_reverse_charge","reverseChargeBasis":"DK_MOMSLOVEN_§46_STK_1_NR_6","reverseChargeNote":"Synthetic customer accounts for VAT","seller":{"name":"Seller","address":"S1","vatOrCvr":"DK87654321"},"buyer":{"name":"Buyer","address":"B1","vatOrCvr":"DK12345678"},"lines":[{"description":"Reverse","quantity":1,"unitPriceExVat":100,"lineTotalExVat":100}],"totals":{"netAmount":100,"grossAmount":100}}),
            ),
            (
                "foreign",
                json!({"invoiceType":"full","currency":"EUR","seller":{"name":"Seller","address":"S1","vatOrCvr":"DK87654321"},"buyer":{"name":"Buyer","address":"B1"},"lines":[{"description":"Synthetic foreign","quantity":1,"unitPriceExVat":100,"lineTotalExVat":100}],"totals":{"netAmount":100,"vatRate":0.25,"vatAmount":25,"grossAmount":125,"fxRateToDkk":7.5,"netAmountDkk":750,"vatAmountDkk":187.5,"grossAmountDkk":937.5}}),
            ),
        ] {
            let tx = db
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            let mut i = input();
            i.invoice = invoice;
            i.delivery_channel = Some("email".into());
            let created = create(&tx, &i, "u", PrincipalKind::User).unwrap();
            assert_eq!(created["ok"], true, "{kind}: {created}");
            let id = created["templateId"].as_i64().unwrap();
            let Prepared::Issue(pending) = prepare(
                &tx,
                &Generate {
                    template_id: id,
                    as_of_date: "2024-01-31".into(),
                    confirm: true,
                },
                "2024-02-29",
                "2024-02-29T12:00:00Z",
            )
            .unwrap_or_else(|e| panic!("{kind}: {}", e.message)) else {
                panic!("new")
            };
            match kind {
                "mixed" => {
                    assert_eq!(pending.payload["totals"]["grossAmount"], 175.0);
                    assert_eq!(pending.payload["lines"][1]["taxClassification"], "exempt");
                }
                "reduced" => assert_eq!(pending.payload["invoiceType"], "simplified"),
                "reverse" => assert_eq!(pending.payload["vatTreatment"], "domestic_reverse_charge"),
                "foreign" => {
                    assert_eq!(pending.payload["currency"], "EUR");
                    assert_eq!(pending.payload["totals"]["grossAmountDkk"], 937.5);
                }
                _ => unreachable!(),
            }
            tx.rollback().unwrap();
        }
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let mut i = input();
        i.invoice["totals"]["grossAmount"] = json!(126);
        assert_eq!(
            create(&tx, &i, "u", PrincipalKind::User).unwrap()["ok"],
            false
        );
        assert_eq!(
            tx.query_row(
                "SELECT count(*) FROM recurring_invoice_templates",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
    }
    #[test]
    fn source_twenty_percent_template_is_retained_but_native_generation_refuses_without_effects() {
        let mut db = fixture();
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let mut i = input();
        i.invoice["totals"] =
            json!({"netAmount":100,"vatRate":0.2,"vatAmount":20,"grossAmount":120});
        let created = create(&tx, &i, "u", PrincipalKind::User).unwrap();
        assert_eq!(created["ok"], true);
        let error = match prepare(
            &tx,
            &Generate {
                template_id: created["templateId"].as_i64().unwrap(),
                as_of_date: "2024-01-31".into(),
                confirm: true,
            },
            "2024-02-29",
            "2024-02-29T12:00:00Z",
        ) {
            Err(error) => error,
            Ok(_) => panic!("20 percent must not be silently issued at25 percent"),
        };
        assert!(error.message.contains("25% VAT"));
        for table in [
            "recurring_invoice_generation_claims",
            "recurring_invoice_generations",
            "documents",
            "journal_entries",
        ] {
            assert_eq!(
                tx.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                0
            );
        }
        assert_eq!(
            tx.query_row("SELECT count(*) FROM sequences", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}
