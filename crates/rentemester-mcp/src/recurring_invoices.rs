use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{
    ApiClient, Error, MAX_RECURRING_BODY, RecurringInvoiceMutation, recurring_invoice_body_schema,
};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn accepts(s: &str) -> bool {
    matches!(
        s,
        "recurring_invoices"
            | "create_recurring_invoice"
            | "generate_recurring_invoice"
            | "retire_recurring_invoice"
    )
}
pub fn tools() -> Vec<Value> {
    let mut tools = vec![tool(
        "recurring_invoices",
        "Read current manual recurring templates and generation evidence. CompanyRead; no query/selectors/confirmation or side effects.",
        json!({"type":"object","required":["company"],"properties":{"company":company_schema()},"additionalProperties":false}),
        true,
    )];
    for (name, op, id) in [
        (
            "create_recurring_invoice",
            RecurringInvoiceMutation::Create,
            false,
        ),
        (
            "generate_recurring_invoice",
            RecurringInvoiceMutation::Generate(1),
            true,
        ),
        (
            "retire_recurring_invoice",
            RecurringInvoiceMutation::Retire(1),
            true,
        ),
    ] {
        let mut body = native_body(recurring_invoice_body_schema(op));
        body["maxLength"] = json!(MAX_RECURRING_BODY);
        let mut props = json!({"company":company_schema(),"body":body,"confirm":{"const":true}});
        let mut required = vec!["company", "body", "confirm"];
        if id {
            props["id"] = json!({"type":"integer","minimum":1,"maximum":9007199254740991u64});
            required.push("id");
        }
        tools.push(tool(name,"Explicit manual recurring lifecycle under current CompanyDraftWrite. Exact native body max256KiB forwarded once, inner/outer confirm:true. No keys, timers, automatic retries or hidden posting. Server owns current eligibility and template/period dedup: explicit same-period generate returns same issued original while active, retired refuses even existing periods. Unknown outcome requires canonical recurring/invoices/documents reads before any further action; create has no dedup guarantee.",json!({"type":"object","required":required,"properties":props,"additionalProperties":false}),false));
    }
    tools
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Read {
    company: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Create {
    company: String,
    body: String,
    confirm: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Write {
    company: String,
    id: i64,
    body: String,
    confirm: bool,
}
pub async fn call(
    api: &ApiClient,
    name: &str,
    args: Value,
) -> Result<Result<Value, Error>, &'static str> {
    if name == "recurring_invoices" {
        let r: Read =
            serde_json::from_value(args).map_err(|_| "recurring_invoices requires only company")?;
        return Ok(api.recurring_invoices(&r.company).await);
    }
    let (company, body, confirmed, op) = if name == "create_recurring_invoice" {
        let r: Create = serde_json::from_value(args)
            .map_err(|_| "create requires only company/body/confirm")?;
        (
            r.company,
            r.body,
            r.confirm,
            RecurringInvoiceMutation::Create,
        )
    } else {
        let r: Write = serde_json::from_value(args)
            .map_err(|_| "generate/retire require only company/id/body/confirm")?;
        if !(1..=9_007_199_254_740_991).contains(&r.id) {
            return Err("positive exact template id required");
        }
        (
            r.company,
            r.body,
            r.confirm,
            if name == "generate_recurring_invoice" {
                RecurringInvoiceMutation::Generate(r.id)
            } else {
                RecurringInvoiceMutation::Retire(r.id)
            },
        )
    };
    if body.len() > MAX_RECURRING_BODY {
        return Err("recurring body exceeds native256KiB");
    }
    Ok(api
        .recurring_invoice_mutation(&company, op, body.into_bytes(), confirmed)
        .await)
}
