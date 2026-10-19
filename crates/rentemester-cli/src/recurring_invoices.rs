use rentemester_client::{
    ApiClient, MAX_RECURRING_BODY, RecurringInvoiceMutation, recurring_invoice_body_schema,
};
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
type Failure = (u8, Error);
fn invalid(s: &str) -> Failure {
    (2, Error::new("invalid_input", s))
}
pub fn accepts(s: &str) -> bool {
    matches!(
        s,
        "recurring-invoices"
            | "create-recurring-invoice"
            | "generate-recurring-invoice"
            | "retire-recurring-invoice"
    )
}
pub fn help() -> Value {
    json!({"usage":"recurring-invoices --company SLUG; create-recurring-invoice --company SLUG --confirm < native.json; generate-recurring-invoice/retire-recurring-invoice additionally --id TEMPLATE_ID. Explicit --server and private --cookie-file required. Native JSON max256KiB.","policy":"Current CompanyRead for list; CompanyDraftWrite for mutations. Manual generation only, no timers or hidden posting. Server determines eligible periods/current customer and creates immutable issued original/PDF. Generate explicit same template/asOfDate returns same invoice only under current server gates while active; retired refuses even existing period. No caller idempotency keys or automatic retries. On outcome_unknown read recurring-invoices/invoices/documents before any explicit action; create has no dedup promise.","bodySchemas":{"create":recurring_invoice_body_schema(RecurringInvoiceMutation::Create),"generate":recurring_invoice_body_schema(RecurringInvoiceMutation::Generate(1)),"retire":recurring_invoice_body_schema(RecurringInvoiceMutation::Retire(1))}})
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow: bool,
    confirmed: bool,
    preview: bool,
) -> Result<Value, Failure> {
    let read = command == "recurring-invoices";
    let needs_id = matches!(
        command,
        "generate-recurring-invoice" | "retire-recurring-invoice"
    );
    if confirmed == read
        || preview
        || flags.keys().any(|k| {
            !(["--server", "--cookie-file", "--company"].contains(k) || needs_id && *k == "--id")
        })
    {
        return Err(invalid(
            "recurring list has no selectors/confirmation; writes require --confirm, generate/retire require --id only",
        ));
    }
    let id = if needs_id {
        flags
            .get("--id")
            .and_then(|s| s.parse::<i64>().ok())
            .filter(|n| (1..=9_007_199_254_740_991).contains(n))
            .ok_or_else(|| invalid("positive exact --id required"))?
    } else {
        0
    };
    let company = flags
        .get("--company")
        .ok_or_else(|| invalid("recurring invoices require --company"))?;
    let server = flags
        .get("--server")
        .ok_or_else(|| invalid("explicit --server required"))?;
    let cookie = flags
        .get("--cookie-file")
        .ok_or_else(|| invalid("private --cookie-file required"))?;
    let cookie = rentemester_client::read_cookie_file(std::path::Path::new(cookie))
        .map_err(crate::remote::client_error)?;
    let api = ApiClient::new(server, allow, Some(cookie)).map_err(crate::remote::client_error)?;
    let mut bytes = vec![];
    if !read {
        std::io::stdin()
            .take(MAX_RECURRING_BODY as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid("could not read exact recurring body"))?;
    }
    let runtime =
        tokio::runtime::Runtime::new().map_err(|_| invalid("could not start HTTP runtime"))?;
    let result = runtime
        .block_on(async {
            if read {
                api.recurring_invoices(company).await
            } else {
                api.recurring_invoice_mutation(
                    company,
                    match command {
                        "create-recurring-invoice" => RecurringInvoiceMutation::Create,
                        "generate-recurring-invoice" => RecurringInvoiceMutation::Generate(id),
                        _ => RecurringInvoiceMutation::Retire(id),
                    },
                    bytes,
                    confirmed,
                )
                .await
            }
        })
        .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":result.get("ok").and_then(Value::as_bool).unwrap_or(true),"data":result}))
}
