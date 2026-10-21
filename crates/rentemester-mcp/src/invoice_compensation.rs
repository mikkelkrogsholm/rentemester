use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{
    ApiClient, Error, InvoiceCompensationOperation as Op, MAX_INVOICE_COMPENSATION_BODY,
    invoice_compensation_body_schema,
};
use serde::Deserialize;
use serde_json::{Value, json};
fn operation(name: &str) -> Option<Op> {
    match name {
        "invoice_compensation_calc" => Some(Op::Calculate),
        "invoice_claim_compensation" => Some(Op::Register),
        "invoice_post_compensation" => Some(Op::Post),
        _ => None,
    }
}
pub fn accepts(name: &str) -> bool {
    operation(name).is_some()
}
pub fn tools() -> Vec<Value> {
    [("invoice_compensation_calc",Op::Calculate),("invoice_claim_compensation",Op::Register),("invoice_post_compensation",Op::Post)].into_iter().map(|(name,op)|{
        let read=op==Op::Calculate;let mut body=native_body(invoice_compensation_body_schema(op));body["maxLength"]=json!(MAX_INVOICE_COMPENSATION_BODY);
        let mut schema=json!({"type":"object","required":["company","body"],"properties":{"company":company_schema(),"body":body},"additionalProperties":false});
        if !read {schema["required"]=json!(["company","body","confirm"]);schema["properties"]["confirm"]=json!({"const":true});}
        let mut descriptor=tool(name,"Current human MFA cookie only (no service writer route). Exact native16KiB JSON once. Calculate: CompanyRead, no changes/confirm. Register: CompanyDraftWrite plus inner/outer confirm:true; one claim per invoice, no journal yet. Post: CompanyLedgerPost plus inner/outer confirm:true and explicit compatible active receivable/income accounts. Server owns eligibility/max310DKK/date/current source and period/backup gates. Principal remains in invoice currency; compensation and receipt DKK. No email, keys, inferred accounts or automatic retry; unknown write requires canonical invoice_reminder_claims/calculation and manual resolution.",schema,read);
        descriptor["annotations"]["openWorldHint"]=json!(false);descriptor
    }).collect()
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Read {
    company: String,
    body: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Write {
    company: String,
    body: String,
    confirm: bool,
}
pub async fn call(
    api: &ApiClient,
    name: &str,
    args: Value,
) -> Result<Result<Value, Error>, &'static str> {
    let op = operation(name).ok_or("unknown compensation operation")?;
    let (company, body, confirm) = if op == Op::Calculate {
        let i: Read =
            serde_json::from_value(args).map_err(|_| "calculate requires only company/body")?;
        (i.company, i.body, false)
    } else {
        let i: Write =
            serde_json::from_value(args).map_err(|_| "write requires company/body/confirm")?;
        (i.company, i.body, i.confirm)
    };
    if body.len() > MAX_INVOICE_COMPENSATION_BODY {
        return Err("compensation body exceeds native16KiB");
    }
    Ok(api
        .invoice_compensation(&company, op, body.into_bytes(), confirm)
        .await)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_calculation_is_read_only_and_writes_have_explicit_confirmation() {
        let tools = tools();
        assert_eq!(tools.len(), 3);
        for (i, t) in tools.iter().enumerate() {
            assert_eq!(t["annotations"]["readOnlyHint"], i == 0);
            assert_eq!(t["annotations"]["idempotentHint"], i == 0);
            assert_eq!(t["annotations"]["openWorldHint"], false);
            assert_eq!(t["inputSchema"]["additionalProperties"], false);
            assert_eq!(
                t["inputSchema"]["required"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("confirm")),
                i != 0
            );
        }
    }
}
